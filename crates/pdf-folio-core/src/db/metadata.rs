//! Library display metadata, preferences, and attribution state.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::naming::{clean_optional_text, clean_title_sort_key, sort_key};
use super::{Db, EntryId, FolderId, LibraryEntry, LibraryPreferences};

impl Db {
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

}
