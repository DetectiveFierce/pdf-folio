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
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

mod naming;

use naming::{sort_key, MANUAL_ORDER_GAP};
use organization::row_to_folder;

mod types;
pub use types::*;

pub mod import;
pub mod metadata;
pub mod organization;
pub mod raindrop;
pub mod schema;
pub mod search;

/// SQLite-backed PDF-Folio library database.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
}

impl Db {
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

    fn next_entry_manual_order_with_connection(&self, connection: &Connection) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM entries WHERE trashed_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(max_order.unwrap_or(0) + MANUAL_ORDER_GAP)
    }

    pub(super) fn next_folder_manual_order_with_connection(
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
}

pub(super) fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryEntry> {
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
