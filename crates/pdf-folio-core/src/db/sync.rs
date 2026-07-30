//! Sync metadata, CRDT operation log, and blob upload persistence.
//!
//! Local tables that mirror what the cloud sync stack needs without requiring
//! a network connection: denormalized entry/folder/membership metadata
//! (`sync_*` tables), an append-only CRDT operation log with per-entity
//! versions and device checkpoints, and a set of content-hash blob upload
//! markers so PDF bytes are not re-uploaded blindly.
//!
//! The cloud crate (`pdf-folio-cloud`) seeds these tables from the main library
//! rows, prepares CRDT ops, pushes/pulls against Turso/R2, and hydrates
//! missing local entries. This module is the SQLite half of that pipeline.
//!
//! # Key types
//!
//! - [`crate::SyncEntryRow`], [`crate::SyncFolderRow`],
//!   [`crate::SyncEntryFolderRow`] — LWW-friendly metadata rows.
//! - [`crate::SyncCrdtOperation`] — immutable ops with hybrid logical time.
//! - [`crate::SyncSeedSummary`], [`crate::SyncCrdtPrepareSummary`] — batch counters.
//!
//! # See also
//!
//! - [`super::schema`] for table creation.
//! - [`super::library`] / [`super::organization`] for source-of-truth library data.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use super::library::row_to_entry;
use super::naming::sort_key;
use super::*;

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
}

/// Maps a `sync_crdt_operations` SELECT row into a [`SyncCrdtOperation`].
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
