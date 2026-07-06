//! SQLite database setup and library entry queries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

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

/// External service whose items have been imported into the local library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSource {
    /// Stable source identifier, e.g. `raindrop:123`.
    pub id: String,
    /// Source provider kind.
    pub kind: String,
    /// Provider account identifier, when known.
    pub account_id: Option<String>,
    /// User-visible account/source label.
    pub display_name: Option<String>,
    /// Source creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Source update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// A Raindrop.io collection mirrored into a local PDF-Folio folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropCollectionMapping {
    /// PDF-Folio import source id.
    pub source_id: String,
    /// Raindrop collection id.
    pub collection_id: i64,
    /// Local folder id.
    pub folder_id: FolderId,
    /// Parent Raindrop collection id.
    pub parent_collection_id: Option<i64>,
    /// Most recent remote collection title seen by PDF-Folio.
    pub title: String,
}

/// A Raindrop.io item imported into a local PDF-Folio entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropEntryMapping {
    /// PDF-Folio import source id.
    pub source_id: String,
    /// Raindrop item id.
    pub raindrop_id: i64,
    /// Local entry id.
    pub entry_id: EntryId,
    /// Raindrop collection id containing the item.
    pub collection_id: Option<i64>,
    /// Remote link used to download/open the item.
    pub remote_link: Option<String>,
    /// Most recent remote title seen by PDF-Folio.
    pub remote_title: Option<String>,
    /// Most recent remote update timestamp seen by PDF-Folio.
    pub remote_updated_at: Option<String>,
    /// Remote filename, when supplied by Raindrop.
    pub file_name: Option<String>,
    /// Remote file size, when supplied by Raindrop.
    pub file_size: Option<u64>,
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
    /// Card scale for the masonry grid view.
    pub grid_zoom: f32,
    /// Metadata fields visible in cards/rows.
    pub visible_metadata_fields: Vec<String>,
    /// Whether the root library tree section is expanded.
    pub library_tree_root_expanded: bool,
    /// Folder tree nodes collapsed by the user.
    pub collapsed_folder_ids: Vec<FolderId>,
}

impl Default for LibraryPreferences {
    fn default() -> Self {
        Self {
            sort_mode: LibrarySortMode::RecentlyAdded,
            layout_mode: LibraryLayoutMode::Grid,
            selected_folder: None,
            sidebar_width: 112.0,
            grid_zoom: 1.0,
            visible_metadata_fields: vec![
                String::from("author"),
                String::from("page_count"),
                String::from("file_size"),
            ],
            library_tree_root_expanded: true,
            collapsed_folder_ids: Vec::new(),
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
    /// File size in bytes, if known.
    pub file_size: Option<u64>,
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

/// One persisted PDF-folder membership row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryFolderMembership {
    /// Library entry id.
    pub entry_id: EntryId,
    /// Folder id containing the entry.
    pub folder_id: FolderId,
    /// Stable manual order within the folder.
    pub manual_order: i64,
}

/// One persisted folder row with trash state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFolderSnapshot {
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
    /// Trash timestamp, when the folder is in the Trash Can.
    pub trashed_at: Option<DateTime<Utc>>,
}

/// One persisted entry trash-state row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryTrashState {
    /// Library entry id.
    pub entry_id: EntryId,
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
    /// Trash timestamp, when the entry is in the Trash Can.
    pub trashed_at: Option<DateTime<Utc>>,
}

/// One persisted entry tag row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryTagSnapshot {
    /// Library entry id.
    pub entry_id: EntryId,
    /// User-visible tag.
    pub tag: String,
}

/// Sync-visible metadata for an entry row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryRow {
    /// BLAKE3/content-addressed entry id.
    pub id: EntryId,
    /// Local library id used by the remote Turso schema.
    pub library_id: String,
    /// User-visible title, when known.
    pub title: Option<String>,
    /// User-visible author, when known.
    pub author: Option<String>,
    /// Last local update timestamp as a Unix timestamp.
    pub updated_at: i64,
    /// Tombstone timestamp, when deleted.
    pub deleted_at: Option<i64>,
}

/// Sync-visible metadata for a folder row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFolderRow {
    /// Stable folder id.
    pub id: FolderId,
    /// Local library id used by the remote Turso schema.
    pub library_id: String,
    /// User-visible folder name.
    pub name: String,
    /// Optional parent folder id.
    pub parent_id: Option<FolderId>,
    /// Last local update timestamp as a Unix timestamp.
    pub updated_at: i64,
    /// Tombstone timestamp, when deleted.
    pub deleted_at: Option<i64>,
}

/// Sync-visible metadata for an entry-folder membership row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncEntryFolderRow {
    /// Entry id.
    pub entry_id: EntryId,
    /// Folder id.
    pub folder_id: FolderId,
    /// Last local update timestamp as a Unix timestamp.
    pub updated_at: i64,
    /// Tombstone timestamp, when deleted.
    pub deleted_at: Option<i64>,
}

/// Counts returned after seeding local library data into sync metadata tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncSeedSummary {
    /// Entry metadata rows written.
    pub entries: usize,
    /// Folder metadata rows written.
    pub folders: usize,
    /// Entry-folder membership rows written.
    pub entry_folders: usize,
}

/// Reversible snapshot of library organization and user-editable entry state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryOrganizationSnapshot {
    /// All user folders, including trashed folders.
    pub folders: Vec<LibraryFolderSnapshot>,
    /// All PDF-folder memberships.
    pub entry_folders: Vec<EntryFolderMembership>,
    /// Reversible entry state for all PDFs.
    pub entry_trash_states: Vec<EntryTrashState>,
    /// User tags for all PDFs.
    pub entry_tags: Vec<EntryTagSnapshot>,
}

impl LibraryOrganizationSnapshot {
    /// Returns entry ids whose indexed search state differs between snapshots.
    pub fn search_changed_entry_ids(&self, other: &Self) -> Vec<EntryId> {
        let entry_search_state = |snapshot: &Self| {
            snapshot
                .entry_trash_states
                .iter()
                .map(|entry| {
                    (
                        entry.entry_id.as_str().to_owned(),
                        (
                            entry.display_title.clone(),
                            entry.display_author.clone(),
                            entry.sort_title.clone(),
                            entry.sort_author.clone(),
                            entry.metadata_locked,
                            entry.trashed_at,
                        ),
                    )
                })
                .collect::<std::collections::HashMap<_, _>>()
        };
        let left = entry_search_state(self);
        let right = entry_search_state(other);
        let mut ids = left
            .keys()
            .chain(right.keys())
            .filter(|id| left.get(*id) != right.get(*id))
            .map(|id| EntryId::new(id.clone()))
            .collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        ids.dedup_by(|left, right| left.as_str() == right.as_str());
        ids
    }

    /// Returns true when restoring this snapshot should rebuild search-derived state.
    pub fn search_state_differs_from(&self, other: &Self) -> bool {
        !self.search_changed_entry_ids(other).is_empty()
    }

    /// Returns true when entry or folder trash state differs between snapshots.
    pub fn trash_state_differs_from(&self, other: &Self) -> bool {
        let entry_trash = |snapshot: &Self| {
            snapshot
                .entry_trash_states
                .iter()
                .map(|entry| (entry.entry_id.as_str().to_owned(), entry.trashed_at.clone()))
                .collect::<std::collections::HashMap<_, _>>()
        };
        let folder_trash = |snapshot: &Self| {
            snapshot
                .folders
                .iter()
                .map(|folder| (folder.id.as_str().to_owned(), folder.trashed_at.clone()))
                .collect::<std::collections::HashMap<_, _>>()
        };

        trash_maps_differ(entry_trash(self), entry_trash(other))
            || trash_maps_differ(folder_trash(self), folder_trash(other))
    }
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
    /// File size in bytes, if known.
    pub file_size: Option<u64>,
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

        for membership in &snapshot.entry_folders {
            self.upsert_sync_entry_folder(&SyncEntryFolderRow {
                entry_id: membership.entry_id.clone(),
                folder_id: membership.folder_id.clone(),
                updated_at: now,
                deleted_at: None,
            })?;
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
        for entry in &mut entries {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders =
                self.folders_for_entry_with_connection(&connection, &entry.id, false)?;
        }
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
        for entry in &mut entries {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
            entry.folders = self.folders_for_entry_with_connection(&connection, &entry.id, true)?;
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

fn trash_maps_differ(
    left: std::collections::HashMap<String, Option<DateTime<Utc>>>,
    right: std::collections::HashMap<String, Option<DateTime<Utc>>>,
) -> bool {
    left.iter().any(|(id, left_value)| {
        let right_value = right.get(id).cloned().flatten();
        left_value != &right_value && (left_value.is_some() || right_value.is_some())
    }) || right.iter().any(|(id, right_value)| {
        let left_value = left.get(id).cloned().flatten();
        right_value != &left_value && (right_value.is_some() || left_value.is_some())
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
#[path = "tests/db.rs"]
mod tests;
