//! Library storage, search indexing, import, and filesystem watching for
//! PDF-Folio.
//!
//! This module manages all persistent data for the user's PDF library:
//!
//! - [`Db`] is the SQLite handle for entries, folders, tags, library
//!   preferences, and the raindrop/sync mapping tables.
//! - [`import`] scans directories for PDF files, hashes them with BLAKE3,
//!   imports them into the database, manages thumbnail cache paths, and
//!   watches configured folders via [`import::LibraryWatcher`].
//! - [`search`] wraps [`tantivy`] to build a full-text search index over
//!   extracted PDF text content ([`search::SearchIndex`],
//!   [`search::SearchHit`]).
//!
//! [`tantivy`]: https://docs.rs/tantivy

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

mod naming;

use naming::{
    clean_folder_name, clean_optional_text, clean_title_sort_key, next_folder_suffix, sort_key,
    MANUAL_ORDER_GAP,
};

mod types;
pub use types::*;

pub mod import;
pub mod search;

/// SQLite-backed PDF-Folio library database.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
}

impl Db {
    /// Opens the default library database under the XDG data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when an XDG project directory cannot be resolved, the database directory
    /// cannot be created, or SQLite cannot open or migrate the database.
    pub fn open_default() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
            .context("Could not find a data directory for PDF-Folio.")?;
        let data_dir = project_dirs.data_dir();
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Could not create data directory: {}.", data_dir.display()))?;
        Self::open(data_dir.join("library.db"))
    }

    /// Opens a library database at `path` and runs migrations.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot open or migrate the database.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db = Self { path: path.into() };
        db.migrate()?;
        Ok(db)
    }

    /// Returns the database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Records sync metadata for a local entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the sync row.
    pub fn upsert_sync_entry(&self, row: &SyncEntryRow) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_entries
                (id, library_id, title, author, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id, library_id) DO UPDATE SET
                title = excluded.title,
                author = excluded.author,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at",
            params![
                row.id.as_str(),
                row.library_id,
                row.title,
                row.author,
                row.updated_at,
                row.deleted_at,
            ],
        )?;
        Ok(())
    }

    /// Records sync metadata for a local folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the sync row.
    pub fn upsert_sync_folder(&self, row: &SyncFolderRow) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_folders
                (id, library_id, name, parent_id, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id, library_id) DO UPDATE SET
                name = excluded.name,
                parent_id = excluded.parent_id,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at",
            params![
                row.id.as_str(),
                row.library_id,
                row.name,
                row.parent_id.as_ref().map(FolderId::as_str),
                row.updated_at,
                row.deleted_at,
            ],
        )?;
        Ok(())
    }

    /// Records sync metadata for a local entry-folder membership.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the sync row.
    pub fn upsert_sync_entry_folder(&self, row: &SyncEntryFolderRow) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_entry_folders
                (entry_id, folder_id, updated_at, deleted_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(entry_id, folder_id) DO UPDATE SET
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at",
            params![
                row.entry_id.as_str(),
                row.folder_id.as_str(),
                row.updated_at,
                row.deleted_at,
            ],
        )?;
        Ok(())
    }

    /// Returns sync entry rows newer than `since`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_entries_updated_since(
        &self,
        library_id: &str,
        since: i64,
    ) -> Result<Vec<SyncEntryRow>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, library_id, title, author, updated_at, deleted_at
             FROM sync_entries
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
        )?;
        let rows = statement.query_map(params![library_id, since], |row| {
            Ok(SyncEntryRow {
                id: EntryId::new(row.get::<_, String>(0)?),
                library_id: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                updated_at: row.get(4)?,
                deleted_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load sync entry rows.")
    }

    /// Returns all sync entry rows for one library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_entries_for_library(&self, library_id: &str) -> Result<Vec<SyncEntryRow>> {
        self.sync_entries_updated_since(library_id, i64::MIN)
    }

    /// Returns sync entry rows that may need a local library row or blob relink.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_entries_needing_hydration(&self, library_id: &str) -> Result<Vec<SyncEntryRow>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT s.id, s.library_id, s.title, s.author, s.updated_at, s.deleted_at
             FROM sync_entries s
             LEFT JOIN entries e ON e.id = s.id
             WHERE s.library_id = ?1
               AND s.deleted_at IS NULL
               AND (e.id IS NULL OR e.missing != 0)
             ORDER BY s.updated_at ASC, s.id ASC",
        )?;
        let rows = statement.query_map(params![library_id], |row| {
            Ok(SyncEntryRow {
                id: EntryId::new(row.get::<_, String>(0)?),
                library_id: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                updated_at: row.get(4)?,
                deleted_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load sync entry rows needing hydration.")
    }

    /// Returns sync folder rows newer than `since`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_folders_updated_since(
        &self,
        library_id: &str,
        since: i64,
    ) -> Result<Vec<SyncFolderRow>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, library_id, name, parent_id, updated_at, deleted_at
             FROM sync_folders
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
        )?;
        let rows = statement.query_map(params![library_id, since], |row| {
            Ok(SyncFolderRow {
                id: FolderId::new(row.get::<_, String>(0)?),
                library_id: row.get(1)?,
                name: row.get(2)?,
                parent_id: row.get::<_, Option<String>>(3)?.map(FolderId::new),
                updated_at: row.get(4)?,
                deleted_at: row.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load sync folder rows.")
    }

    /// Returns all sync folder rows for one library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_folders_for_library(&self, library_id: &str) -> Result<Vec<SyncFolderRow>> {
        self.sync_folders_updated_since(library_id, i64::MIN)
    }

    /// Returns sync membership rows newer than `since`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_entry_folders_updated_since(
        &self,
        library_id: &str,
        since: i64,
    ) -> Result<Vec<SyncEntryFolderRow>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT ef.entry_id, ef.folder_id, ef.updated_at, ef.deleted_at
             FROM sync_entry_folders ef
             INNER JOIN sync_entries e ON e.id = ef.entry_id
             WHERE e.library_id = ?1 AND ef.updated_at > ?2
             ORDER BY ef.updated_at ASC, ef.entry_id ASC, ef.folder_id ASC",
        )?;
        let rows = statement.query_map(params![library_id, since], |row| {
            Ok(SyncEntryFolderRow {
                entry_id: EntryId::new(row.get::<_, String>(0)?),
                folder_id: FolderId::new(row.get::<_, String>(1)?),
                updated_at: row.get(2)?,
                deleted_at: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load sync entry-folder rows.")
    }

    /// Returns all sync membership rows for one library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query sync rows.
    pub fn sync_entry_folders_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<SyncEntryFolderRow>> {
        self.sync_entry_folders_updated_since(library_id, i64::MIN)
    }

    fn sync_entry_folder_updated_at(
        &self,
        entry_id: &EntryId,
        folder_id: &FolderId,
    ) -> Result<Option<i64>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT updated_at FROM sync_entry_folders
                 WHERE entry_id = ?1 AND folder_id = ?2",
                params![entry_id.as_str(), folder_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .context("Could not load sync entry-folder timestamp.")
    }

    /// Returns the last completed sync timestamp for a library/device pair.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the checkpoint.
    pub fn sync_checkpoint(&self, library_id: &str, device_id: &str) -> Result<Option<i64>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT last_synced_at FROM sync_checkpoints
                 WHERE library_id = ?1 AND device_id = ?2",
                params![library_id, device_id],
                |row| row.get(0),
            )
            .optional()
            .context("Could not load sync checkpoint.")
    }

    /// Updates the last completed sync timestamp for a library/device pair.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the checkpoint.
    pub fn set_sync_checkpoint(
        &self,
        library_id: &str,
        device_id: &str,
        last_synced_at: i64,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_checkpoints (library_id, device_id, last_synced_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(library_id, device_id) DO UPDATE SET
                last_synced_at = excluded.last_synced_at",
            params![library_id, device_id, last_synced_at],
        )?;
        Ok(())
    }

    /// Records a local CRDT operation when an entity payload changed.
    ///
    /// The `(library_id, entity_kind, entity_id)` payload hash table keeps the
    /// snapshot-driven sync loop from emitting duplicate operations for
    /// unchanged local state.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot read or write CRDT sync state.
    pub fn record_sync_crdt_operation_if_changed(
        &self,
        operation: &SyncCrdtOperation,
        payload_hash: &str,
    ) -> Result<bool> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing_hash: Option<String> = transaction
            .query_row(
                "SELECT payload_hash FROM sync_crdt_entity_versions
                 WHERE library_id = ?1 AND entity_kind = ?2 AND entity_id = ?3",
                params![
                    operation.library_id,
                    operation.entity_kind,
                    operation.entity_id
                ],
                |row| row.get(0),
            )
            .optional()?;
        if existing_hash.as_deref() == Some(payload_hash) {
            transaction.commit()?;
            return Ok(false);
        }

        transaction.execute(
            "INSERT OR IGNORE INTO sync_crdt_operations
                (op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                 payload, created_at, remote_sequence, pushed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                operation.op_id,
                operation.library_id,
                operation.device_id,
                operation.logical_time,
                operation.entity_kind,
                operation.entity_id,
                operation.payload,
                operation.created_at,
                operation.remote_sequence,
                operation.pushed_at,
            ],
        )?;
        transaction.execute(
            "INSERT INTO sync_crdt_entity_versions
                (library_id, entity_kind, entity_id, payload_hash, logical_time, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(library_id, entity_kind, entity_id) DO UPDATE SET
                payload_hash = excluded.payload_hash,
                logical_time = excluded.logical_time,
                device_id = excluded.device_id",
            params![
                operation.library_id,
                operation.entity_kind,
                operation.entity_id,
                payload_hash,
                operation.logical_time,
                operation.device_id,
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    /// Remembers the payload hash currently materialized for a CRDT entity.
    ///
    /// This is used after replaying remote operations so the next local
    /// snapshot pass does not echo a remotely-originated winning state as a new
    /// local operation.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the entity version.
    pub fn remember_sync_crdt_entity_payload(
        &self,
        library_id: &str,
        entity_kind: &str,
        entity_id: &str,
        payload_hash: &str,
        logical_time: i64,
        device_id: &str,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_crdt_entity_versions
                (library_id, entity_kind, entity_id, payload_hash, logical_time, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(library_id, entity_kind, entity_id) DO UPDATE SET
                payload_hash = excluded.payload_hash,
                logical_time = excluded.logical_time,
                device_id = excluded.device_id",
            params![
                library_id,
                entity_kind,
                entity_id,
                payload_hash,
                logical_time,
                device_id,
            ],
        )?;
        Ok(())
    }

    /// Stores a CRDT operation received from the remote log.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the operation.
    pub fn upsert_sync_crdt_operation(&self, operation: &SyncCrdtOperation) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_crdt_operations
                (op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                 payload, created_at, remote_sequence, pushed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(op_id) DO UPDATE SET
                remote_sequence = COALESCE(excluded.remote_sequence, remote_sequence),
                pushed_at = COALESCE(sync_crdt_operations.pushed_at, excluded.pushed_at)",
            params![
                operation.op_id,
                operation.library_id,
                operation.device_id,
                operation.logical_time,
                operation.entity_kind,
                operation.entity_id,
                operation.payload,
                operation.created_at,
                operation.remote_sequence,
                operation.pushed_at,
            ],
        )?;
        Ok(())
    }

    /// Returns local CRDT operations that still need to be pushed.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query operations.
    pub fn pending_sync_crdt_operations(
        &self,
        library_id: &str,
        device_id: &str,
    ) -> Result<Vec<SyncCrdtOperation>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                    payload, created_at, remote_sequence, pushed_at
             FROM sync_crdt_operations
             WHERE library_id = ?1 AND device_id = ?2 AND pushed_at IS NULL
             ORDER BY logical_time ASC, op_id ASC",
        )?;
        let rows =
            statement.query_map(params![library_id, device_id], row_to_sync_crdt_operation)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load pending CRDT sync operations.")
    }

    /// Returns the newest local CRDT logical time for a library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query operations.
    pub fn sync_crdt_max_logical_time(&self, library_id: &str) -> Result<Option<i64>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT MAX(logical_time) FROM sync_crdt_operations WHERE library_id = ?1",
                params![library_id],
                |row| row.get(0),
            )
            .optional()
            .map(Option::flatten)
            .context("Could not load max CRDT logical time.")
    }

    /// Marks local CRDT operations as pushed.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update operations.
    pub fn mark_sync_crdt_operations_pushed<'a>(
        &self,
        op_ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<usize> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let now = Utc::now().timestamp();
        let mut updated = 0;
        for op_id in op_ids {
            updated += transaction.execute(
                "UPDATE sync_crdt_operations
                 SET pushed_at = COALESCE(pushed_at, ?1)
                 WHERE op_id = ?2",
                params![now, op_id],
            )?;
        }
        transaction.commit()?;
        Ok(updated)
    }

    /// Returns all CRDT operations for one library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query operations.
    pub fn sync_crdt_operations_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<SyncCrdtOperation>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                    payload, created_at, remote_sequence, pushed_at
             FROM sync_crdt_operations
             WHERE library_id = ?1
             ORDER BY logical_time ASC, device_id ASC, op_id ASC",
        )?;
        let rows = statement.query_map(params![library_id], row_to_sync_crdt_operation)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load CRDT sync operations.")
    }

    /// Returns all CRDT operations for one entity.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query operations.
    pub fn sync_crdt_operations_for_entity(
        &self,
        library_id: &str,
        entity_kind: &str,
        entity_id: &str,
    ) -> Result<Vec<SyncCrdtOperation>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                    payload, created_at, remote_sequence, pushed_at
             FROM sync_crdt_operations
             WHERE library_id = ?1 AND entity_kind = ?2 AND entity_id = ?3
             ORDER BY logical_time ASC, device_id ASC, op_id ASC",
        )?;
        let rows = statement.query_map(
            params![library_id, entity_kind, entity_id],
            row_to_sync_crdt_operation,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load CRDT sync operations for entity.")
    }

    /// Returns the remote sequence cursor for a library/device pair.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the cursor.
    pub fn sync_crdt_remote_cursor(&self, library_id: &str, device_id: &str) -> Result<i64> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT last_remote_sequence FROM sync_crdt_checkpoints
                 WHERE library_id = ?1 AND device_id = ?2",
                params![library_id, device_id],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.unwrap_or(0))
            .context("Could not load CRDT sync cursor.")
    }

    /// Updates the remote sequence cursor for a library/device pair.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the cursor.
    pub fn set_sync_crdt_remote_cursor(
        &self,
        library_id: &str,
        device_id: &str,
        last_remote_sequence: i64,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_crdt_checkpoints
                (library_id, device_id, last_remote_sequence)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(library_id, device_id) DO UPDATE SET
                last_remote_sequence = excluded.last_remote_sequence",
            params![library_id, device_id, last_remote_sequence],
        )?;
        Ok(())
    }

    /// Returns when a content-addressed PDF blob was successfully uploaded.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the upload ledger.
    pub fn sync_blob_uploaded_at(&self, hash: &str) -> Result<Option<i64>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT uploaded_at FROM sync_blob_uploads WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()
            .context("Could not load sync blob upload state.")
    }

    /// Returns local entries whose blobs have not been marked uploaded yet.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entries_needing_sync_blob_upload(&self) -> Result<Vec<LibraryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT e.id, e.path, e.title, e.author, e.display_title, e.display_author,
                    e.sort_title, e.sort_author, e.metadata_locked, e.manual_order,
                    e.author_attributed, e.page_count_attributed, e.added_at, e.opened_at,
                    e.page_count, e.file_size, e.last_page, e.rating, e.cover_hash, e.missing
             FROM entries e
             LEFT JOIN sync_blob_uploads b ON b.hash = e.id
             WHERE b.hash IS NULL AND e.trashed_at IS NULL
             ORDER BY e.added_at ASC, e.id ASC",
        )?;
        let rows = statement.query_map([], row_to_entry)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entries needing sync blob upload.")
    }

    /// Returns true when at least one local PDF blob still needs remote upload.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn has_entries_needing_sync_blob_upload(&self) -> Result<bool> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT 1
                 FROM entries e
                 LEFT JOIN sync_blob_uploads b ON b.hash = e.id
                 WHERE b.hash IS NULL AND e.trashed_at IS NULL
                 LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()
            .map(|value| value.is_some())
            .context("Could not check entries needing sync blob upload.")
    }

    /// Marks a content-addressed PDF blob as uploaded to remote storage.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the upload ledger.
    pub fn remember_sync_blob_uploaded(&self, hash: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO sync_blob_uploads (hash, uploaded_at)
             VALUES (?1, ?2)
             ON CONFLICT(hash) DO UPDATE SET uploaded_at = excluded.uploaded_at",
            params![hash, Utc::now().timestamp()],
        )?;
        Ok(())
    }

    /// Materializes a sync entry row into the local entries table.
    ///
    /// The local `added_at` timestamp is set from the sync row so a later
    /// snapshot pass does not echo a hydrated remote entry as a new local
    /// operation merely because it was inserted locally later.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the entry.
    pub fn hydrate_sync_entry(
        &self,
        row: &SyncEntryRow,
        path: &Path,
        file_size: Option<u64>,
        missing: bool,
    ) -> Result<bool> {
        let connection = self.connection()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM entries WHERE id = ?1",
                params![row.id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Ok(false);
        }

        let manual_order = self.next_entry_manual_order_with_connection(&connection)?;
        connection.execute(
            "INSERT INTO entries
                (id, path, title, author, sort_title, sort_author, manual_order,
                 author_attributed, page_count_attributed, added_at, page_count,
                file_size, cover_hash, missing, trashed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, NULL, ?10, NULL, ?11, ?12)",
            params![
                row.id.as_str(),
                path.to_string_lossy(),
                row.title,
                row.author,
                sort_key(row.title.as_deref()),
                sort_key(row.author.as_deref()),
                manual_order,
                i64::from(row.author.is_some()),
                row.updated_at,
                file_size.map(|value| value as i64),
                i64::from(missing),
                row.deleted_at,
            ],
        )?;
        Ok(true)
    }

    /// Applies user-facing synced metadata to an existing local entry.
    ///
    /// This intentionally preserves the local PDF path and source-file status
    /// while updating the cross-device reading and organization metadata that
    /// lets a user continue on another device.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry or its tags.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_synced_entry_state(
        &self,
        entry_id: &EntryId,
        title: Option<&str>,
        author: Option<&str>,
        display_title: Option<&str>,
        display_author: Option<&str>,
        metadata_locked: bool,
        page_count: Option<u16>,
        last_page: u16,
        opened_at: Option<i64>,
        tags: &[String],
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let title = title.map(str::to_owned);
        let author = author.map(str::to_owned);
        let display_title = display_title.map(str::to_owned);
        let display_author = display_author.map(str::to_owned);
        let sort_title = sort_key(display_title.as_deref().or(title.as_deref()));
        let sort_author = sort_key(display_author.as_deref().or(author.as_deref()));
        transaction.execute(
            "UPDATE entries
             SET title = ?1,
                 author = ?2,
                 display_title = ?3,
                 display_author = ?4,
                 sort_title = ?5,
                 sort_author = ?6,
                 metadata_locked = ?7,
                 page_count = COALESCE(?8, page_count),
                 page_count_attributed = CASE WHEN ?8 IS NULL THEN page_count_attributed ELSE 1 END,
                 last_page = ?9,
                 opened_at = ?10
             WHERE id = ?11",
            params![
                title,
                author,
                display_title,
                display_author,
                sort_title,
                sort_author,
                i64::from(metadata_locked),
                page_count.map(i64::from),
                i64::from(last_page),
                opened_at,
                entry_id.as_str(),
            ],
        )?;
        transaction.execute(
            "DELETE FROM tags WHERE entry_id = ?1",
            params![entry_id.as_str()],
        )?;
        for tag in tags {
            transaction.execute(
                "INSERT OR IGNORE INTO tags (entry_id, tag) VALUES (?1, ?2)",
                params![entry_id.as_str(), tag],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Applies synchronized trash state for an entry.
    ///
    /// Unlike user-initiated trash/restore operations, this preserves the
    /// remote timestamp so devices do not echo equivalent trash changes back
    /// with a different local clock value.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn apply_synced_entry_trash_state(
        &self,
        entry_id: &EntryId,
        trashed_at: Option<i64>,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET trashed_at = ?1 WHERE id = ?2",
            params![trashed_at, entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Applies synchronized folder state to an existing local folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the folder.
    pub fn apply_synced_folder_state(&self, row: &SyncFolderRow) -> Result<()> {
        let connection = self.connection()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM folders WHERE id = ?1",
                params![row.id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(());
        }
        let parent_id = match &row.parent_id {
            Some(parent_id)
                if connection
                    .query_row(
                        "SELECT 1 FROM folders WHERE id = ?1",
                        params![parent_id.as_str()],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some() =>
            {
                Some(parent_id.as_str())
            }
            _ => None,
        };
        connection.execute(
            "UPDATE folders
             SET name = ?1, parent_id = ?2, updated_at = ?3, trashed_at = ?4
             WHERE id = ?5",
            params![
                row.name,
                parent_id,
                row.updated_at,
                row.deleted_at,
                row.id.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Applies synchronized entry-folder membership state to the local graph.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the membership.
    pub fn apply_synced_entry_folder_state(&self, row: &SyncEntryFolderRow) -> Result<bool> {
        if row.deleted_at.is_some() {
            return Ok(self.connection()?.execute(
                "DELETE FROM entry_folders WHERE entry_id = ?1 AND folder_id = ?2",
                params![row.entry_id.as_str(), row.folder_id.as_str()],
            )? > 0);
        }
        self.hydrate_sync_entry_folder(row)
    }

    /// Materializes a sync folder row into the local folder table.
    ///
    /// Parent links are applied only when the parent folder already exists,
    /// which keeps hydration robust when remote rows arrive out of hierarchy
    /// order. A later hydration pass can fill the parent once both rows exist.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the folder.
    pub fn hydrate_sync_folder(&self, row: &SyncFolderRow) -> Result<bool> {
        let connection = self.connection()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM folders WHERE id = ?1",
                params![row.id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let parent_id = match &row.parent_id {
            Some(parent_id)
                if connection
                    .query_row(
                        "SELECT 1 FROM folders WHERE id = ?1",
                        params![parent_id.as_str()],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some() =>
            {
                Some(parent_id.as_str())
            }
            _ => None,
        };
        let manual_order = self.next_folder_manual_order_with_connection(
            &connection,
            row.parent_id.as_ref().filter(|_| parent_id.is_some()),
        )?;
        connection.execute(
            "INSERT INTO folders
                (id, name, parent_id, manual_order, created_at, updated_at, trashed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                parent_id = excluded.parent_id,
                updated_at = excluded.updated_at,
                trashed_at = excluded.trashed_at",
            params![
                row.id.as_str(),
                row.name,
                parent_id,
                manual_order,
                row.updated_at,
                row.deleted_at,
            ],
        )?;
        Ok(!exists)
    }

    /// Materializes a sync entry-folder membership into the local library.
    ///
    /// Returns `false` when the entry/folder does not exist yet, the remote row
    /// is deleted, or the membership already exists.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the membership.
    pub fn hydrate_sync_entry_folder(&self, row: &SyncEntryFolderRow) -> Result<bool> {
        if row.deleted_at.is_some() {
            return Ok(false);
        }
        let connection = self.connection()?;
        let entry_exists = connection
            .query_row(
                "SELECT 1 FROM entries WHERE id = ?1",
                params![row.entry_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let folder_exists = connection
            .query_row(
                "SELECT 1 FROM folders WHERE id = ?1",
                params![row.folder_id.as_str()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !entry_exists || !folder_exists {
            return Ok(false);
        }
        let manual_order =
            self.next_folder_entry_manual_order_with_connection(&connection, &row.folder_id)?;
        let inserted = connection.execute(
            "INSERT INTO entry_folders (entry_id, folder_id, manual_order)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(entry_id, folder_id) DO NOTHING",
            params![row.entry_id.as_str(), row.folder_id.as_str(), manual_order],
        )?;
        Ok(inserted > 0)
    }

    /// Seeds sync metadata from the current local library state.
    ///
    /// This is primarily used for the first sync on an existing library. It is
    /// idempotent and does not change sync checkpoints.
    ///
    /// # Errors
    ///
    /// Returns an error when local library state cannot be read or sync metadata
    /// cannot be written.
    pub fn seed_sync_metadata(&self, library_id: &str) -> Result<SyncSeedSummary> {
        let now = Utc::now().timestamp();
        let entries = self
            .get_all_entries()?
            .into_iter()
            .chain(self.get_trashed_entries()?)
            .collect::<Vec<_>>();
        let snapshot = self.library_organization_snapshot()?;
        let entry_ids = entries
            .iter()
            .map(|entry| entry.id.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        let folder_ids = snapshot
            .folders
            .iter()
            .map(|folder| folder.id.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>();
        let membership_ids = snapshot
            .entry_folders
            .iter()
            .map(|membership| {
                (
                    membership.entry_id.as_str().to_owned(),
                    membership.folder_id.as_str().to_owned(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();

        for entry in &entries {
            let updated_at = entry
                .opened_at
                .unwrap_or(entry.added_at)
                .timestamp()
                .max(entry.added_at.timestamp());
            self.upsert_sync_entry(&SyncEntryRow {
                id: entry.id.clone(),
                library_id: library_id.to_owned(),
                title: entry
                    .display_title
                    .clone()
                    .or_else(|| entry.title.clone())
                    .or_else(|| {
                        entry
                            .path
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .map(str::to_owned)
                    }),
                author: entry
                    .display_author
                    .clone()
                    .or_else(|| entry.author.clone()),
                updated_at,
                deleted_at: snapshot
                    .entry_trash_states
                    .iter()
                    .find(|state| state.entry_id == entry.id)
                    .and_then(|state| state.trashed_at)
                    .map(|timestamp| timestamp.timestamp()),
            })?;
        }

        for row in self.sync_entries_for_library(library_id)? {
            if row.deleted_at.is_none() && !entry_ids.contains(row.id.as_str()) {
                self.upsert_sync_entry(&SyncEntryRow {
                    updated_at: now.max(row.updated_at + 1),
                    deleted_at: Some(now),
                    ..row
                })?;
            }
        }

        for folder in &snapshot.folders {
            let deleted_at = folder.trashed_at.map(|timestamp| timestamp.timestamp());
            self.upsert_sync_folder(&SyncFolderRow {
                id: folder.id.clone(),
                library_id: library_id.to_owned(),
                name: folder.name.clone(),
                parent_id: folder.parent_id.clone(),
                updated_at: folder.updated_at.timestamp(),
                deleted_at,
            })?;
        }

        for row in self.sync_folders_for_library(library_id)? {
            if row.deleted_at.is_none() && !folder_ids.contains(row.id.as_str()) {
                self.upsert_sync_folder(&SyncFolderRow {
                    updated_at: now.max(row.updated_at + 1),
                    deleted_at: Some(now),
                    ..row
                })?;
            }
        }

        for membership in &snapshot.entry_folders {
            let updated_at = self
                .sync_entry_folder_updated_at(&membership.entry_id, &membership.folder_id)?
                .unwrap_or(now);
            self.upsert_sync_entry_folder(&SyncEntryFolderRow {
                entry_id: membership.entry_id.clone(),
                folder_id: membership.folder_id.clone(),
                updated_at,
                deleted_at: None,
            })?;
        }

        for row in self.sync_entry_folders_for_library(library_id)? {
            let key = (
                row.entry_id.as_str().to_owned(),
                row.folder_id.as_str().to_owned(),
            );
            if row.deleted_at.is_none() && !membership_ids.contains(&key) {
                self.upsert_sync_entry_folder(&SyncEntryFolderRow {
                    updated_at: now.max(row.updated_at + 1),
                    deleted_at: Some(now),
                    ..row
                })?;
            }
        }

        Ok(SyncSeedSummary {
            entries: entries.len(),
            folders: snapshot.folders.len(),
            entry_folders: snapshot.entry_folders.len(),
        })
    }

    /// Inserts or replaces a library entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the entry.
    pub fn insert_entry(&self, entry: &NewLibraryEntry) -> Result<()> {
        let connection = self.connection()?;
        let now = Utc::now().timestamp();
        let manual_order = self.next_entry_manual_order_with_connection(&connection)?;
        connection.execute(
            "INSERT OR REPLACE INTO entries
                (id, path, title, author, sort_title, sort_author, manual_order, author_attributed, page_count_attributed, added_at, page_count, file_size, cover_hash, missing)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0)
             ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                title = excluded.title,
                author = COALESCE(excluded.author, entries.author),
                sort_title = COALESCE(entries.sort_title, excluded.sort_title),
                sort_author = COALESCE(entries.sort_author, excluded.sort_author),
                author_attributed = CASE
                    WHEN excluded.author_attributed != 0 THEN excluded.author_attributed
                    ELSE entries.author_attributed
                END,
                page_count_attributed = CASE
                    WHEN excluded.page_count_attributed != 0 THEN excluded.page_count_attributed
                    ELSE entries.page_count_attributed
                END,
                page_count = COALESCE(excluded.page_count, entries.page_count),
                file_size = COALESCE(excluded.file_size, entries.file_size),
                cover_hash = COALESCE(excluded.cover_hash, entries.cover_hash),
                missing = 0,
                trashed_at = NULL",
            params![
                entry.id.as_str(),
                entry.path.to_string_lossy(),
                entry.title,
                entry.author,
                sort_key(entry.title.as_deref()),
                sort_key(entry.author.as_deref()),
                manual_order,
                i64::from(entry.author_attributed),
                i64::from(entry.page_count_attributed),
                now,
                entry.page_count.map(i64::from),
                entry.file_size.map(|value| value as i64),
                entry.cover_hash,
            ],
        )?;
        Ok(())
    }

    /// Returns all library entries ordered by most recent addition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn get_all_entries(&self) -> Result<Vec<LibraryEntry>> {
        self.get_entries_sorted(LibrarySortMode::RecentlyAdded)
    }

    /// Returns the entry with the given id, if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entry_by_id(&self, entry_id: &EntryId) -> Result<Option<LibraryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, file_size, last_page, rating, cover_hash, missing
             FROM entries
             WHERE id = ?1",
        )?;
        let mut entry = statement
            .query_row(params![entry_id.as_str()], row_to_entry)
            .optional()
            .context("Could not load library entry by id.")?;

        if let Some(entry) = &mut entry {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id, true)?;
        }

        Ok(entry)
    }

    /// Returns all library entries ordered for a selected library sort mode.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn get_entries_sorted(&self, sort_mode: LibrarySortMode) -> Result<Vec<LibraryEntry>> {
        let connection = self.connection()?;
        let order_by = match sort_mode {
            LibrarySortMode::Manual => {
                "manual_order ASC, lower(COALESCE(sort_title, display_title, title, path)) ASC"
            }
            LibrarySortMode::TitleAsc => {
                "lower(COALESCE(sort_title, display_title, title, path)) ASC, manual_order ASC"
            }
            LibrarySortMode::TitleDesc => {
                "lower(COALESCE(sort_title, display_title, title, path)) DESC, manual_order ASC"
            }
            LibrarySortMode::AuthorAsc => {
                "lower(COALESCE(sort_author, display_author, author, '')) ASC, lower(COALESCE(sort_title, display_title, title, path)) ASC"
            }
            LibrarySortMode::AuthorDesc => {
                "lower(COALESCE(sort_author, display_author, author, '')) DESC, lower(COALESCE(sort_title, display_title, title, path)) ASC"
            }
            LibrarySortMode::RecentlyAdded => "added_at DESC, manual_order ASC",
            LibrarySortMode::RecentlyOpened => "opened_at DESC NULLS LAST, manual_order ASC",
            LibrarySortMode::ReadingProgress => {
                "CASE WHEN page_count IS NULL OR page_count = 0 THEN 0.0 ELSE CAST(last_page + 1 AS REAL) / page_count END DESC, manual_order ASC"
            }
            LibrarySortMode::PageCount => "page_count DESC NULLS LAST, manual_order ASC",
            LibrarySortMode::MissingFiles => "missing DESC, manual_order ASC",
        };
        let mut statement = connection.prepare(
            &format!(
                "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, file_size, last_page, rating, cover_hash, missing
             FROM entries
             WHERE trashed_at IS NULL
             ORDER BY {order_by}"
            ),
        )?;

        let rows = statement.query_map([], row_to_entry)?;

        let mut entries = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load library entries.")?;
        self.attach_entry_collections_with_connection(&connection, &mut entries, false)?;
        Ok(entries)
    }

    /// Returns all entries currently in the trash.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn get_trashed_entries(&self) -> Result<Vec<LibraryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, file_size, last_page, rating, cover_hash, missing
             FROM entries
             WHERE trashed_at IS NOT NULL
             ORDER BY trashed_at DESC, manual_order ASC",
        )?;
        let rows = statement.query_map([], row_to_entry)?;
        let mut entries = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load trashed library entries.")?;
        self.attach_entry_collections_with_connection(&connection, &mut entries, true)?;
        Ok(entries)
    }

    /// Replaces the manual order of entries with the given visible order.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the order.
    pub fn set_manual_entry_order(&self, entry_ids: &[EntryId]) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for (index, entry_id) in entry_ids.iter().enumerate() {
            transaction.execute(
                "UPDATE entries SET manual_order = ?1 WHERE id = ?2",
                params![(index as i64 + 1) * MANUAL_ORDER_GAP, entry_id.as_str()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Replaces the manual order of entries inside one folder.
    ///
    /// # Errors
    ///
    /// Returns an error when any entry is not in the folder or SQLite cannot write the order.
    pub fn set_manual_folder_entry_order(
        &self,
        folder_id: &FolderId,
        entry_ids: &[EntryId],
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for (index, entry_id) in entry_ids.iter().enumerate() {
            let updated = transaction.execute(
                "UPDATE entry_folders
                 SET manual_order = ?1
                 WHERE folder_id = ?2 AND entry_id = ?3",
                params![
                    (index as i64 + 1) * MANUAL_ORDER_GAP,
                    folder_id.as_str(),
                    entry_id.as_str()
                ],
            )?;
            if updated == 0 {
                anyhow::bail!(
                    "Entry {} is not in folder {}.",
                    entry_id.as_str(),
                    folder_id.as_str()
                );
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Replaces the manual order of sibling folders under one parent.
    ///
    /// # Errors
    ///
    /// Returns an error when any folder is missing, does not belong to the requested parent, or
    /// SQLite cannot write the order.
    pub fn set_manual_folder_order(
        &self,
        parent_id: Option<&FolderId>,
        folder_ids: &[FolderId],
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for (index, folder_id) in folder_ids.iter().enumerate() {
            let actual_parent = transaction
                .query_row(
                    "SELECT parent_id FROM folders WHERE id = ?1",
                    params![folder_id.as_str()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?
                .map(|parent| parent.map(FolderId::new))
                .with_context(|| format!("Folder {} does not exist.", folder_id.as_str()))?;
            if actual_parent.as_ref() != parent_id {
                anyhow::bail!(
                    "Folder {} does not belong to the requested parent.",
                    folder_id.as_str()
                );
            }

            transaction.execute(
                "UPDATE folders SET manual_order = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    (index as i64 + 1) * MANUAL_ORDER_GAP,
                    Utc::now().timestamp(),
                    folder_id.as_str()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Updates display metadata overrides for an entry.
    ///
    /// Empty or whitespace-only values clear the corresponding override.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the metadata.
    pub fn update_display_metadata(
        &self,
        entry_id: &EntryId,
        display_title: Option<&str>,
        display_author: Option<&str>,
    ) -> Result<()> {
        let display_title = clean_optional_text(display_title);
        let display_author = clean_optional_text(display_author);
        let sort_title = sort_key(display_title.as_deref());
        let sort_author = sort_key(display_author.as_deref());
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries
             SET display_title = ?1,
                 display_author = ?2,
                 sort_title = COALESCE(?3, sort_title),
                 sort_author = COALESCE(?4, sort_author),
                 metadata_locked = 1
             WHERE id = ?5",
            params![
                display_title,
                display_author,
                sort_title,
                sort_author,
                entry_id.as_str()
            ],
        )?;
        Ok(())
    }

    /// Clears display metadata overrides and unlocks extracted metadata updates.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the metadata.
    pub fn reset_display_metadata(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries
             SET display_title = NULL,
                 display_author = NULL,
                 sort_title = lower(title),
                 sort_author = lower(author),
                 metadata_locked = 0
             WHERE id = ?1",
            params![entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Applies title sort cleanup for leading English articles.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot load or write the title sort key.
    pub fn apply_title_sort_cleanup(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        let title: Option<String> = connection
            .query_row(
                "SELECT COALESCE(display_title, title) FROM entries WHERE id = ?1",
                params![entry_id.as_str()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let sort_title = title.as_deref().and_then(clean_title_sort_key);
        connection.execute(
            "UPDATE entries SET sort_title = ?1 WHERE id = ?2",
            params![sort_title, entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Creates a user folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the folder or the parent does not exist.
    pub fn create_folder(&self, name: &str, parent_id: Option<&FolderId>) -> Result<FolderId> {
        let name = clean_folder_name(name)?;
        let connection = self.connection()?;
        let id = FolderId::new(format!(
            "folder-{}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            next_folder_suffix(&connection)?
        ));
        let now = Utc::now().timestamp();
        let manual_order = self.next_folder_manual_order_with_connection(&connection, parent_id)?;
        connection.execute(
            "INSERT INTO folders (id, name, parent_id, manual_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.as_str(),
                name,
                parent_id.map(FolderId::as_str),
                manual_order,
                now,
                now
            ],
        )?;
        Ok(id)
    }

    /// Creates or updates an external import source.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the source.
    pub fn upsert_import_source(
        &self,
        id: &str,
        kind: &str,
        account_id: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<ImportSource> {
        let connection = self.connection()?;
        let now = Utc::now().timestamp();
        connection.execute(
            "INSERT INTO import_sources
                (id, kind, account_id, display_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                account_id = excluded.account_id,
                display_name = excluded.display_name,
                updated_at = excluded.updated_at",
            params![id, kind, account_id, display_name, now],
        )?;
        self.import_source(id)?
            .with_context(|| format!("Import source {id} was not saved."))
    }

    /// Returns one import source.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the source.
    pub fn import_source(&self, id: &str) -> Result<Option<ImportSource>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT id, kind, account_id, display_name, created_at, updated_at
                 FROM import_sources
                 WHERE id = ?1",
                params![id],
                row_to_import_source,
            )
            .optional()
            .context("Could not load import source.")
    }

    /// Creates or updates the local folder mirrored from a Raindrop collection.
    ///
    /// Parent collection mappings should be created before child mappings when possible.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the mapping or folder.
    pub fn upsert_raindrop_collection_mapping(
        &self,
        source_id: &str,
        collection_id: i64,
        parent_collection_id: Option<i64>,
        title: &str,
        root_folder_id: Option<&FolderId>,
    ) -> Result<(FolderId, bool)> {
        let title = clean_folder_name(title)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing_folder_id: Option<FolderId> = transaction
            .query_row(
                "SELECT folder_id FROM raindrop_collections
                 WHERE source_id = ?1 AND collection_id = ?2",
                params![source_id, collection_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(FolderId::new);
        let mapped_parent_folder_id = match parent_collection_id {
            Some(parent_id) => transaction
                .query_row(
                    "SELECT folder_id FROM raindrop_collections
                     WHERE source_id = ?1 AND collection_id = ?2",
                    params![source_id, parent_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(FolderId::new),
            None => None,
        };
        let parent_folder_id = mapped_parent_folder_id.or_else(|| {
            parent_collection_id
                .is_none()
                .then(|| root_folder_id.cloned())
                .flatten()
        });
        let now = Utc::now().timestamp();
        let (folder_id, created) = if let Some(folder_id) = existing_folder_id {
            transaction.execute(
                "UPDATE folders
                 SET name = ?1, parent_id = ?2, updated_at = ?3
                 WHERE id = ?4",
                params![
                    title,
                    parent_folder_id.as_ref().map(FolderId::as_str),
                    now,
                    folder_id.as_str()
                ],
            )?;
            (folder_id, false)
        } else {
            let folder_id = FolderId::new(format!(
                "folder-{}-{}",
                Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                next_folder_suffix(&transaction)?
            ));
            let manual_order = self.next_folder_manual_order_with_connection(
                &transaction,
                parent_folder_id.as_ref(),
            )?;
            transaction.execute(
                "INSERT INTO folders
                    (id, name, parent_id, manual_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    folder_id.as_str(),
                    title,
                    parent_folder_id.as_ref().map(FolderId::as_str),
                    manual_order,
                    now
                ],
            )?;
            (folder_id, true)
        };
        transaction.execute(
            "INSERT INTO raindrop_collections
                (source_id, collection_id, folder_id, parent_collection_id, title)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(source_id, collection_id) DO UPDATE SET
                folder_id = excluded.folder_id,
                parent_collection_id = excluded.parent_collection_id,
                title = excluded.title",
            params![
                source_id,
                collection_id,
                folder_id.as_str(),
                parent_collection_id,
                title
            ],
        )?;
        transaction.commit()?;
        Ok((folder_id, created))
    }

    /// Returns the local folder mapped to a Raindrop collection.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the mapping.
    pub fn raindrop_collection_folder(
        &self,
        source_id: &str,
        collection_id: i64,
    ) -> Result<Option<FolderId>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT folder_id FROM raindrop_collections
                 WHERE source_id = ?1 AND collection_id = ?2",
                params![source_id, collection_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|id| id.map(FolderId::new))
            .context("Could not load Raindrop collection mapping.")
    }

    /// Renames a folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the folder.
    pub fn rename_folder(&self, folder_id: &FolderId, name: &str) -> Result<()> {
        let name = clean_folder_name(name)?;
        let connection = self.connection()?;
        connection.execute(
            "UPDATE folders SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![name, Utc::now().timestamp(), folder_id.as_str()],
        )?;
        Ok(())
    }

    /// Deletes a folder without deleting PDFs.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the folder.
    pub fn delete_folder(&self, folder_id: &FolderId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM folders WHERE id = ?1",
            params![folder_id.as_str()],
        )?;
        Ok(())
    }

    /// Moves a folder subtree and its PDFs to the trash.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the folder subtree.
    pub fn trash_folder_tree(&self, folder_id: &FolderId) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let folders = self.get_folders_with_connection(&transaction)?;
        let mut folder_ids = std::collections::HashSet::new();
        collect_folder_subtree_ids_from(&folders, folder_id, &mut folder_ids);
        if folder_ids.is_empty() {
            return Ok(());
        }

        let now = Utc::now().timestamp();
        for folder_id in &folder_ids {
            transaction.execute(
                "UPDATE folders SET trashed_at = ?1, updated_at = ?1 WHERE id = ?2",
                params![now, folder_id.as_str()],
            )?;
        }
        for folder_id in &folder_ids {
            transaction.execute(
                "UPDATE entries
                 SET trashed_at = ?1
                 WHERE id IN (
                    SELECT entry_id FROM entry_folders WHERE folder_id = ?2
                 )",
                params![now, folder_id.as_str()],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    /// Restores a trashed folder subtree and its PDFs from the trash.
    ///
    /// If the restored folder's original parent is not being restored and is not an active folder,
    /// the restored folder is moved to the library root.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the folder subtree.
    pub fn restore_folder_tree(&self, folder_id: &FolderId) -> Result<usize> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let snapshot = self.library_organization_snapshot_with_connection(&transaction)?;
        let Some(selected_folder) = snapshot
            .folders
            .iter()
            .find(|folder| &folder.id == folder_id)
        else {
            return Ok(0);
        };
        let mut folder_ids = std::collections::HashSet::new();
        collect_folder_snapshot_subtree_ids_from(&snapshot.folders, folder_id, &mut folder_ids);
        if folder_ids.is_empty() {
            return Ok(0);
        }

        let selected_parent_id = selected_folder.parent_id.clone();
        let selected_parent_active = selected_parent_id.as_ref().is_some_and(|parent_id| {
            snapshot.folders.iter().any(|folder| {
                &folder.id == parent_id
                    && folder.trashed_at.is_none()
                    && !folder_ids.contains(parent_id)
            })
        });
        let restore_to_root = selected_parent_id
            .as_ref()
            .is_some_and(|parent_id| !folder_ids.contains(parent_id) && !selected_parent_active);

        let now = Utc::now().timestamp();
        for restored_folder_id in &folder_ids {
            if restored_folder_id == folder_id && restore_to_root {
                transaction.execute(
                    "UPDATE folders
                     SET parent_id = NULL, trashed_at = NULL, updated_at = ?1
                     WHERE id = ?2",
                    params![now, restored_folder_id.as_str()],
                )?;
            } else {
                transaction.execute(
                    "UPDATE folders
                     SET trashed_at = NULL, updated_at = ?1
                     WHERE id = ?2",
                    params![now, restored_folder_id.as_str()],
                )?;
            }
        }

        let mut restored_entry_ids = std::collections::HashSet::new();
        for restored_folder_id in &folder_ids {
            {
                let mut statement = transaction.prepare(
                    "SELECT entry_id FROM entry_folders WHERE folder_id = ?1 ORDER BY entry_id",
                )?;
                let rows = statement.query_map(params![restored_folder_id.as_str()], |row| {
                    Ok(EntryId::new(row.get::<_, String>(0)?))
                })?;
                for entry_id in rows {
                    restored_entry_ids.insert(entry_id?);
                }
            }
        }
        for entry_id in &restored_entry_ids {
            transaction.execute(
                "UPDATE entries SET trashed_at = NULL WHERE id = ?1",
                params![entry_id.as_str()],
            )?;
        }
        transaction.commit()?;

        Ok(folder_ids.len() + restored_entry_ids.len())
    }

    /// Permanently deletes a trashed folder subtree and PDFs in that subtree.
    ///
    /// Source PDF files are left untouched.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the folder subtree.
    pub fn permanently_delete_trashed_folder_tree(
        &self,
        folder_id: &FolderId,
    ) -> Result<(usize, Vec<EntryId>)> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let snapshot = self.library_organization_snapshot_with_connection(&transaction)?;
        if !snapshot
            .folders
            .iter()
            .any(|folder| &folder.id == folder_id && folder.trashed_at.is_some())
        {
            return Ok((0, Vec::new()));
        }

        let mut folder_ids = std::collections::HashSet::new();
        collect_folder_snapshot_subtree_ids_from(&snapshot.folders, folder_id, &mut folder_ids);
        if folder_ids.is_empty() {
            return Ok((0, Vec::new()));
        }

        let mut entry_ids = std::collections::HashSet::new();
        for deleted_folder_id in &folder_ids {
            {
                let mut statement = transaction.prepare(
                    "SELECT entry_id FROM entry_folders WHERE folder_id = ?1 ORDER BY entry_id",
                )?;
                let rows = statement.query_map(params![deleted_folder_id.as_str()], |row| {
                    Ok(EntryId::new(row.get::<_, String>(0)?))
                })?;
                for entry_id in rows {
                    entry_ids.insert(entry_id?);
                }
            }
        }

        for entry_id in &entry_ids {
            transaction.execute(
                "DELETE FROM entries WHERE id = ?1 AND trashed_at IS NOT NULL",
                params![entry_id.as_str()],
            )?;
        }
        transaction.execute(
            "DELETE FROM folders WHERE id = ?1 AND trashed_at IS NOT NULL",
            params![folder_id.as_str()],
        )?;
        transaction.commit()?;

        let mut entry_ids = entry_ids.into_iter().collect::<Vec<_>>();
        entry_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Ok((folder_ids.len() + entry_ids.len(), entry_ids))
    }

    /// Returns all folders in manual order.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query folders.
    pub fn get_folders(&self) -> Result<Vec<Folder>> {
        let connection = self.connection()?;
        self.get_folders_with_connection(&connection)
    }

    /// Returns all folders currently in the trash in manual order.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query folders.
    pub fn get_trashed_folders(&self) -> Result<Vec<Folder>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, manual_order, created_at, updated_at
             FROM folders
             WHERE trashed_at IS NOT NULL
             ORDER BY COALESCE(parent_id, ''), trashed_at DESC, manual_order ASC, name ASC",
        )?;
        let rows = statement.query_map([], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load trashed folders.")
    }

    /// Adds an entry to a folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write membership.
    pub fn add_entry_to_folder(&self, entry_id: &EntryId, folder_id: &FolderId) -> Result<()> {
        let connection = self.connection()?;
        let manual_order =
            self.next_folder_entry_manual_order_with_connection(&connection, folder_id)?;
        connection.execute(
            "INSERT INTO entry_folders (entry_id, folder_id, manual_order)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(entry_id, folder_id) DO NOTHING",
            params![entry_id.as_str(), folder_id.as_str(), manual_order],
        )?;
        Ok(())
    }

    /// Moves an entry into exactly one folder.
    ///
    /// Existing folder memberships for the entry are removed before the new membership is added.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write membership.
    pub fn move_entry_to_folder(&self, entry_id: &EntryId, folder_id: &FolderId) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let manual_order =
            self.next_folder_entry_manual_order_with_connection(&transaction, folder_id)?;
        transaction.execute(
            "DELETE FROM entry_folders WHERE entry_id = ?1",
            params![entry_id.as_str()],
        )?;
        transaction.execute(
            "INSERT INTO entry_folders (entry_id, folder_id, manual_order)
             VALUES (?1, ?2, ?3)",
            params![entry_id.as_str(), folder_id.as_str(), manual_order],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Moves an entry to the library root by removing all folder memberships.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete memberships.
    pub fn move_entry_to_root(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM entry_folders WHERE entry_id = ?1",
            params![entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Removes an entry from a folder.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete membership.
    pub fn remove_entry_from_folder(&self, entry_id: &EntryId, folder_id: &FolderId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM entry_folders WHERE entry_id = ?1 AND folder_id = ?2",
            params![entry_id.as_str(), folder_id.as_str()],
        )?;
        Ok(())
    }

    /// Returns entries in one folder ordered by the entry-folder manual order.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entries_in_folder(&self, folder_id: &FolderId) -> Result<Vec<LibraryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT e.id, e.path, e.title, e.author, e.display_title, e.display_author, e.sort_title, e.sort_author, e.metadata_locked, e.manual_order, e.author_attributed, e.page_count_attributed, e.added_at, e.opened_at, e.page_count, e.file_size, e.last_page, e.rating, e.cover_hash, e.missing
             FROM entries e
             INNER JOIN entry_folders ef ON ef.entry_id = e.id
             WHERE ef.folder_id = ?1
             ORDER BY ef.manual_order ASC, e.manual_order ASC",
        )?;
        let rows = statement.query_map(params![folder_id.as_str()], row_to_entry)?;
        let mut entries = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load folder entries.")?;
        for entry in &mut entries {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders =
                self.folders_for_entry_with_connection(&connection, &entry.id, false)?;
        }
        Ok(entries)
    }

    /// Captures all folder rows and PDF-folder memberships for undoable organization edits.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the organization graph.
    pub fn library_organization_snapshot(&self) -> Result<LibraryOrganizationSnapshot> {
        let connection = self.connection()?;
        self.library_organization_snapshot_with_connection(&connection)
    }

    /// Restores folder rows, PDF-folder memberships, tags, trash state, and user metadata from a
    /// previous snapshot.
    ///
    /// Source PDF files are left untouched.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot restore the organization graph.
    pub fn restore_library_organization_snapshot(
        &self,
        snapshot: &LibraryOrganizationSnapshot,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
        transaction.execute("DELETE FROM entry_folders", [])?;
        transaction.execute("DELETE FROM folders", [])?;

        for folder in &snapshot.folders {
            transaction.execute(
                "INSERT INTO folders
                    (id, name, parent_id, manual_order, created_at, updated_at, trashed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    folder.id.as_str(),
                    folder.name,
                    folder.parent_id.as_ref().map(FolderId::as_str),
                    folder.manual_order,
                    folder.created_at.timestamp(),
                    folder.updated_at.timestamp(),
                    folder.trashed_at.map(|timestamp| timestamp.timestamp()),
                ],
            )?;
        }

        for membership in &snapshot.entry_folders {
            transaction.execute(
                "INSERT OR IGNORE INTO entry_folders
                    (entry_id, folder_id, manual_order)
                 VALUES (?1, ?2, ?3)",
                params![
                    membership.entry_id.as_str(),
                    membership.folder_id.as_str(),
                    membership.manual_order,
                ],
            )?;
        }

        for entry in &snapshot.entry_trash_states {
            transaction.execute(
                "UPDATE entries
                 SET display_title = ?1,
                     display_author = ?2,
                     sort_title = ?3,
                     sort_author = ?4,
                     metadata_locked = ?5,
                     manual_order = ?6,
                     trashed_at = ?7
                 WHERE id = ?8",
                params![
                    entry.display_title,
                    entry.display_author,
                    entry.sort_title,
                    entry.sort_author,
                    if entry.metadata_locked { 1 } else { 0 },
                    entry.manual_order,
                    entry.trashed_at.map(|timestamp| timestamp.timestamp()),
                    entry.entry_id.as_str(),
                ],
            )?;
        }

        let snapshot_entry_ids = snapshot
            .entry_trash_states
            .iter()
            .map(|entry| entry.entry_id.as_str())
            .collect::<Vec<_>>();
        if !snapshot_entry_ids.is_empty() {
            let placeholders = std::iter::repeat("?")
                .take(snapshot_entry_ids.len())
                .collect::<Vec<_>>()
                .join(", ");
            transaction.execute(
                &format!("DELETE FROM tags WHERE entry_id IN ({placeholders})"),
                params_from_iter(snapshot_entry_ids),
            )?;
        }

        for tag in &snapshot.entry_tags {
            transaction.execute(
                "INSERT OR IGNORE INTO tags (entry_id, tag) VALUES (?1, ?2)",
                params![tag.entry_id.as_str(), tag.tag],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    /// Duplicates a folder subtree under a destination folder or the library root.
    ///
    /// The copied folders receive new ids while preserving descendant names, order, and PDF
    /// memberships. The copied root folder is suffixed with `Copy`.
    ///
    /// # Errors
    ///
    /// Returns an error when the source folder is missing or SQLite cannot write the copy.
    pub fn copy_folder_subtree(
        &self,
        folder_id: &FolderId,
        destination_parent_id: Option<&FolderId>,
    ) -> Result<FolderId> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let folders = self.get_folders_with_connection(&transaction)?;
        let source = folders
            .iter()
            .find(|folder| &folder.id == folder_id)
            .cloned()
            .with_context(|| format!("Folder {} does not exist.", folder_id.as_str()))?;
        let mut subtree_ids = std::collections::HashSet::new();
        collect_folder_subtree_ids_from(&folders, folder_id, &mut subtree_ids);

        let now = Utc::now();
        let mut id_map = std::collections::HashMap::new();
        let mut suffix = next_folder_suffix(&transaction)?;
        for source_id in &subtree_ids {
            let copied_id = FolderId::new(format!(
                "folder-{}-{}",
                now.timestamp_nanos_opt().unwrap_or_default(),
                suffix
            ));
            suffix += 1;
            id_map.insert(source_id.clone(), copied_id);
        }

        let copied_root_id = id_map
            .get(folder_id)
            .cloned()
            .context("Could not allocate copied folder id.")?;
        let root_manual_order =
            self.next_folder_manual_order_with_connection(&transaction, destination_parent_id)?;
        let mut copied_folders = folders
            .iter()
            .filter(|folder| subtree_ids.contains(&folder.id))
            .cloned()
            .collect::<Vec<_>>();
        copied_folders.sort_by_key(|folder| folder_depth(&folders, &folder.id));

        for folder in copied_folders {
            let copied_id = id_map
                .get(&folder.id)
                .cloned()
                .context("Could not map copied folder id.")?;
            let copied_parent_id = if folder.id == source.id {
                destination_parent_id.cloned()
            } else {
                folder
                    .parent_id
                    .as_ref()
                    .and_then(|parent_id| id_map.get(parent_id))
                    .cloned()
            };
            let copied_name = if folder.id == source.id {
                format!("{} Copy", folder.name)
            } else {
                folder.name
            };
            let manual_order = if folder.id == source.id {
                root_manual_order
            } else {
                folder.manual_order
            };

            transaction.execute(
                "INSERT INTO folders
                    (id, name, parent_id, manual_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    copied_id.as_str(),
                    copied_name,
                    copied_parent_id.as_ref().map(FolderId::as_str),
                    manual_order,
                    now.timestamp(),
                ],
            )?;

            let mut memberships = transaction.prepare(
                "SELECT entry_id, manual_order
                 FROM entry_folders
                 WHERE folder_id = ?1
                 ORDER BY manual_order ASC",
            )?;
            let rows = memberships.query_map(params![folder.id.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (entry_id, entry_manual_order) = row?;
                transaction.execute(
                    "INSERT OR IGNORE INTO entry_folders
                        (entry_id, folder_id, manual_order)
                     VALUES (?1, ?2, ?3)",
                    params![entry_id, copied_id.as_str(), entry_manual_order],
                )?;
            }
        }

        transaction.commit()?;
        Ok(copied_root_id)
    }

    /// Moves a folder to a new parent.
    ///
    /// # Errors
    ///
    /// Returns an error when the move is invalid or SQLite cannot write it.
    pub fn move_folder(
        &self,
        folder_id: &FolderId,
        new_parent_id: Option<&FolderId>,
    ) -> Result<()> {
        if new_parent_id == Some(folder_id) {
            anyhow::bail!("A folder cannot be moved into itself.");
        }

        let connection = self.connection()?;
        if let Some(parent_id) = new_parent_id {
            let mut current = Some(parent_id.clone());
            while let Some(id) = current {
                if &id == folder_id {
                    anyhow::bail!("A folder cannot be moved into one of its descendants.");
                }
                current = connection
                    .query_row(
                        "SELECT parent_id FROM folders WHERE id = ?1",
                        params![id.as_str()],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .flatten()
                    .map(FolderId::new);
            }
        }

        let manual_order =
            self.next_folder_manual_order_with_connection(&connection, new_parent_id)?;
        connection.execute(
            "UPDATE folders
             SET parent_id = ?1, manual_order = ?2, updated_at = ?3
             WHERE id = ?4",
            params![
                new_parent_id.map(FolderId::as_str),
                manual_order,
                Utc::now().timestamp(),
                folder_id.as_str()
            ],
        )?;
        Ok(())
    }

    /// Loads persisted library preferences.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query preferences.
    pub fn library_preferences(&self) -> Result<LibraryPreferences> {
        let connection = self.connection()?;
        let mut preferences = LibraryPreferences::default();

        if let Some(value) = self.preference_with_connection(&connection, "sort_mode")? {
            preferences.sort_mode = value.parse().unwrap_or(preferences.sort_mode);
        }
        if let Some(value) = self.preference_with_connection(&connection, "layout_mode")? {
            preferences.layout_mode = value.parse().unwrap_or(preferences.layout_mode);
        }
        preferences.selected_folder = self
            .preference_with_connection(&connection, "selected_folder")?
            .filter(|value| !value.is_empty())
            .map(FolderId::new);
        if let Some(value) = self.preference_with_connection(&connection, "sidebar_width")? {
            preferences.sidebar_width = value.parse().unwrap_or(preferences.sidebar_width);
        }
        if let Some(value) = self.preference_with_connection(&connection, "grid_zoom")? {
            preferences.grid_zoom = value.parse().unwrap_or(preferences.grid_zoom);
        }
        if let Some(value) =
            self.preference_with_connection(&connection, "visible_metadata_fields")?
        {
            preferences.visible_metadata_fields = value
                .split(',')
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
        if let Some(value) =
            self.preference_with_connection(&connection, "library_tree_root_expanded")?
        {
            preferences.library_tree_root_expanded = value.parse().unwrap_or(true);
        }
        if let Some(value) = self.preference_with_connection(&connection, "collapsed_folder_ids")? {
            preferences.collapsed_folder_ids = value
                .split(',')
                .map(str::trim)
                .filter(|folder_id| !folder_id.is_empty())
                .map(FolderId::new)
                .collect();
        }

        Ok(preferences)
    }

    /// Persists library preferences.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write preferences.
    pub fn save_library_preferences(&self, preferences: &LibraryPreferences) -> Result<()> {
        let connection = self.connection()?;
        let visible_metadata_fields = preferences.visible_metadata_fields.join(",");
        let collapsed_folder_ids = preferences
            .collapsed_folder_ids
            .iter()
            .map(FolderId::as_str)
            .collect::<Vec<_>>()
            .join(",");
        self.set_preference_with_connection(
            &connection,
            "sort_mode",
            preferences.sort_mode.as_str(),
        )?;
        self.set_preference_with_connection(
            &connection,
            "layout_mode",
            preferences.layout_mode.as_str(),
        )?;
        self.set_preference_with_connection(
            &connection,
            "selected_folder",
            preferences
                .selected_folder
                .as_ref()
                .map_or("", FolderId::as_str),
        )?;
        self.set_preference_with_connection(
            &connection,
            "sidebar_width",
            &preferences.sidebar_width.to_string(),
        )?;
        self.set_preference_with_connection(
            &connection,
            "grid_zoom",
            &preferences.grid_zoom.to_string(),
        )?;
        self.set_preference_with_connection(
            &connection,
            "visible_metadata_fields",
            &visible_metadata_fields,
        )?;
        self.set_preference_with_connection(
            &connection,
            "library_tree_root_expanded",
            &preferences.library_tree_root_expanded.to_string(),
        )?;
        self.set_preference_with_connection(
            &connection,
            "collapsed_folder_ids",
            &collapsed_folder_ids,
        )?;
        Ok(())
    }

    /// Updates reading progress for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn update_last_page(&self, entry_id: &EntryId, page: u16) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET last_page = ?1, opened_at = ?2 WHERE id = ?3",
            params![i64::from(page), Utc::now().timestamp(), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Updates the most recent open timestamp for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn mark_entry_opened(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET opened_at = ?1 WHERE id = ?2",
            params![Utc::now().timestamp(), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Saves the result of one author attribution attempt for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn update_author_attribution(
        &self,
        entry_id: &EntryId,
        author: Option<&str>,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET author = ?1, author_attributed = 1 WHERE id = ?2",
            params![author, entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Saves the result of one page-count attribution attempt for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn update_page_count_attribution(
        &self,
        entry_id: &EntryId,
        page_count: Option<u16>,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET page_count = ?1, page_count_attributed = 1 WHERE id = ?2",
            params![page_count.map(i64::from), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Returns entries whose author attribution has not been attempted.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entries_needing_author_attribution(&self) -> Result<Vec<LibraryEntry>> {
        Ok(self
            .get_all_entries()?
            .into_iter()
            .filter(|entry| !entry.author_attributed && !entry.missing)
            .collect())
    }

    /// Returns entries whose page count has not been attempted.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entries_needing_page_count_attribution(&self) -> Result<Vec<LibraryEntry>> {
        Ok(self
            .get_all_entries()?
            .into_iter()
            .filter(|entry| !entry.page_count_attributed && !entry.missing)
            .collect())
    }

    /// Adds a tag to an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the tag.
    pub fn add_tag(&self, entry_id: &EntryId, tag: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO tags (entry_id, tag) VALUES (?1, ?2)",
            params![entry_id.as_str(), tag],
        )?;
        Ok(())
    }

    /// Creates or updates the mapping between a Raindrop item and a local entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the mapping.
    pub fn upsert_raindrop_entry_mapping(&self, mapping: &RaindropEntryMapping) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO raindrop_entries
                (source_id, raindrop_id, entry_id, collection_id, remote_link, remote_title,
                 remote_updated_at, file_name, file_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_id, raindrop_id) DO UPDATE SET
                entry_id = excluded.entry_id,
                collection_id = excluded.collection_id,
                remote_link = excluded.remote_link,
                remote_title = excluded.remote_title,
                remote_updated_at = excluded.remote_updated_at,
                file_name = excluded.file_name,
                file_size = excluded.file_size",
            params![
                mapping.source_id,
                mapping.raindrop_id,
                mapping.entry_id.as_str(),
                mapping.collection_id,
                mapping.remote_link,
                mapping.remote_title,
                mapping.remote_updated_at,
                mapping.file_name,
                mapping.file_size.map(|value| value as i64),
            ],
        )?;
        Ok(())
    }

    /// Returns the local entry id currently mapped to a Raindrop item.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the mapping.
    pub fn raindrop_entry_id(&self, source_id: &str, raindrop_id: i64) -> Result<Option<EntryId>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT entry_id FROM raindrop_entries
                 WHERE source_id = ?1 AND raindrop_id = ?2",
                params![source_id, raindrop_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|id| id.map(EntryId::new))
            .context("Could not load Raindrop entry mapping.")
    }

    /// Removes a tag from an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the tag.
    pub fn remove_tag(&self, entry_id: &EntryId, tag: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM tags WHERE entry_id = ?1 AND tag = ?2",
            params![entry_id.as_str(), tag],
        )?;
        Ok(())
    }

    /// Renames a tag everywhere it is used.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the tag rows.
    pub fn rename_tag(&self, old_tag: &str, new_tag: &str) -> Result<usize> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO tags (entry_id, tag)
             SELECT entry_id, ?1 FROM tags WHERE tag = ?2",
            params![new_tag, old_tag],
        )?;
        let removed = transaction.execute("DELETE FROM tags WHERE tag = ?1", params![old_tag])?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Deletes a tag everywhere it is used.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the tag rows.
    pub fn delete_tag(&self, tag: &str) -> Result<usize> {
        let connection = self.connection()?;
        let removed = connection.execute("DELETE FROM tags WHERE tag = ?1", params![tag])?;
        Ok(removed)
    }

    /// Deletes an entry and its dependent rows.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the entry.
    pub fn delete_entry(&self, entry_id: &EntryId) -> Result<()> {
        self.delete_entries([entry_id])
    }

    /// Deletes entries and their dependent rows.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the entries.
    pub fn delete_entries<'a>(
        &self,
        entry_ids: impl IntoIterator<Item = &'a EntryId>,
    ) -> Result<()> {
        const DELETE_CHUNK_SIZE: usize = 900;

        let entry_ids = entry_ids.into_iter().collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(());
        }

        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;

        for chunk in entry_ids.chunks(DELETE_CHUNK_SIZE) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("DELETE FROM entries WHERE id IN ({placeholders})");
            transaction.execute(
                &sql,
                params_from_iter(chunk.iter().map(|entry_id| entry_id.as_str())),
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    /// Moves entries to the trash without deleting their metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entries.
    pub fn trash_entries<'a>(
        &self,
        entry_ids: impl IntoIterator<Item = &'a EntryId>,
    ) -> Result<()> {
        self.set_entries_trashed_at(entry_ids, Some(Utc::now().timestamp()))
    }

    /// Restores entries from the trash.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entries.
    pub fn restore_entries<'a>(
        &self,
        entry_ids: impl IntoIterator<Item = &'a EntryId>,
    ) -> Result<()> {
        self.set_entries_trashed_at(entry_ids, None)
    }

    /// Permanently removes trash items older than `retention_days`.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete expired trash items.
    pub fn purge_expired_trash(&self, retention_days: i64) -> Result<usize> {
        let cutoff = Utc::now().timestamp() - retention_days.max(0) * 24 * 60 * 60;
        let connection = self.connection()?;
        let deleted_entries = connection.execute(
            "DELETE FROM entries WHERE trashed_at IS NOT NULL AND trashed_at < ?1",
            params![cutoff],
        )?;
        let deleted_folders = connection.execute(
            "DELETE FROM folders WHERE trashed_at IS NOT NULL AND trashed_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted_entries + deleted_folders)
    }

    /// Marks an entry as missing or present without deleting its metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn set_missing(&self, entry_id: &EntryId, missing: bool) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET missing = ?1 WHERE id = ?2",
            params![i64::from(missing), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Marks an entry as missing or present by its source path.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn set_missing_by_path(&self, path: &Path, missing: bool) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET missing = ?1 WHERE path = ?2",
            params![i64::from(missing), path.to_string_lossy()],
        )?;
        Ok(())
    }

    /// Updates an entry's source path and marks it present.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn relink_entry_path(&self, entry_id: &EntryId, path: &Path) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET path = ?1, missing = 0 WHERE id = ?2",
            params![path.to_string_lossy(), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Returns the entry with the given path, if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entry_by_path(&self, path: &Path) -> Result<Option<LibraryEntry>> {
        let connection = self.connection()?;
        let path = path.to_string_lossy();
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, file_size, last_page, rating, cover_hash, missing
             FROM entries
             WHERE path = ?1",
        )?;
        let mut entry = statement
            .query_row(params![path], row_to_entry)
            .optional()
            .context("Could not load library entry by path.")?;

        if let Some(entry) = &mut entry {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id, true)?;
        }

        Ok(entry)
    }

    /// Returns all tags currently used in the library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query tags.
    pub fn all_tags(&self) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT DISTINCT tag FROM tags ORDER BY tag")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load library tags.")
    }

    fn tags_for_entry_with_connection(
        &self,
        connection: &Connection,
        entry_id: &EntryId,
    ) -> Result<Vec<String>> {
        let mut statement =
            connection.prepare("SELECT tag FROM tags WHERE entry_id = ?1 ORDER BY tag")?;
        let rows =
            statement.query_map(params![entry_id.as_str()], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry tags.")
    }

    fn folders_for_entry_with_connection(
        &self,
        connection: &Connection,
        entry_id: &EntryId,
        include_trashed: bool,
    ) -> Result<Vec<Folder>> {
        let trash_filter = if include_trashed {
            ""
        } else {
            " AND f.trashed_at IS NULL"
        };
        let mut statement = connection.prepare(&format!(
            "SELECT f.id, f.name, f.parent_id, f.manual_order, f.created_at, f.updated_at
             FROM folders f
             INNER JOIN entry_folders ef ON ef.folder_id = f.id
             WHERE ef.entry_id = ?1{trash_filter}
             ORDER BY ef.manual_order ASC, f.name ASC"
        ))?;
        let rows = statement.query_map(params![entry_id.as_str()], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry folders.")
    }

    fn attach_entry_collections_with_connection(
        &self,
        connection: &Connection,
        entries: &mut [LibraryEntry],
        include_trashed_folders: bool,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let entry_ids = entries
            .iter()
            .map(|entry| entry.id.as_str().to_owned())
            .collect::<Vec<_>>();
        let placeholders = std::iter::repeat("?")
            .take(entry_ids.len())
            .collect::<Vec<_>>()
            .join(",");

        let mut tags_by_entry = std::collections::HashMap::<String, Vec<String>>::new();
        let mut tag_statement = connection.prepare(&format!(
            "SELECT entry_id, tag
             FROM tags
             WHERE entry_id IN ({placeholders})
             ORDER BY entry_id ASC, tag ASC"
        ))?;
        let tag_rows = tag_statement.query_map(params_from_iter(entry_ids.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in tag_rows {
            let (entry_id, tag) = row.context("Could not load entry tag.")?;
            tags_by_entry.entry(entry_id).or_default().push(tag);
        }

        let trash_filter = if include_trashed_folders {
            ""
        } else {
            " AND f.trashed_at IS NULL"
        };
        let mut folders_by_entry = std::collections::HashMap::<String, Vec<Folder>>::new();
        let mut folder_orders_by_entry =
            std::collections::HashMap::<String, Vec<EntryFolderMembership>>::new();
        let mut folder_statement = connection.prepare(&format!(
            "SELECT ef.entry_id, f.id, f.name, f.parent_id, f.manual_order, f.created_at, f.updated_at, ef.manual_order
             FROM entry_folders ef
             INNER JOIN folders f ON f.id = ef.folder_id
             WHERE ef.entry_id IN ({placeholders}){trash_filter}
             ORDER BY ef.entry_id ASC, ef.manual_order ASC, f.name ASC"
        ))?;
        let folder_rows =
            folder_statement.query_map(params_from_iter(entry_ids.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    Folder {
                        id: FolderId::new(row.get::<_, String>(1)?),
                        name: row.get(2)?,
                        parent_id: row.get::<_, Option<String>>(3)?.map(FolderId::new),
                        manual_order: row.get(4)?,
                        created_at: DateTime::from_timestamp(row.get(5)?, 0)
                            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
                        updated_at: DateTime::from_timestamp(row.get(6)?, 0)
                            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
                    },
                    row.get::<_, i64>(7)?,
                ))
            })?;
        for row in folder_rows {
            let (entry_id, folder, folder_entry_manual_order) =
                row.context("Could not load entry folder.")?;
            folder_orders_by_entry
                .entry(entry_id.clone())
                .or_default()
                .push(EntryFolderMembership {
                    entry_id: EntryId::new(entry_id.clone()),
                    folder_id: folder.id.clone(),
                    manual_order: folder_entry_manual_order,
                });
            folders_by_entry.entry(entry_id).or_default().push(folder);
        }

        for entry in entries {
            entry.tags = tags_by_entry.remove(entry.id.as_str()).unwrap_or_default();
            entry.folders = folders_by_entry
                .remove(entry.id.as_str())
                .unwrap_or_default();
            entry.folder_orders = folder_orders_by_entry
                .remove(entry.id.as_str())
                .unwrap_or_default();
        }

        Ok(())
    }

    fn get_folders_with_connection(&self, connection: &Connection) -> Result<Vec<Folder>> {
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, manual_order, created_at, updated_at
             FROM folders
             WHERE trashed_at IS NULL
             ORDER BY COALESCE(parent_id, ''), manual_order ASC, name ASC",
        )?;
        let rows = statement.query_map([], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load folders.")
    }

    fn library_organization_snapshot_with_connection(
        &self,
        connection: &Connection,
    ) -> Result<LibraryOrganizationSnapshot> {
        let mut folder_statement = connection.prepare(
            "SELECT id, name, parent_id, manual_order, created_at, updated_at, trashed_at
             FROM folders
             ORDER BY COALESCE(parent_id, ''), manual_order ASC, name ASC",
        )?;
        let folders = folder_statement.query_map([], row_to_folder_snapshot)?;
        let folders = folders
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load folder snapshot.")?;

        let mut membership_statement = connection.prepare(
            "SELECT entry_id, folder_id, manual_order
             FROM entry_folders
             ORDER BY folder_id, manual_order ASC, entry_id",
        )?;
        let rows = membership_statement.query_map([], |row| {
            Ok(EntryFolderMembership {
                entry_id: EntryId::new(row.get::<_, String>(0)?),
                folder_id: FolderId::new(row.get::<_, String>(1)?),
                manual_order: row.get(2)?,
            })
        })?;
        let entry_folders = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry folder memberships.")?;
        let mut entry_statement = connection.prepare(
            "SELECT id, display_title, display_author, sort_title, sort_author,
                    metadata_locked, manual_order, trashed_at
             FROM entries
             ORDER BY id",
        )?;
        let rows = entry_statement.query_map([], |row| {
            let trashed_at: Option<i64> = row.get(7)?;
            Ok(EntryTrashState {
                entry_id: EntryId::new(row.get::<_, String>(0)?),
                display_title: row.get(1)?,
                display_author: row.get(2)?,
                sort_title: row.get(3)?,
                sort_author: row.get(4)?,
                metadata_locked: row.get::<_, i64>(5)? != 0,
                manual_order: row.get(6)?,
                trashed_at: trashed_at.and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
            })
        })?;
        let entry_trash_states = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry trash states.")?;
        let mut tag_statement = connection.prepare(
            "SELECT entry_id, tag
             FROM tags
             ORDER BY entry_id, tag",
        )?;
        let rows = tag_statement.query_map([], |row| {
            Ok(EntryTagSnapshot {
                entry_id: EntryId::new(row.get::<_, String>(0)?),
                tag: row.get(1)?,
            })
        })?;
        let entry_tags = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry tags snapshot.")?;
        Ok(LibraryOrganizationSnapshot {
            folders,
            entry_folders,
            entry_trash_states,
            entry_tags,
        })
    }

    fn preference_with_connection(
        &self,
        connection: &Connection,
        key: &str,
    ) -> Result<Option<String>> {
        connection
            .query_row(
                "SELECT value FROM library_preferences WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("Could not load library preference.")
    }

    fn set_preference_with_connection(
        &self,
        connection: &Connection,
        key: &str,
        value: &str,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO library_preferences (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    fn next_entry_manual_order_with_connection(&self, connection: &Connection) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM entries WHERE trashed_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(max_order.unwrap_or(0) + MANUAL_ORDER_GAP)
    }

    fn next_folder_manual_order_with_connection(
        &self,
        connection: &Connection,
        parent_id: Option<&FolderId>,
    ) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM folders WHERE parent_id IS ?1 AND trashed_at IS NULL",
            params![parent_id.map(FolderId::as_str)],
            |row| row.get(0),
        )?;
        Ok(max_order.unwrap_or(0) + MANUAL_ORDER_GAP)
    }

    fn next_folder_entry_manual_order_with_connection(
        &self,
        connection: &Connection,
        folder_id: &FolderId,
    ) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM entry_folders WHERE folder_id = ?1",
            params![folder_id.as_str()],
            |row| row.get(0),
        )?;
        Ok(max_order.unwrap_or(0) + MANUAL_ORDER_GAP)
    }

    fn set_entries_trashed_at<'a>(
        &self,
        entry_ids: impl IntoIterator<Item = &'a EntryId>,
        trashed_at: Option<i64>,
    ) -> Result<()> {
        const UPDATE_CHUNK_SIZE: usize = 900;

        let entry_ids = entry_ids.into_iter().collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(());
        }

        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        for chunk in entry_ids.chunks(UPDATE_CHUNK_SIZE) {
            let placeholders = std::iter::repeat_n("?", chunk.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("UPDATE entries SET trashed_at = ? WHERE id IN ({placeholders})");
            let values = std::iter::once(trashed_at.map_or(
                rusqlite::types::Value::Null,
                rusqlite::types::Value::Integer,
            ))
            .chain(
                chunk
                    .iter()
                    .map(|entry_id| rusqlite::types::Value::Text(entry_id.as_str().to_owned())),
            );
            transaction.execute(&sql, params_from_iter(values))?;
        }

        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path).with_context(|| {
            format!("Could not open library database: {}.", self.path.display())
        })?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn migrate(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS entries (
                id          TEXT PRIMARY KEY,
                path        TEXT NOT NULL UNIQUE,
                title       TEXT,
                author      TEXT,
                display_title TEXT,
                display_author TEXT,
                sort_title TEXT,
                sort_author TEXT,
                metadata_locked INTEGER DEFAULT 0 NOT NULL,
                manual_order INTEGER DEFAULT 0 NOT NULL,
                author_attributed INTEGER DEFAULT 0 NOT NULL,
                page_count_attributed INTEGER DEFAULT 0 NOT NULL,
                added_at    INTEGER NOT NULL,
                opened_at   INTEGER,
                page_count  INTEGER,
                file_size   INTEGER,
                last_page   INTEGER DEFAULT 0,
                rating      INTEGER DEFAULT 0,
                cover_hash  TEXT,
                missing     INTEGER DEFAULT 0 NOT NULL,
                trashed_at  INTEGER
            );

            CREATE TABLE IF NOT EXISTS tags (
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                tag         TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag)
            );

            CREATE TABLE IF NOT EXISTS bookmarks (
                id          TEXT PRIMARY KEY,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                page        INTEGER NOT NULL,
                label       TEXT,
                created_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS annotations (
                id          TEXT PRIMARY KEY,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                page        INTEGER NOT NULL,
                kind        TEXT NOT NULL,
                data        TEXT NOT NULL,
                created_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS folders (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                parent_id   TEXT REFERENCES folders(id) ON DELETE CASCADE,
                manual_order INTEGER NOT NULL,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL,
                trashed_at  INTEGER
            );

            CREATE TABLE IF NOT EXISTS entry_folders (
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                folder_id   TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                manual_order INTEGER NOT NULL,
                PRIMARY KEY (entry_id, folder_id)
            );

            CREATE TABLE IF NOT EXISTS library_preferences (
                key         TEXT PRIMARY KEY,
                value       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS import_sources (
                id          TEXT PRIMARY KEY,
                kind        TEXT NOT NULL,
                account_id  TEXT,
                display_name TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS raindrop_collections (
                source_id   TEXT NOT NULL REFERENCES import_sources(id) ON DELETE CASCADE,
                collection_id INTEGER NOT NULL,
                folder_id   TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                parent_collection_id INTEGER,
                title       TEXT NOT NULL,
                PRIMARY KEY (source_id, collection_id)
            );

            CREATE TABLE IF NOT EXISTS raindrop_entries (
                source_id   TEXT NOT NULL REFERENCES import_sources(id) ON DELETE CASCADE,
                raindrop_id INTEGER NOT NULL,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                collection_id INTEGER,
                remote_link TEXT,
                remote_title TEXT,
                remote_updated_at TEXT,
                file_name   TEXT,
                file_size   INTEGER,
                PRIMARY KEY (source_id, raindrop_id)
            );

            CREATE TABLE IF NOT EXISTS sync_entries (
                id          TEXT NOT NULL,
                library_id  TEXT NOT NULL,
                title       TEXT,
                author      TEXT,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (id, library_id)
            );

            CREATE TABLE IF NOT EXISTS sync_folders (
                id          TEXT NOT NULL,
                library_id  TEXT NOT NULL,
                name        TEXT NOT NULL,
                parent_id   TEXT,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (id, library_id)
            );

            CREATE TABLE IF NOT EXISTS sync_entry_folders (
                entry_id    TEXT NOT NULL,
                folder_id   TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (entry_id, folder_id)
            );

            CREATE TABLE IF NOT EXISTS sync_checkpoints (
                library_id      TEXT NOT NULL,
                device_id       TEXT NOT NULL,
                last_synced_at  INTEGER NOT NULL,
                PRIMARY KEY (library_id, device_id)
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_operations (
                op_id           TEXT PRIMARY KEY,
                library_id      TEXT NOT NULL,
                device_id       TEXT NOT NULL,
                logical_time    INTEGER NOT NULL,
                entity_kind     TEXT NOT NULL,
                entity_id       TEXT NOT NULL,
                payload         TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                remote_sequence INTEGER,
                pushed_at       INTEGER
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_entity_versions (
                library_id      TEXT NOT NULL,
                entity_kind     TEXT NOT NULL,
                entity_id       TEXT NOT NULL,
                payload_hash    TEXT NOT NULL,
                logical_time    INTEGER NOT NULL,
                device_id       TEXT NOT NULL,
                PRIMARY KEY (library_id, entity_kind, entity_id)
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_checkpoints (
                library_id            TEXT NOT NULL,
                device_id             TEXT NOT NULL,
                last_remote_sequence  INTEGER NOT NULL,
                PRIMARY KEY (library_id, device_id)
            );

            CREATE TABLE IF NOT EXISTS sync_blob_uploads (
                hash        TEXT PRIMARY KEY,
                uploaded_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sync_crdt_operations_library_entity
                ON sync_crdt_operations(library_id, entity_kind, entity_id);

            CREATE INDEX IF NOT EXISTS idx_sync_crdt_operations_pending
                ON sync_crdt_operations(library_id, device_id, pushed_at);

            INSERT OR IGNORE INTO schema_version (version) VALUES (1);
            "#,
        )?;
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN missing INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN author_attributed INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN page_count_attributed INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN display_title TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN display_author TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN sort_title TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN sort_author TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN file_size INTEGER", []);
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN metadata_locked INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN manual_order INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN trashed_at INTEGER", []);
        let _ = connection.execute("ALTER TABLE folders ADD COLUMN trashed_at INTEGER", []);
        connection.execute(
            "UPDATE entries
             SET manual_order = rowid * ?1
             WHERE manual_order = 0",
            params![MANUAL_ORDER_GAP],
        )?;
        connection.execute(
            "UPDATE entries
             SET sort_title = lower(COALESCE(display_title, title))
             WHERE sort_title IS NULL AND COALESCE(display_title, title) IS NOT NULL",
            [],
        )?;
        connection.execute(
            "UPDATE entries
             SET sort_author = lower(COALESCE(display_author, author))
             WHERE sort_author IS NULL AND COALESCE(display_author, author) IS NOT NULL",
            [],
        )?;
        backfill_file_sizes(&connection)?;
        Ok(())
    }
}

fn backfill_file_sizes(connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT id, path FROM entries WHERE file_size IS NULL AND missing = 0")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let entries = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    for (id, path) in entries {
        if let Ok(metadata) = std::fs::metadata(&path) {
            connection.execute(
                "UPDATE entries SET file_size = ?1 WHERE id = ?2",
                params![metadata.len() as i64, id],
            )?;
        }
    }
    Ok(())
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryEntry> {
    let added_at: i64 = row.get(12)?;
    let opened_at: Option<i64> = row.get(13)?;
    let page_count: Option<i64> = row.get(14)?;
    let file_size: Option<i64> = row.get(15)?;
    let last_page: i64 = row.get(16)?;
    let rating: i64 = row.get(17)?;

    Ok(LibraryEntry {
        id: EntryId::new(row.get::<_, String>(0)?),
        path: PathBuf::from(row.get::<_, String>(1)?),
        title: row.get(2)?,
        author: row.get(3)?,
        display_title: row.get(4)?,
        display_author: row.get(5)?,
        sort_title: row.get(6)?,
        sort_author: row.get(7)?,
        metadata_locked: row.get::<_, i64>(8)? != 0,
        manual_order: row.get(9)?,
        author_attributed: row.get::<_, i64>(10)? != 0,
        page_count_attributed: row.get::<_, i64>(11)? != 0,
        added_at: DateTime::from_timestamp(added_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        opened_at: opened_at.and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
        page_count: page_count.map(|value| value as u16),
        file_size: file_size.map(|value| value as u64),
        last_page: last_page as u16,
        rating: rating as u8,
        cover_hash: row.get(18)?,
        tags: Vec::new(),
        folders: Vec::new(),
        folder_orders: Vec::new(),
        missing: row.get::<_, i64>(19)? != 0,
    })
}

fn row_to_folder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    Ok(Folder {
        id: FolderId::new(row.get::<_, String>(0)?),
        name: row.get(1)?,
        parent_id: row.get::<_, Option<String>>(2)?.map(FolderId::new),
        manual_order: row.get(3)?,
        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
    })
}

fn row_to_sync_crdt_operation(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncCrdtOperation> {
    Ok(SyncCrdtOperation {
        op_id: row.get(0)?,
        library_id: row.get(1)?,
        device_id: row.get(2)?,
        logical_time: row.get(3)?,
        entity_kind: row.get(4)?,
        entity_id: row.get(5)?,
        payload: row.get(6)?,
        created_at: row.get(7)?,
        remote_sequence: row.get(8)?,
        pushed_at: row.get(9)?,
    })
}

fn row_to_folder_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryFolderSnapshot> {
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    let trashed_at: Option<i64> = row.get(6)?;
    Ok(LibraryFolderSnapshot {
        id: FolderId::new(row.get::<_, String>(0)?),
        name: row.get(1)?,
        parent_id: row.get::<_, Option<String>>(2)?.map(FolderId::new),
        manual_order: row.get(3)?,
        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        trashed_at: trashed_at.and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
    })
}

fn collect_folder_subtree_ids_from(
    folders: &[Folder],
    folder_id: &FolderId,
    folder_ids: &mut std::collections::HashSet<FolderId>,
) {
    if !folder_ids.insert(folder_id.clone()) {
        return;
    }
    for child in folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
    {
        collect_folder_subtree_ids_from(folders, &child.id, folder_ids);
    }
}

fn collect_folder_snapshot_subtree_ids_from(
    folders: &[LibraryFolderSnapshot],
    folder_id: &FolderId,
    folder_ids: &mut std::collections::HashSet<FolderId>,
) {
    if !folder_ids.insert(folder_id.clone()) {
        return;
    }
    for child in folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
    {
        collect_folder_snapshot_subtree_ids_from(folders, &child.id, folder_ids);
    }
}

fn folder_depth(folders: &[Folder], folder_id: &FolderId) -> usize {
    let mut depth = 0;
    let mut current = folders
        .iter()
        .find(|folder| &folder.id == folder_id)
        .and_then(|folder| folder.parent_id.as_ref());
    while let Some(parent_id) = current {
        depth += 1;
        current = folders
            .iter()
            .find(|folder| &folder.id == parent_id)
            .and_then(|folder| folder.parent_id.as_ref());
    }
    depth
}

fn row_to_import_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImportSource> {
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    Ok(ImportSource {
        id: row.get(0)?,
        kind: row.get(1)?,
        account_id: row.get(2)?,
        display_name: row.get(3)?,
        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
    })
}

#[cfg(test)]
mod tests;
