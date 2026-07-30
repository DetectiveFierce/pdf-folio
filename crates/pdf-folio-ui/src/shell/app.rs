//! Root application types: `PDFolioApp`, modes, runtimes, and library chrome state.
//!
//! This module defines the long-lived state bags mounted on [`PDFolioApp`].
//! Domain behavior (search, render, drag, etc.) lives in `library/` and
//! `viewer/`; those modules read and mutate these structs through
//! `PDFolioApp` methods and the top-level reducers.
//!
//! # Key types
//!
//! - [`PDFolioApp`] — root iced state; holds all other runtimes plus `db`.
//! - [`AppMode`] — which full-screen surface is visible.
//! - [`LibraryRuntime`] — library browser selection, filters, dialogs, drag.
//! - [`ChromeRuntime`] — cross-mode overlays (confirmations, context menu,
//!   command palette, cursor).
//! - [`AppearanceRuntime`] — theme id and loaded KDL style book.
//! - Export / history / clipboard helpers used by library organization flows.
//!
//! # Related modules
//!
//! - [`super::messages`] — events that mutate these structs.
//! - [`super::session`] — snapshots of mode/viewer/library for relaunch.
//! - [`crate::library::registry`] — multi-library vault profiles on
//!   `PDFolioApp::libraries`.
//! - [`crate::viewer::document::ViewerRuntime`] — open PDF runtime on
//!   `PDFolioApp::viewer`.

use crate::*;

/// Primary full-screen surface shown by the application shell.
///
/// Mode switches are driven by messages (`OpenLibraryEntry`, `BackToLibrary`,
/// sync sign-in, library switcher open/close) and restored from
/// [`super::session::AppSession`] on launch when no CLI file is provided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Sync sign-in gate; blocks library access until Google auth succeeds.
    SignedOut,
    /// Library manager view (grid/list of PDFs, folders, tags).
    Library,
    /// PDF viewer view for the currently open document.
    Viewer,
    /// Top-level multi-library / vault selector screen.
    LibrarySwitcher,
}

/// User-configurable application settings shared across modes.
///
/// These are not the full preferences store (library layout lives in the DB
/// and session JSON); they cover viewer defaults and filesystem watch roots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Default rendered page width in logical pixels for newly opened docs.
    pub default_zoom_width: u16,
    /// Number of rendered pages held in the viewer tile cache.
    pub tile_cache_pages: usize,
    /// Directories watched for PDF add/remove events.
    pub watch_directories: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_zoom_width: 800,
            tile_cache_pages: 64,
            watch_directories: Vec::new(),
        }
    }
}

/// Root iced application state for PDF-Folio.
///
/// [`PDFolioApp`] is the single source of UI truth passed to `update`, `view`,
/// and `subscription`. Child runtimes partition that state by surface so
/// library and viewer code can evolve independently while still sharing the
/// database handle, appearance, and chrome overlays.
///
/// Constructed at launch by `with_initial_file_and_session` (in the viewer
/// state helpers). Prefer mutating through message handlers rather than
/// constructing a new instance at runtime.
#[derive(Debug, Clone)]
pub struct PDFolioApp {
    /// Current full-screen surface (`Library`, `Viewer`, etc.).
    pub mode: AppMode,
    /// Open PDF document, zoom, scroll, find, and outline state.
    pub viewer: ViewerRuntime,
    /// Library browsing, selection, filtering, dialogs, and drag state.
    pub library: LibraryRuntime,
    /// Multi-library vault registry (profiles, previews, switcher UI).
    pub libraries: LibraryRegistryRuntime,
    /// Cross-mode chrome: confirmations, context menu, command palette.
    pub chrome: ChromeRuntime,
    /// Active theme id and loaded KDL style book.
    pub appearance: AppearanceRuntime,
    /// Viewer defaults and filesystem watch directories.
    pub settings: Settings,
    /// Google sync sign-in state that gates access when configured.
    pub sync_auth: SyncAuthRuntime,
    /// Handle for the currently active library database.
    pub db: Arc<Db>,
    /// Library id currently mid automatic CRDT sync, if any.
    pub sync_in_progress: Option<String>,
    /// Libraries that should sync after the current pass finishes.
    pub sync_queued_libraries: HashSet<String>,
    /// Last automatic sync start time (rate-limiting / status UI).
    pub last_sync_started_at: Option<Instant>,
    /// Last successful automatic sync/check completion time.
    pub last_sync_completed_at: Option<SystemTime>,
    /// Whether heavier background subscriptions may run after first paint.
    pub startup_background_ready: bool,
    /// Last-run session waiting for library/document prerequisites to load.
    pub(crate) pending_session_restore: Option<AppSession>,
}

/// Runtime state owned by the library manager surface.
///
/// Holds entry/folder lists, selection and drag, sidebar filters, inspector
/// inputs, import/export dialogs, Raindrop UI, clipboard, and undo history.
/// Mutated primarily by `library::update` and library async tasks.
#[derive(Debug, Clone)]
pub struct LibraryRuntime {
    pub compact_view_mode: bool,
    pub library_grid_zoom: f32,
    pub library_metadata_density: LibraryMetadataDensity,
    pub library_entries: Vec<LibraryEntry>,
    pub library_trash_entries: Vec<LibraryEntry>,
    pub library_folders: Vec<Folder>,
    pub library_trash_folders: Vec<Folder>,
    pub(crate) folder_smart_count_cache: HashMap<(bool, Option<FolderId>), FolderSmartCounts>,
    pub trash_view_active: bool,
    pub library_sort_mode: LibrarySortMode,
    pub selected_folder: Option<FolderId>,
    pub details_folder_id: Option<FolderId>,
    pub new_folder_name: String,
    pub create_folder_dialog_open: bool,
    pub folder_rename_input: String,
    pub search_query: String,
    pub search_results: Option<Vec<LibraryEntry>>,
    pub search_hit_pages: HashMap<EntryId, u16>,
    pub search_generation: u64,
    pub library_scroll_offset: f32,
    pub library_viewport_height: f32,
    pub library_viewport_x: f32,
    pub library_viewport_y: f32,
    pub library_viewport_width: f32,
    pub library_tag_sidebar_width: f32,
    pub library_tag_sidebar_open: bool,
    pub resizing_library_tag_sidebar: bool,
    pub library_inspector_width: f32,
    pub library_inspector_open: bool,
    pub resizing_library_inspector: bool,
    pub library_sidebar_tab: LibrarySidebarTab,
    pub library_tree_root_expanded: bool,
    pub library_tags_expanded: bool,
    pub collapsed_library_tree_folders: HashSet<FolderId>,
    pub folder_details_sidebar_open: bool,
    pub thumbnails: HashMap<ThumbnailCacheKey, ThumbnailView>,
    pub pending_thumbnails: HashSet<ThumbnailCacheKey>,
    pub active_tag_filter: Option<String>,
    pub active_reading_filter: Option<LibraryReadingFilter>,
    pub active_recently_opened_filter: bool,
    pub missing_filter_active: bool,
    pub previous_tag_pill_view: Option<LibraryViewSnapshot>,
    pub tag_entry_id: Option<EntryId>,
    pub tag_input: String,
    pub renaming_tag: Option<String>,
    pub tag_rename_input: String,
    pub selected_library_entries: HashSet<EntryId>,
    pub library_selection_anchor: Option<EntryId>,
    pub bulk_tag_input: String,
    pub inspector_tag_input: String,
    pub inspector_tag_suggestions_open: bool,
    pub inspector_tag_highlighted_index: usize,
    pub details_entry_id: Option<EntryId>,
    pub details_title_input: String,
    pub details_author_input: String,
    pub library_status: Option<String>,
    pub library_error: Option<String>,
    pub library_startup_loading: bool,
    pub library_history_restore_started_at: Option<Instant>,
    pub raindrop_connect_dialog_open: bool,
    pub raindrop_callback_copied: bool,
    pub raindrop_client_id_input: String,
    pub raindrop_client_secret_input: String,
    pub raindrop_import_dialog_open: bool,
    pub raindrop_import_preview: Option<RaindropImportPreview>,
    pub raindrop_pdf_thumbnails: HashMap<i64, image::Handle>,
    pub selected_raindrop_pdf_ids: HashSet<i64>,
    pub raindrop_import_destination: RaindropImportDestination,
    pub raindrop_import_location_menu_open: bool,
    pub expanded_raindrop_import_location_folders: HashSet<FolderId>,
    pub raindrop_import_new_folder_active: bool,
    pub raindrop_import_new_folder_name: String,
    pub raindrop_import_progress: Option<RaindropImportProgressView>,
    pub import_menu_open: bool,
    pub import_review: Option<ImportReviewState>,
    pub tag_manager_open: bool,
    pub tag_manager_filter: String,
    pub tag_manager_merge_destination: String,
    pub export_dialog: Option<LibraryExportDialog>,
    pub export_progress: Option<LibraryExportProgress>,
    pub last_export_summary: Option<LibraryExportSummary>,
    pub raindrop_rollback_recovery_active: bool,
    pub raindrop_rollback_recovery_status: Option<String>,
    pub dismissed_library_errors: HashSet<String>,
    pub bulk_operation_progress: Option<BulkOperationProgress>,
    pub folder_drop_flash: Option<(FolderId, Instant)>,
    pub last_library_click: Option<(EntryId, Instant)>,
    pub last_folder_click: Option<(Option<FolderId>, Instant)>,
    pub last_tag_click: Option<(String, Instant)>,
    pub folder_drag_started_in_tree: bool,
    pub parent_directory_drop_scroll_adjusted: bool,
    pub library_card_hover_animations: HashMap<EntryId, Animation<bool>>,
    pub animation_now: Instant,
    pub library_drag: Option<LibraryDragState>,
    pub folder_drag: Option<FolderDragState>,
    pub move_picker: Option<LibraryMovePicker>,
    pub clipboard: Option<LibraryClipboard>,
    pub history: LibraryHistory,
}

/// In-app clipboard for cut/copy of library entries or folders.
///
/// Distinct from the OS clipboard: paste applies organization changes inside
/// the open library (and may push undo history) rather than writing files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryClipboard {
    pub mode: LibraryClipboardMode,
    pub target: LibraryClipboardTarget,
}

/// Whether the library clipboard operation is cut (move) or copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryClipboardMode {
    Cut,
    Copy,
}

/// Payload held by the library clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryClipboardTarget {
    Entries(Vec<EntryId>),
    Folder(FolderId),
}

/// Branching undo/redo stack of library organization snapshots.
///
/// Each applied move/tag/delete records a [`LibraryHistoryAction`]; undo/redo
/// restores the corresponding `before` / `after` organization snapshot via
/// database tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistory {
    pub nodes: Vec<LibraryHistoryNode>,
    pub current: usize,
}

/// One undo/redo graph node with optional action payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryNode {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub action: Option<LibraryHistoryAction>,
}

/// Labelled history action with before/after organization snapshots.
///
/// `refresh_search_on_restore` forces a search re-run when filters may have
/// depended on the mutated organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryAction {
    pub label: String,
    pub before: LibraryOrganizationSnapshot,
    pub after: LibraryOrganizationSnapshot,
    pub refresh_search_on_restore: bool,
}

/// State for the move-to-folder picker dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMovePicker {
    pub target: LibraryMoveTarget,
    pub selected_destination: Option<FolderId>,
    pub expanded_folders: HashSet<FolderId>,
}

/// What the move picker is relocating (selected PDFs vs a single folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryMoveTarget {
    SelectedEntries,
    Folder(FolderId),
}

/// Post-import review sheet: counts, errors, and suggested tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReviewState {
    pub title: String,
    pub imported_entry_ids: Vec<EntryId>,
    pub imported_count: usize,
    pub duplicate_count: usize,
    pub failed_count: usize,
    pub destination_label: String,
    pub suggested_tags: Vec<String>,
    pub errors: Vec<String>,
}

/// Export dialog configuration: source set, packaging, naming, conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportDialog {
    pub source: ExportSource,
    pub destination: Option<PathBuf>,
    pub mode: ExportMode,
    pub filename_template: ExportFilenameTemplate,
    pub include_metadata_csv: bool,
    pub include_metadata_json: bool,
    pub include_tags: bool,
    pub include_reading_progress: bool,
    pub conflict_behavior: ExportConflictBehavior,
}

impl LibraryExportDialog {
    /// Builds a dialog with default packaging options for `source`.
    pub(crate) fn new(source: ExportSource) -> Self {
        Self {
            source,
            destination: None,
            mode: ExportMode::CopyFlat,
            filename_template: ExportFilenameTemplate::OriginalFilename,
            include_metadata_csv: true,
            include_metadata_json: false,
            include_tags: true,
            include_reading_progress: true,
            conflict_behavior: ExportConflictBehavior::KeepBoth,
        }
    }
}

/// Which library entries an export includes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSource {
    SelectedEntries,
    SingleEntry(EntryId),
    Folder(FolderId),
    Tag(String),
}

/// Export packaging mode (flat copy, preserved folders, or ZIP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    CopyFlat,
    PreserveFolders,
    Zip,
}

impl ExportMode {
    /// Returns the user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::CopyFlat => "Copy PDFs to folder",
            Self::PreserveFolders => "Preserve folder structure",
            Self::Zip => "Export as ZIP",
        }
    }
}

/// Filename template used when exporting PDFs to disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFilenameTemplate {
    OriginalFilename,
    Title,
    AuthorTitle,
    YearAuthorTitle,
}

impl ExportFilenameTemplate {
    /// Returns the user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::OriginalFilename => "Original filename",
            Self::Title => "{title}.pdf",
            Self::AuthorTitle => "{author} - {title}.pdf",
            Self::YearAuthorTitle => "{year} - {author} - {title}.pdf",
        }
    }
}

/// How to handle name conflicts when exporting files to an existing path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportConflictBehavior {
    Skip,
    Overwrite,
    KeepBoth,
}

impl ExportConflictBehavior {
    /// Returns the user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Skip => "Skip existing",
            Self::Overwrite => "Overwrite",
            Self::KeepBoth => "Keep both",
        }
    }
}

/// Live progress indicator while a library export task runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportProgress {
    pub label: String,
    pub total: usize,
    pub started_at: Instant,
}

/// Summary counts shown after a library export completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportSummary {
    pub destination: PathBuf,
    pub exported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Restorable library view state captured before drilling into a tag pill.
///
/// Clicking a tag on a card filters the library; restoring this snapshot
/// returns the previous search/filters/scroll without a full reload.
#[derive(Debug, Clone)]
pub struct LibraryViewSnapshot {
    pub search_query: String,
    pub search_results: Option<Vec<LibraryEntry>>,
    pub search_hit_pages: HashMap<EntryId, u16>,
    pub active_tag_filter: Option<String>,
    pub active_reading_filter: Option<LibraryReadingFilter>,
    pub active_recently_opened_filter: bool,
    pub missing_filter_active: bool,
    pub selected_folder: Option<FolderId>,
    pub details_folder_id: Option<FolderId>,
    pub library_scroll_offset: f32,
}

/// Cross-mode chrome: confirmations, context menu, command palette, cursor.
///
/// Lives on [`PDFolioApp`] rather than library/viewer so overlays can open
/// from either surface without duplicating state.
#[derive(Debug, Clone)]
pub struct ChromeRuntime {
    pub pending_confirmation: Option<ConfirmationAction>,
    pub folder_delete_warning_suppressed: bool,
    pub folder_delete_skip_warning_checked: bool,
    pub open_context_menu: Option<ContextMenu>,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub command_palette_selected_index: usize,
    pub cursor_position: Point,
}

/// Open right-click contextual menu target and window position.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub target: ContextMenuTarget,
    pub position: Point,
}

/// Active visual theme and loaded KDL style book.
#[derive(Debug, Clone)]
pub struct AppearanceRuntime {
    pub theme: AppTheme,
    pub style_book: Arc<StyleBook>,
    pub style_load_error: Option<String>,
}

/// One item in a library list/grid render pass (real entry, drag ghost, or drop zone).
#[derive(Debug, Clone)]
pub(crate) enum LibraryRenderItem {
    Entry(LibraryEntry),
    Ghost(LibraryEntry),
    DropZone(LibraryEntry),
}

/// Cached smart counts for a folder tree node (total / in-progress / missing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FolderSmartCounts {
    pub(crate) total: usize,
    pub(crate) in_progress: usize,
    pub(crate) missing: usize,
}

/// Progress indicator for multi-entry library bulk operations.
///
/// Shown while reindex, metadata refresh, trash, or similar tasks run; uses
/// `started_at` for indeterminate animation when exact progress is unavailable.
#[derive(Debug, Clone)]
pub struct BulkOperationProgress {
    /// User-facing operation label.
    pub label: String,
    /// Number of PDFs included in the operation.
    pub total: usize,
    /// Time when the operation began, used for indeterminate animation.
    pub started_at: Instant,
}

/// UI-facing Raindrop import progress snapshot, including cancel handle.
#[derive(Debug, Clone)]
pub struct RaindropImportProgressView {
    pub completed: usize,
    pub total: usize,
    pub current_title: String,
    pub phase: RaindropImportPhase,
    pub progress_basis_points: Option<u16>,
    pub failed: bool,
    pub started_at: Instant,
    pub imported_entries: Vec<ImportedEntry>,
    pub created_folders: Vec<FolderId>,
    pub task_handle: Option<iced::task::Handle>,
}

/// Precomputed masonry column layout for the library grid.
#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryLayout {
    pub(crate) columns: Vec<Vec<LibraryMasonryItem>>,
    pub(crate) content_height: f32,
}

/// One card position inside a masonry column.
#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryItem {
    pub(crate) index: usize,
    pub(crate) top: f32,
    pub(crate) height: f32,
}

/// How a library entry card should render during drag reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryEntryRenderMode {
    Normal,
    Placeholder,
    Floating,
}

/// How a folder card should render during folder drag/nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderCardRenderMode {
    Normal,
    Placeholder,
    NestingTarget,
    Floating,
}
