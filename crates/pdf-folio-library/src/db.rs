//! SQLite database setup and library entry queries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};

const MANUAL_ORDER_GAP: i64 = 1024;

/// Stable library entry identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(String);

impl EntryId {
    /// Creates an entry identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable library folder identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderId(String);

impl FolderId {
    /// Creates a folder identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// User-managed PDF folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// Stable folder identifier.
    pub id: FolderId,
    /// User-visible folder name.
    pub name: String,
    /// Optional parent folder.
    pub parent_id: Option<FolderId>,
    /// Stable manual order among sibling folders.
    pub manual_order: i64,
    /// Folder creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Folder update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Library layout preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryLayoutMode {
    /// Grid of PDF cards.
    Grid,
    /// Dense list of PDF rows.
    List,
}

impl LibraryLayoutMode {
    /// Returns the stable string stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::List => "list",
        }
    }
}

impl std::str::FromStr for LibraryLayoutMode {
    type Err = ();

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "grid" => Ok(Self::Grid),
            "list" => Ok(Self::List),
            _ => Err(()),
        }
    }
}

/// Library sort preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySortMode {
    /// User-managed ordering.
    Manual,
    /// Title, ascending.
    TitleAsc,
    /// Title, descending.
    TitleDesc,
    /// Author, ascending.
    AuthorAsc,
    /// Author, descending.
    AuthorDesc,
    /// Recently added PDFs first.
    RecentlyAdded,
    /// Recently opened PDFs first.
    RecentlyOpened,
    /// Most progress first.
    ReadingProgress,
    /// Page count, descending.
    PageCount,
    /// Missing files first.
    MissingFiles,
}

impl LibrarySortMode {
    /// Returns the stable string stored in SQLite.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
            Self::AuthorAsc => "author_asc",
            Self::AuthorDesc => "author_desc",
            Self::RecentlyAdded => "recently_added",
            Self::RecentlyOpened => "recently_opened",
            Self::ReadingProgress => "reading_progress",
            Self::PageCount => "page_count",
            Self::MissingFiles => "missing_files",
        }
    }

    /// Returns the user-facing label for this sort mode.
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::TitleAsc => "Title A-Z",
            Self::TitleDesc => "Title Z-A",
            Self::AuthorAsc => "Author A-Z",
            Self::AuthorDesc => "Author Z-A",
            Self::RecentlyAdded => "Recently Added",
            Self::RecentlyOpened => "Recently Opened",
            Self::ReadingProgress => "Progress",
            Self::PageCount => "Page Count",
            Self::MissingFiles => "Missing",
        }
    }
}

impl std::fmt::Display for LibrarySortMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

impl std::str::FromStr for LibrarySortMode {
    type Err = ();

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "manual" => Ok(Self::Manual),
            "title_asc" => Ok(Self::TitleAsc),
            "title_desc" => Ok(Self::TitleDesc),
            "author_asc" => Ok(Self::AuthorAsc),
            "author_desc" => Ok(Self::AuthorDesc),
            "recently_added" => Ok(Self::RecentlyAdded),
            "recently_opened" => Ok(Self::RecentlyOpened),
            "reading_progress" => Ok(Self::ReadingProgress),
            "page_count" => Ok(Self::PageCount),
            "missing_files" => Ok(Self::MissingFiles),
            _ => Err(()),
        }
    }
}

/// Persisted library view preferences.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryPreferences {
    /// Active sort mode.
    pub sort_mode: LibrarySortMode,
    /// Active layout mode.
    pub layout_mode: LibraryLayoutMode,
    /// Selected folder filter.
    pub selected_folder: Option<FolderId>,
    /// Last sidebar width.
    pub sidebar_width: f32,
    /// Metadata fields visible in cards/rows.
    pub visible_metadata_fields: Vec<String>,
}

impl Default for LibraryPreferences {
    fn default() -> Self {
        Self {
            sort_mode: LibrarySortMode::RecentlyAdded,
            layout_mode: LibraryLayoutMode::Grid,
            selected_folder: None,
            sidebar_width: 112.0,
            visible_metadata_fields: vec![
                String::from("author"),
                String::from("page_count"),
                String::from("file_size"),
            ],
        }
    }
}

/// A PDF entry stored in the local library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// Stable content-derived identifier.
    pub id: EntryId,
    /// Absolute or user-selected path to the PDF.
    pub path: PathBuf,
    /// Optional document title.
    pub title: Option<String>,
    /// Optional document author.
    pub author: Option<String>,
    /// User override for the displayed title.
    pub display_title: Option<String>,
    /// User override for the displayed author.
    pub display_author: Option<String>,
    /// Normalized value used for title sorting.
    pub sort_title: Option<String>,
    /// Normalized value used for author sorting.
    pub sort_author: Option<String>,
    /// True when extracted metadata should not overwrite display metadata.
    pub metadata_locked: bool,
    /// Stable manual order in the root library.
    pub manual_order: i64,
    /// True once author attribution has been attempted for this entry.
    pub author_attributed: bool,
    /// True once page-count attribution has been attempted for this entry.
    pub page_count_attributed: bool,
    /// Timestamp when the entry was added.
    pub added_at: DateTime<Utc>,
    /// Most recent open timestamp.
    pub opened_at: Option<DateTime<Utc>>,
    /// Page count, if known.
    pub page_count: Option<u16>,
    /// Last zero-based page read by the user.
    pub last_page: u16,
    /// User rating from 0 to 5.
    pub rating: u8,
    /// Hash of the cached cover thumbnail bytes.
    pub cover_hash: Option<String>,
    /// User tags attached to the entry.
    pub tags: Vec<String>,
    /// Folders containing the entry.
    pub folders: Vec<Folder>,
    /// True when the source file disappeared from disk.
    pub missing: bool,
}

/// Input data for creating a library entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLibraryEntry {
    /// Stable content-derived identifier.
    pub id: EntryId,
    /// Path to the PDF.
    pub path: PathBuf,
    /// Optional document title.
    pub title: Option<String>,
    /// Optional document author.
    pub author: Option<String>,
    /// True once author attribution has been attempted for this entry.
    pub author_attributed: bool,
    /// True once page-count attribution has been attempted for this entry.
    pub page_count_attributed: bool,
    /// Page count, if known.
    pub page_count: Option<u16>,
    /// Hash of the cached cover thumbnail bytes.
    pub cover_hash: Option<String>,
}

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
                (id, path, title, author, sort_title, sort_author, manual_order, author_attributed, page_count_attributed, added_at, page_count, cover_hash, missing)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0)
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
                cover_hash = COALESCE(excluded.cover_hash, entries.cover_hash),
                missing = 0",
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
                "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, last_page, rating, cover_hash, missing
             FROM entries
             ORDER BY {order_by}"
            ),
        )?;

        let rows = statement.query_map([], row_to_entry)?;

        let mut entries = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load library entries.")?;
        for entry in &mut entries {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id)?;
        }
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

    /// Returns all folders in manual order.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query folders.
    pub fn get_folders(&self) -> Result<Vec<Folder>> {
        let connection = self.connection()?;
        self.get_folders_with_connection(&connection)
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
            "SELECT e.id, e.path, e.title, e.author, e.display_title, e.display_author, e.sort_title, e.sort_author, e.metadata_locked, e.manual_order, e.author_attributed, e.page_count_attributed, e.added_at, e.opened_at, e.page_count, e.last_page, e.rating, e.cover_hash, e.missing
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
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id)?;
        }
        Ok(entries)
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
            "visible_metadata_fields",
            &visible_metadata_fields,
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

    /// Deletes an entry and its dependent rows.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the entry.
    pub fn delete_entry(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM entries WHERE id = ?1",
            params![entry_id.as_str()],
        )?;
        Ok(())
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

    /// Returns the entry with the given path, if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entry_by_path(&self, path: &Path) -> Result<Option<LibraryEntry>> {
        let connection = self.connection()?;
        let path = path.to_string_lossy();
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, display_title, display_author, sort_title, sort_author, metadata_locked, manual_order, author_attributed, page_count_attributed, added_at, opened_at, page_count, last_page, rating, cover_hash, missing
             FROM entries
             WHERE path = ?1",
        )?;
        let mut entry = statement
            .query_row(params![path], row_to_entry)
            .optional()
            .context("Could not load library entry by path.")?;

        if let Some(entry) = &mut entry {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id)?;
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
    ) -> Result<Vec<Folder>> {
        let mut statement = connection.prepare(
            "SELECT f.id, f.name, f.parent_id, f.manual_order, f.created_at, f.updated_at
             FROM folders f
             INNER JOIN entry_folders ef ON ef.folder_id = f.id
             WHERE ef.entry_id = ?1
             ORDER BY ef.manual_order ASC, f.name ASC",
        )?;
        let rows = statement.query_map(params![entry_id.as_str()], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry folders.")
    }

    fn get_folders_with_connection(&self, connection: &Connection) -> Result<Vec<Folder>> {
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, manual_order, created_at, updated_at
             FROM folders
             ORDER BY COALESCE(parent_id, ''), manual_order ASC, name ASC",
        )?;
        let rows = statement.query_map([], row_to_folder)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load folders.")
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
        let max_order: Option<i64> =
            connection.query_row("SELECT MAX(manual_order) FROM entries", [], |row| {
                row.get(0)
            })?;
        Ok(max_order.unwrap_or(0) + MANUAL_ORDER_GAP)
    }

    fn next_folder_manual_order_with_connection(
        &self,
        connection: &Connection,
        parent_id: Option<&FolderId>,
    ) -> Result<i64> {
        let max_order: Option<i64> = connection.query_row(
            "SELECT MAX(manual_order) FROM folders WHERE parent_id IS ?1",
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
                last_page   INTEGER DEFAULT 0,
                rating      INTEGER DEFAULT 0,
                cover_hash  TEXT,
                missing     INTEGER DEFAULT 0 NOT NULL
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
                updated_at  INTEGER NOT NULL
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
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN metadata_locked INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN manual_order INTEGER DEFAULT 0 NOT NULL",
            [],
        );
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
        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryEntry> {
    let added_at: i64 = row.get(12)?;
    let opened_at: Option<i64> = row.get(13)?;
    let page_count: Option<i64> = row.get(14)?;
    let last_page: i64 = row.get(15)?;
    let rating: i64 = row.get(16)?;

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
        last_page: last_page as u16,
        rating: rating as u8,
        cover_hash: row.get(17)?,
        tags: Vec::new(),
        folders: Vec::new(),
        missing: row.get::<_, i64>(18)? != 0,
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

fn sort_key(value: Option<&str>) -> Option<String> {
    clean_optional_text(value).map(|value| value.to_lowercase())
}

fn clean_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(|value| {
            value
                .chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>()
        })
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(512).collect())
}

fn clean_title_sort_key(title: &str) -> Option<String> {
    let title = clean_optional_text(Some(title))?;
    let lower = title.to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return Some(rest.to_owned());
        }
    }
    Some(lower)
}

fn clean_folder_name(name: &str) -> Result<String> {
    clean_optional_text(Some(name)).context("Folder name cannot be empty.")
}

fn next_folder_suffix(connection: &Connection) -> Result<i64> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))?;
    Ok(count + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Db {
        let path = std::env::temp_dir().join(format!(
            "pdf-folio-library-{}-{}.db",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        Db::open(path).expect("test database should open")
    }

    fn entry(id: &str, title: &str) -> NewLibraryEntry {
        NewLibraryEntry {
            id: EntryId::new(id),
            path: PathBuf::from(format!("/tmp/{id}.pdf")),
            title: Some(title.to_owned()),
            author: None,
            author_attributed: false,
            page_count_attributed: false,
            page_count: Some(10),
            cover_hash: None,
        }
    }

    #[test]
    fn inserts_entries_with_gapped_manual_order_and_reorders_them() {
        let db = test_db();
        db.insert_entry(&entry("a", "Alpha")).unwrap();
        db.insert_entry(&entry("b", "Beta")).unwrap();
        db.insert_entry(&entry("c", "Gamma")).unwrap();

        let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        assert!(entries[1].manual_order - entries[0].manual_order >= MANUAL_ORDER_GAP);

        db.set_manual_entry_order(&[EntryId::new("c"), EntryId::new("a"), EntryId::new("b")])
            .unwrap();

        let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    #[test]
    fn updates_and_resets_display_metadata() {
        let db = test_db();
        let id = EntryId::new("book");
        db.insert_entry(&entry("book", "The Book")).unwrap();

        db.update_display_metadata(&id, Some("  A Better Book  "), Some(" Author Name "))
            .unwrap();
        db.apply_title_sort_cleanup(&id).unwrap();

        let entry = db
            .entry_by_path(Path::new("/tmp/book.pdf"))
            .unwrap()
            .unwrap();
        assert_eq!(entry.display_title.as_deref(), Some("A Better Book"));
        assert_eq!(entry.display_author.as_deref(), Some("Author Name"));
        assert_eq!(entry.sort_title.as_deref(), Some("better book"));
        assert!(entry.metadata_locked);

        db.reset_display_metadata(&id).unwrap();
        let entry = db
            .entry_by_path(Path::new("/tmp/book.pdf"))
            .unwrap()
            .unwrap();
        assert_eq!(entry.display_title, None);
        assert_eq!(entry.display_author, None);
        assert_eq!(entry.sort_title.as_deref(), Some("the book"));
        assert!(!entry.metadata_locked);
    }

    #[test]
    fn folders_support_membership_nesting_and_cascade() {
        let db = test_db();
        db.insert_entry(&entry("a", "Alpha")).unwrap();
        db.insert_entry(&entry("b", "Beta")).unwrap();

        let parent = db.create_folder("Work", None).unwrap();
        let child = db.create_folder("Drafts", Some(&parent)).unwrap();
        db.add_entry_to_folder(&EntryId::new("a"), &parent).unwrap();
        db.add_entry_to_folder(&EntryId::new("b"), &parent).unwrap();
        db.add_entry_to_folder(&EntryId::new("a"), &child).unwrap();

        let folders = db.get_folders().unwrap();
        assert_eq!(folders.len(), 2);
        assert_eq!(
            folders
                .iter()
                .find(|folder| folder.id == child)
                .and_then(|folder| folder.parent_id.as_ref()),
            Some(&parent)
        );

        let entries = db.entries_in_folder(&parent).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert_eq!(entries[0].folders.len(), 2);

        assert!(db.move_folder(&parent, Some(&child)).is_err());

        db.delete_entry(&EntryId::new("a")).unwrap();
        assert_eq!(db.entries_in_folder(&parent).unwrap().len(), 1);

        db.delete_folder(&parent).unwrap();
        assert!(db.get_folders().unwrap().is_empty());
        assert_eq!(db.get_all_entries().unwrap().len(), 1);
    }

    #[test]
    fn library_preferences_round_trip() {
        let db = test_db();
        let folder = db.create_folder("Reading", None).unwrap();
        let preferences = LibraryPreferences {
            sort_mode: LibrarySortMode::TitleAsc,
            layout_mode: LibraryLayoutMode::List,
            selected_folder: Some(folder.clone()),
            sidebar_width: 220.0,
            visible_metadata_fields: vec![String::from("author"), String::from("progress")],
        };

        db.save_library_preferences(&preferences).unwrap();
        let loaded = db.library_preferences().unwrap();

        assert_eq!(loaded.sort_mode, LibrarySortMode::TitleAsc);
        assert_eq!(loaded.layout_mode, LibraryLayoutMode::List);
        assert_eq!(loaded.selected_folder, Some(folder));
        assert_eq!(loaded.sidebar_width, 220.0);
        assert_eq!(
            loaded.visible_metadata_fields,
            vec![String::from("author"), String::from("progress")]
        );
    }
}
