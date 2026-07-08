//! Folder tree, manual ordering, and library organization snapshots.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

use super::naming::{clean_folder_name, next_folder_suffix, MANUAL_ORDER_GAP};
use super::{
    Db, EntryFolderMembership, EntryId, EntryTagSnapshot, EntryTrashState, Folder, FolderId,
    LibraryEntry, LibraryFolderSnapshot, LibraryOrganizationSnapshot, row_to_entry,
};

impl Db {
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

}

pub(super) fn row_to_folder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
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
