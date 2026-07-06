use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use pdf_folio_db::{Db, SyncEntryFolderRow, SyncEntryRow, SyncFolderRow};

use crate::r2_client::R2Client;
use crate::session::Session;
use crate::turso_client::{TursoClient, TursoRemote, TursoValue};

/// Sync-visible app library profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncLibraryRow {
    /// Stable library id from the PDF-Folio library registry.
    pub id: String,
    /// User-visible library name.
    pub name: String,
    /// Last local update timestamp as a Unix timestamp.
    pub updated_at: i64,
    /// Tombstone timestamp, when deleted.
    pub deleted_at: Option<i64>,
}

/// Durable sync checkpoint for one device/library pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCheckpoint {
    /// Local library identifier.
    pub library_id: String,
    /// Stable device identifier.
    pub device_id: String,
    /// Last fully completed sync time.
    pub last_synced_at: DateTime<Utc>,
}

/// Counts of local work a future push/pull pass should perform.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlan {
    /// Local entries newer than the checkpoint.
    pub entries_to_push: usize,
    /// Local folders newer than the checkpoint.
    pub folders_to_push: usize,
    /// Entry-folder memberships newer than the checkpoint.
    pub memberships_to_push: usize,
}

/// High-level sync coordinator.
#[derive(Debug, Clone)]
pub struct SyncClient {
    /// Session used for all control-plane calls.
    pub session: Session,
    /// Turso credential client.
    pub turso: TursoClient,
    /// R2 blob client.
    pub r2: R2Client,
}

impl SyncClient {
    /// Creates a sync coordinator from a cached session.
    pub fn new(session: Session) -> Self {
        Self {
            turso: TursoClient::new(session.clone()),
            r2: R2Client::new(session.clone()),
            session,
        }
    }

    /// Returns a lightweight push plan from local sync tracking tables.
    ///
    /// # Errors
    ///
    /// Returns an error when local sync metadata cannot be queried.
    pub fn plan_push(&self, db: &Db, library_id: &str, device_id: &str) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?;
        let since = checkpoint.unwrap_or(0);
        Ok(SyncPlan {
            entries_to_push: db.sync_entries_updated_since(library_id, since)?.len(),
            folders_to_push: db.sync_folders_updated_since(library_id, since)?.len(),
            memberships_to_push: db
                .sync_entry_folders_updated_since(library_id, since)?
                .len(),
        })
    }

    /// Pushes app-level library records to Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be written.
    pub async fn push_libraries(&self, libraries: &[SyncLibraryRow]) -> Result<usize> {
        if libraries.is_empty() {
            return Ok(0);
        }
        let remote = self.turso.remote().await?;
        upsert_remote_libraries(&remote, libraries).await?;
        Ok(libraries.len())
    }

    /// Pulls all non-deleted app-level library records from Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be read.
    pub async fn pull_libraries(&self) -> Result<Vec<SyncLibraryRow>> {
        let remote = self.turso.remote().await?;
        remote_libraries(&remote).await
    }

    /// Creates the remote Turso schema if it does not already exist.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote database cannot execute the schema batch.
    pub async fn ensure_remote_schema(&self) -> Result<()> {
        let remote = self.turso.remote().await?;
        remote
            .execute_batch(include_str!("../turso_schema.sql"))
            .await
            .context("Could not create PDF-Folio sync schema in Turso.")?;
        Ok(())
    }

    /// Pushes local metadata rows newer than the current checkpoint to Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when local rows cannot be read, remote rows cannot be written,
    /// or the local checkpoint cannot be saved.
    pub async fn push_local_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?.unwrap_or(0);
        let entries = db.sync_entries_updated_since(library_id, checkpoint)?;
        let folders = db.sync_folders_updated_since(library_id, checkpoint)?;
        let memberships = db.sync_entry_folders_updated_since(library_id, checkpoint)?;
        let plan = SyncPlan {
            entries_to_push: entries.len(),
            folders_to_push: folders.len(),
            memberships_to_push: memberships.len(),
        };
        if entries.is_empty() && folders.is_empty() && memberships.is_empty() {
            return Ok(plan);
        }

        let remote = self.turso.remote().await?;
        upsert_remote_entries(&remote, &entries).await?;
        upsert_remote_folders(&remote, &folders).await?;
        upsert_remote_entry_folders(&remote, library_id, &memberships).await?;
        let last_synced_at = entries
            .iter()
            .map(|row| row.updated_at)
            .chain(folders.iter().map(|row| row.updated_at))
            .chain(memberships.iter().map(|row| row.updated_at))
            .max()
            .unwrap_or(checkpoint);
        db.set_sync_checkpoint(library_id, device_id, last_synced_at)?;
        Ok(plan)
    }

    /// Pulls remote metadata newer than the local checkpoint into local sync tables.
    ///
    /// This records remote state and tombstones locally; downloading unknown PDF blobs
    /// and creating full local `entries` rows is intentionally a higher-level step
    /// because it needs a destination library path.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be read or local sync rows cannot be saved.
    pub async fn pull_remote_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?.unwrap_or(0);
        let remote = self.turso.remote().await?;
        let entries = remote_entries_updated_since(&remote, library_id, checkpoint).await?;
        let folders = remote_folders_updated_since(&remote, library_id, checkpoint).await?;
        let memberships =
            remote_entry_folders_updated_since(&remote, library_id, checkpoint).await?;

        for row in &entries {
            db.upsert_sync_entry(row)?;
        }
        for row in &folders {
            db.upsert_sync_folder(row)?;
        }
        for row in &memberships {
            db.upsert_sync_entry_folder(row)?;
        }
        let last_synced_at = entries
            .iter()
            .map(|row| row.updated_at)
            .chain(folders.iter().map(|row| row.updated_at))
            .chain(memberships.iter().map(|row| row.updated_at))
            .max()
            .unwrap_or(checkpoint);
        db.set_sync_checkpoint(library_id, device_id, last_synced_at)?;
        Ok(SyncPlan {
            entries_to_push: entries.len(),
            folders_to_push: folders.len(),
            memberships_to_push: memberships.len(),
        })
    }
}

async fn upsert_remote_libraries(remote: &TursoRemote, rows: &[SyncLibraryRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO libraries
                    (id, name, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.name.as_str()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote library metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_entries(remote: &TursoRemote, rows: &[SyncEntryRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_entries
                    (id, library_id, title, author, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(library_id, id) DO UPDATE SET
                    title = excluded.title,
                    author = excluded.author,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.library_id.as_str()),
                    TursoValue::nullable_text(row.title.as_deref()),
                    TursoValue::nullable_text(row.author.as_deref()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote entry metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_folders(remote: &TursoRemote, rows: &[SyncFolderRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_folders
                    (id, library_id, name, parent_id, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(library_id, id) DO UPDATE SET
                    name = excluded.name,
                    parent_id = excluded.parent_id,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.library_id.as_str()),
                    TursoValue::text(row.name.as_str()),
                    TursoValue::nullable_text(row.parent_id.as_ref().map(|id| id.as_str())),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote folder metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_entry_folders(
    remote: &TursoRemote,
    library_id: &str,
    rows: &[SyncEntryFolderRow],
) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_entry_folders
                    (library_id, entry_id, folder_id, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(library_id, entry_id, folder_id) DO UPDATE SET
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(library_id),
                    TursoValue::text(row.entry_id.as_str()),
                    TursoValue::text(row.folder_id.as_str()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote entry-folder metadata.")?;
    }
    Ok(())
}

async fn remote_libraries(remote: &TursoRemote) -> Result<Vec<SyncLibraryRow>> {
    let rows = remote
        .query(
            "SELECT id, name, updated_at, deleted_at
             FROM libraries
             WHERE deleted_at IS NULL
             ORDER BY lower(name) ASC, id ASC",
            vec![],
        )
        .await
        .context("Could not query remote libraries.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncLibraryRow {
            id: row[0].as_string()?,
            name: row[1].as_string()?,
            updated_at: row[2].as_i64()?,
            deleted_at: row[3].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_entries_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncEntryRow>> {
    let rows = remote
        .query(
            "SELECT id, library_id, title, author, updated_at, deleted_at
             FROM library_entries
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote entry metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncEntryRow {
            id: pdf_folio_db::EntryId::new(row[0].as_string()?),
            library_id: row[1].as_string()?,
            title: row[2].as_optional_string()?,
            author: row[3].as_optional_string()?,
            updated_at: row[4].as_i64()?,
            deleted_at: row[5].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_folders_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncFolderRow>> {
    let rows = remote
        .query(
            "SELECT id, library_id, name, parent_id, updated_at, deleted_at
             FROM library_folders
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote folder metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncFolderRow {
            id: pdf_folio_db::FolderId::new(row[0].as_string()?),
            library_id: row[1].as_string()?,
            name: row[2].as_string()?,
            parent_id: row[3]
                .as_optional_string()?
                .map(pdf_folio_db::FolderId::new),
            updated_at: row[4].as_i64()?,
            deleted_at: row[5].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_entry_folders_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncEntryFolderRow>> {
    let rows = remote
        .query(
            "SELECT ef.entry_id, ef.folder_id, ef.updated_at, ef.deleted_at
             FROM library_entry_folders ef
             WHERE ef.library_id = ?1 AND ef.updated_at > ?2
             ORDER BY ef.updated_at ASC, ef.entry_id ASC, ef.folder_id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote entry-folder metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncEntryFolderRow {
            entry_id: pdf_folio_db::EntryId::new(row[0].as_string()?),
            folder_id: pdf_folio_db::FolderId::new(row[1].as_string()?),
            updated_at: row[2].as_i64()?,
            deleted_at: row[3].as_optional_i64()?,
        });
    }
    Ok(output)
}
