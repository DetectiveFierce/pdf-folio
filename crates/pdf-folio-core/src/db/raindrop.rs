//! Raindrop.io import mapping persistence.
//!
//! Bridges remote Raindrop collections/items to local folders and entries so
//! re-imports stay idempotent. Collection mappings create or update a
//! [`crate::Folder`] and store the remote collection id in
//! `raindrop_collections`; entry mappings link a Raindrop item id to an
//! [`crate::EntryId`] in `raindrop_entries`.
//!
//! Parent collections should be upserted before children so local parent
//! folders resolve correctly. Actual HTTP download and matching live in
//! `pdf-folio-cloud`; this module only persists what that crate discovers.
//!
//! # See also
//!
//! - [`crate::RaindropEntryMapping`] for entry provenance rows.
//! - [`crate::Db::upsert_import_source`] for the parent import-source row.
//! - [`super::organization`] for general folder APIs used after mapping.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use super::naming::{clean_folder_name, next_folder_suffix};
use super::{Db, EntryId, FolderId, RaindropEntryMapping};

impl Db {
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
}
