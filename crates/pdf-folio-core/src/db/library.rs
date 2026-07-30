//! Library entry CRUD, tagging, trash, and file-presence state.
//!
//! Implements the core [`Db`] methods for PDF rows in the `entries` table:
//! insert/upsert, sorted listing, soft-delete (trash) and hard delete, tags,
//! and marking files missing when the filesystem path disappears.
//!
//! Entry IDs are content-derived (see [`crate::hash_file`]); `insert_entry`
//! merges on conflict so re-importing the same PDF updates path and metadata
//! without duplicating the row. Sorted queries exclude trashed entries; use
//! [`Db::get_trashed_entries`] for the trash can view.
//!
//! # See also
//!
//! - [`super::organization`] for folder membership and manual order.
//! - [`super::metadata`] for display overrides, ratings, and reading progress.
//! - [`crate::LibraryEntry`] / [`crate::NewLibraryEntry`] for row shapes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use super::naming::{sort_key, MANUAL_ORDER_GAP};
use super::organization::row_to_folder;
use super::{
    Db, EntryFolderMembership, EntryId, Folder, FolderId, LibraryEntry, LibrarySortMode,
    NewLibraryEntry,
};

impl Db {
    /// Inserts or upserts a library entry by content id.
    ///
    /// On conflict, path and several metadata fields are updated, the entry is
    /// marked present (`missing = 0`), and trash state is cleared. Existing
    /// sort keys and attribution flags are preserved unless the incoming row
    /// supplies stronger attribution.
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

    /// Loads tags for one entry using an existing connection (sorted alphabetically).
    pub(super) fn tags_for_entry_with_connection(
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

    /// Loads folders containing `entry_id`, optionally including trashed folders.
    pub(super) fn folders_for_entry_with_connection(
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
        let placeholders = std::iter::repeat_n("?", entry_ids.len())
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

    /// Next root-library manual order rank (max existing + naming gap constant).
    pub(super) fn next_entry_manual_order_with_connection(
        &self,
        connection: &Connection,
    ) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM entries WHERE trashed_at IS NULL",
            [],
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

/// Maps a SELECT of the standard entry columns into a [`crate::LibraryEntry`]
/// (tags and folders left empty for the caller to fill).
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
