//! Shared library types: IDs, rows, preferences, snapshots, and sync DTOs.
//!
//! This module is pure data — no SQLite or I/O. Every row type mirrors a table
//! (or join) used by [`crate::Db`]. Callers in the UI and cloud crates depend
//! on these shapes for library lists, organization undo, Raindrop import, and
//! CRDT sync without opening the database themselves.
//!
//! # Groups
//!
//! - **IDs** — [`EntryId`] (usually a BLAKE3 content hash), [`FolderId`].
//! - **Library rows** — [`LibraryEntry`], [`Folder`], [`NewLibraryEntry`],
//!   tags and folder memberships.
//! - **Preferences** — [`LibraryPreferences`], [`LibraryLayoutMode`],
//!   [`LibrarySortMode`].
//! - **Organization undo** — [`LibraryOrganizationSnapshot`] and related
//!   snapshot row types for reversible bulk edits.
//! - **Integrations** — [`ImportSource`], Raindrop mappings, and sync/CRDT
//!   DTOs ([`SyncEntryRow`], [`SyncCrdtOperation`], …).
//!
//! # See also
//!
//! - [`crate::db::library`] / [`crate::db::organization`] for mutating these types.
//! - [`crate::db::sync`] for writing and querying the sync-side DTOs.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

/// Stable library entry identifier (typically a BLAKE3 content hash hex string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(String);

impl EntryId {
    /// Wraps an existing identifier string without validating format.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the underlying identifier string (used as the SQLite primary key).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable library folder identifier (generated when the folder is created).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FolderId(String);

impl FolderId {
    /// Wraps an existing folder identifier string without validating format.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrows the underlying identifier string (used as the SQLite primary key).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// User-managed PDF folder in the library tree (not a filesystem path).
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

/// External service account whose items have been imported into the local library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSource {
    /// Stable source identifier, e.g. `raindrop:123`.
    pub id: String,
    /// Provider kind string (e.g. `"raindrop"`).
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

/// How the library main pane lays out entry cards (persisted in preferences).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryLayoutMode {
    /// Masonry/grid of PDF cards (default).
    Grid,
    /// Dense list of PDF rows with metadata columns.
    List,
}

impl LibraryLayoutMode {
    /// Stable wire/storage string (`"grid"` / `"list"`) written to SQLite preferences.
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

/// Sort mode for library entry lists (persisted and applied by `Db::get_entries_sorted`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySortMode {
    /// User-managed root/folder order via `manual_order` columns.
    Manual,
    /// Title A→Z (uses sort/display/title fallbacks).
    TitleAsc,
    /// Title Z→A.
    TitleDesc,
    /// Author A→Z.
    AuthorAsc,
    /// Author Z→A.
    AuthorDesc,
    /// Most recently imported first.
    RecentlyAdded,
    /// Most recently opened first (`opened_at` nulls last).
    RecentlyOpened,
    /// Highest reading progress ratio first.
    ReadingProgress,
    /// Highest page count first.
    PageCount,
    /// Entries whose source file is missing first.
    MissingFiles,
}

impl LibrarySortMode {
    /// Stable wire/storage key written to SQLite preferences (snake_case).
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

/// Persisted library view preferences (sort, layout, sidebar, grid zoom, tree state).
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

/// A PDF entry stored in the local library, including tags and folder memberships.
///
/// `title` / `author` hold extracted or import-time values; `display_*` are
/// optional user overrides. When `metadata_locked` is true, re-import should
/// not overwrite display fields. `id` is content-derived and stable across
/// path changes after a relink.
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
    /// Manual order for each folder membership.
    pub folder_orders: Vec<EntryFolderMembership>,
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

/// One immutable sync CRDT operation stored locally and mirrored remotely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCrdtOperation {
    /// Globally stable operation id.
    pub op_id: String,
    /// Library this operation belongs to.
    pub library_id: String,
    /// Device that originally created this operation.
    pub device_id: String,
    /// Hybrid logical timestamp used for deterministic LWW conflict resolution.
    pub logical_time: i64,
    /// CRDT entity kind, for example `entry`, `folder`, or `entry_folder`.
    pub entity_kind: String,
    /// Stable entity id within the kind.
    pub entity_id: String,
    /// JSON payload for the entity state carried by this operation.
    pub payload: String,
    /// Local creation timestamp as a Unix timestamp.
    pub created_at: i64,
    /// Remote append-only sequence, once this op has been seen in Turso.
    pub remote_sequence: Option<i64>,
    /// Local timestamp when this op was pushed, if it originated locally.
    pub pushed_at: Option<i64>,
}

/// Counts produced while preparing CRDT operations from local metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncCrdtPrepareSummary {
    /// Newly generated local operations.
    pub generated: usize,
    /// Locally-originated operations still waiting to be pushed.
    pub pending_push: usize,
}

/// Reversible snapshot of library organization and user-editable entry state.
///
/// Captured before bulk folder/tag/trash edits so the UI can restore a prior
/// tree without touching PDF files on disk. Diff helpers compare search- and
/// trash-relevant fields to decide which indexes need refreshing after restore.
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
                .map(|entry| (entry.entry_id.as_str().to_owned(), entry.trashed_at))
                .collect::<std::collections::HashMap<_, _>>()
        };
        let folder_trash = |snapshot: &Self| {
            snapshot
                .folders
                .iter()
                .map(|folder| (folder.id.as_str().to_owned(), folder.trashed_at))
                .collect::<std::collections::HashMap<_, _>>()
        };

        trash_maps_differ(entry_trash(self), entry_trash(other))
            || trash_maps_differ(folder_trash(self), folder_trash(other))
    }
}

/// Input data for inserting or upserting a library entry via [`crate::Db::insert_entry`].
///
/// Prefer content-hash [`EntryId`] values from [`crate::hash_file`] so the same
/// PDF at a new path merges onto the existing row.
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
