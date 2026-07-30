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
    /// When true, the library uses the compact list layout instead of the masonry grid.
    pub compact_view_mode: bool,
    /// Scale factor for masonry grid cards (clamped by layout limits).
    pub library_grid_zoom: f32,
    /// How much metadata each library card/row shows.
    pub library_metadata_density: LibraryMetadataDensity,
    /// Active (non-trashed) library PDF entries for the current vault.
    pub library_entries: Vec<LibraryEntry>,
    /// PDF entries currently in the trash can.
    pub library_trash_entries: Vec<LibraryEntry>,
    /// Active folder tree for the current vault.
    pub library_folders: Vec<Folder>,
    /// Folders currently in the trash can.
    pub library_trash_folders: Vec<Folder>,
    /// Cached smart counts keyed by `(trash_view, folder_or_root)`.
    pub(crate) folder_smart_count_cache: HashMap<(bool, Option<FolderId>), FolderSmartCounts>,
    /// When true, the main pane shows trash contents instead of the live library.
    pub trash_view_active: bool,
    /// Active sort mode for the library browser.
    pub library_sort_mode: LibrarySortMode,
    /// Folder currently scoped in the main browser (`None` = all / root).
    pub selected_folder: Option<FolderId>,
    /// Folder shown in the inspector / rename panel (`None` when none focused).
    pub details_folder_id: Option<FolderId>,
    /// Draft name for the create-folder dialog.
    pub new_folder_name: String,
    /// Whether the create-folder modal is open.
    pub create_folder_dialog_open: bool,
    /// Draft rename text for the selected/details folder.
    pub folder_rename_input: String,
    /// Full-text / title search box contents.
    pub search_query: String,
    /// Latest search hit list when a query is active; `None` means unfiltered browse.
    pub search_results: Option<Vec<LibraryEntry>>,
    /// Best matching page per entry for search result jump-to-page.
    pub search_hit_pages: HashMap<EntryId, u16>,
    /// Monotonic generation used to ignore stale search task results.
    pub search_generation: u64,
    /// Vertical scroll offset of the library content pane.
    pub library_scroll_offset: f32,
    /// Visible height of the library content viewport (for virtualization).
    pub library_viewport_height: f32,
    /// Window-space x of the library content viewport.
    pub library_viewport_x: f32,
    /// Window-space y of the library content viewport.
    pub library_viewport_y: f32,
    /// Visible width of the library content viewport.
    pub library_viewport_width: f32,
    /// Logical width of the left library sidebar (folders/tags).
    pub library_tag_sidebar_width: f32,
    /// Whether the left library sidebar is expanded.
    pub library_tag_sidebar_open: bool,
    /// Whether a sidebar width drag is in progress.
    pub resizing_library_tag_sidebar: bool,
    /// Logical width of the right library inspector pane.
    pub library_inspector_width: f32,
    /// Whether the right library inspector is open.
    pub library_inspector_open: bool,
    /// Whether an inspector width drag is in progress.
    pub resizing_library_inspector: bool,
    /// Active left-sidebar tab (`Files` vs `Tags`).
    pub library_sidebar_tab: LibrarySidebarTab,
    /// Whether the library root node in the folder tree is expanded.
    pub library_tree_root_expanded: bool,
    /// Whether the tags section in the left sidebar is expanded.
    pub library_tags_expanded: bool,
    /// Folder ids whose tree children are collapsed in the sidebar.
    pub collapsed_library_tree_folders: HashSet<FolderId>,
    /// Whether the folder details strip / sidebar section is open.
    pub folder_details_sidebar_open: bool,
    /// Cached rendered thumbnails keyed by entry + size.
    pub thumbnails: HashMap<ThumbnailCacheKey, ThumbnailView>,
    /// Thumbnail cache keys currently being rendered.
    pub pending_thumbnails: HashSet<ThumbnailCacheKey>,
    /// Active sidebar/card tag filter, if any.
    pub active_tag_filter: Option<String>,
    /// Active reading-progress filter (unread / reading / finished).
    pub active_reading_filter: Option<LibraryReadingFilter>,
    /// When true, the browser is scoped to recently opened PDFs.
    pub active_recently_opened_filter: bool,
    /// When true, only entries with missing source files are shown.
    pub missing_filter_active: bool,
    /// Snapshot of filters/scroll captured before a tag-pill drill-in, for restore.
    pub previous_tag_pill_view: Option<LibraryViewSnapshot>,
    /// Entry currently receiving an inline tag input, if any.
    pub tag_entry_id: Option<EntryId>,
    /// Draft text for the active inline tag field.
    pub tag_input: String,
    /// Tag name currently being renamed in the sidebar, if any.
    pub renaming_tag: Option<String>,
    /// Draft text for the active sidebar tag rename.
    pub tag_rename_input: String,
    /// Currently multi-selected library entry ids.
    pub selected_library_entries: HashSet<EntryId>,
    /// Anchor entry for shift-range selection.
    pub library_selection_anchor: Option<EntryId>,
    /// Draft tag for bulk add/remove on the selection toolbar.
    pub bulk_tag_input: String,
    /// Draft tag typed in the right-side inspector.
    pub inspector_tag_input: String,
    /// Whether inspector tag autocomplete suggestions are shown.
    pub inspector_tag_suggestions_open: bool,
    /// Highlighted index within inspector tag suggestions.
    pub inspector_tag_highlighted_index: usize,
    /// Entry whose details editor is bound in the inspector.
    pub details_entry_id: Option<EntryId>,
    /// Draft title override in the details editor.
    pub details_title_input: String,
    /// Draft author override in the details editor.
    pub details_author_input: String,
    /// Transient success/status banner for library operations.
    pub library_status: Option<String>,
    /// Transient error banner for library operations.
    pub library_error: Option<String>,
    /// True while the initial library load / session restore is in flight.
    pub library_startup_loading: bool,
    /// When the current undo/redo restore began (progress / debounce).
    pub library_history_restore_started_at: Option<Instant>,
    /// Whether the Raindrop OAuth connect dialog is open.
    pub raindrop_connect_dialog_open: bool,
    /// Whether the Raindrop OAuth callback URL was just copied (brief UI feedback).
    pub raindrop_callback_copied: bool,
    /// Draft Raindrop OAuth client id.
    pub raindrop_client_id_input: String,
    /// Draft Raindrop OAuth client secret.
    pub raindrop_client_secret_input: String,
    /// Whether the Raindrop remote-PDF import picker is open.
    pub raindrop_import_dialog_open: bool,
    /// Remote Raindrop PDF list loaded for the import picker.
    pub raindrop_import_preview: Option<RaindropImportPreview>,
    /// Decoded remote thumbnails keyed by Raindrop item id.
    pub raindrop_pdf_thumbnails: HashMap<i64, image::Handle>,
    /// Raindrop item ids checked in the import picker.
    pub selected_raindrop_pdf_ids: HashSet<i64>,
    /// Destination folder / structure options for Raindrop import.
    pub raindrop_import_destination: RaindropImportDestination,
    /// Whether the Raindrop import root-folder dropdown is open.
    pub raindrop_import_location_menu_open: bool,
    /// Expanded folder branches in the Raindrop import location picker.
    pub expanded_raindrop_import_location_folders: HashSet<FolderId>,
    /// Whether the user is naming a new import-root folder.
    pub raindrop_import_new_folder_active: bool,
    /// Draft name for a new Raindrop import-root folder.
    pub raindrop_import_new_folder_name: String,
    /// Live progress for an in-flight Raindrop import (includes cancel handle).
    pub raindrop_import_progress: Option<RaindropImportProgressView>,
    /// Whether the unified import chooser menu is open.
    pub import_menu_open: bool,
    /// Post-import review sheet state after a bulk import finishes.
    pub import_review: Option<ImportReviewState>,
    /// Whether the tag manager modal is open.
    pub tag_manager_open: bool,
    /// Filter text for the tag manager list.
    pub tag_manager_filter: String,
    /// Destination tag name for merge operations in the tag manager.
    pub tag_manager_merge_destination: String,
    /// Open export dialog configuration, if any.
    pub export_dialog: Option<LibraryExportDialog>,
    /// Live export progress while files are written to disk.
    pub export_progress: Option<LibraryExportProgress>,
    /// Summary of the last completed export (for reveal / copy path).
    pub last_export_summary: Option<LibraryExportSummary>,
    /// True while recovering from a cancelled Raindrop import rollback.
    pub raindrop_rollback_recovery_active: bool,
    /// Status text for Raindrop rollback recovery UI.
    pub raindrop_rollback_recovery_status: Option<String>,
    /// Library error messages the user has dismissed this session.
    pub dismissed_library_errors: HashSet<String>,
    /// Progress for multi-entry bulk ops (reindex, trash, metadata, …).
    pub bulk_operation_progress: Option<BulkOperationProgress>,
    /// Brief highlight on a folder that just received a drop.
    pub folder_drop_flash: Option<(FolderId, Instant)>,
    /// Last entry click time used to detect double-open.
    pub last_library_click: Option<(EntryId, Instant)>,
    /// Last folder click time used to detect double-open / expand.
    pub last_folder_click: Option<(Option<FolderId>, Instant)>,
    /// Last tag click time used to detect double-activate.
    pub last_tag_click: Option<(String, Instant)>,
    /// True when the active folder drag began in the sidebar tree (not a card).
    pub folder_drag_started_in_tree: bool,
    /// True after auto-scroll adjusted for a parent-directory drop target.
    pub parent_directory_drop_scroll_adjusted: bool,
    /// Per-card hover tween state for lift/shadow animations.
    pub library_card_hover_animations: HashMap<EntryId, Animation<bool>>,
    /// Clock sample used to advance library UI animations.
    pub animation_now: Instant,
    /// Active PDF card drag-reorder / assign-to-folder gesture, if any.
    pub library_drag: Option<LibraryDragState>,
    /// Active folder drag-nest / reorder gesture, if any.
    pub folder_drag: Option<FolderDragState>,
    /// Open move-to-folder picker dialog state, if any.
    pub move_picker: Option<LibraryMovePicker>,
    /// In-app cut/copy clipboard for entries or folders.
    pub clipboard: Option<LibraryClipboard>,
    /// Branching undo/redo stack of library organization edits.
    pub history: LibraryHistory,
}

/// In-app clipboard for cut/copy of library entries or folders.
///
/// Distinct from the OS clipboard: paste applies organization changes inside
/// the open library (and may push undo history) rather than writing files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryClipboard {
    /// Whether paste should cut (move) or copy organization membership.
    pub mode: LibraryClipboardMode,
    /// Entries or folder held for the next paste into the active folder.
    pub target: LibraryClipboardTarget,
}

/// Whether the library clipboard operation is cut (move) or copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryClipboardMode {
    /// Paste moves the payload out of its previous folder membership.
    Cut,
    /// Paste duplicates folder membership without removing the source.
    Copy,
}

/// Payload held by the library clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryClipboardTarget {
    /// One or more PDF entries cut/copied for paste into a folder.
    Entries(Vec<EntryId>),
    /// A single folder cut/copied for paste (re-parent / nest).
    Folder(FolderId),
}

/// Branching undo/redo stack of library organization snapshots.
///
/// Each applied move/tag/delete records a [`LibraryHistoryAction`]; undo/redo
/// restores the corresponding `before` / `after` organization snapshot via
/// database tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistory {
    /// Graph nodes in insertion order; indices are referenced by `current` and edges.
    pub nodes: Vec<LibraryHistoryNode>,
    /// Index of the node representing the live library organization state.
    pub current: usize,
}

/// One undo/redo graph node with optional action payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryNode {
    /// Parent node index in the branching history graph (`None` for the root).
    pub parent: Option<usize>,
    /// Child node indices created by edits from this state.
    pub children: Vec<usize>,
    /// Action that produced this node from its parent (`None` for the empty root).
    pub action: Option<LibraryHistoryAction>,
}

/// Labelled history action with before/after organization snapshots.
///
/// `refresh_search_on_restore` forces a search re-run when filters may have
/// depended on the mutated organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryAction {
    /// Short user-facing description shown in status / history UI.
    pub label: String,
    /// Organization snapshot prior to applying the edit.
    pub before: LibraryOrganizationSnapshot,
    /// Organization snapshot after applying the edit.
    pub after: LibraryOrganizationSnapshot,
    /// When true, restoring this action re-runs the active library search.
    pub refresh_search_on_restore: bool,
}

/// State for the move-to-folder picker dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMovePicker {
    /// What is being relocated (selected PDFs or a single folder).
    pub target: LibraryMoveTarget,
    /// Chosen destination folder (`None` means library root).
    pub selected_destination: Option<FolderId>,
    /// Folder ids expanded in the picker tree.
    pub expanded_folders: HashSet<FolderId>,
}

/// What the move picker is relocating (selected PDFs vs a single folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryMoveTarget {
    /// Relocate the current multi-selected PDFs.
    SelectedEntries,
    /// Relocate one folder (and its subtree nesting).
    Folder(FolderId),
}

/// Post-import review sheet: counts, errors, and suggested tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReviewState {
    /// Heading shown on the review sheet (e.g. source of the import).
    pub title: String,
    /// Entry ids successfully added by this import.
    pub imported_entry_ids: Vec<EntryId>,
    /// Count of newly imported PDFs.
    pub imported_count: usize,
    /// Count of PDFs skipped as already present.
    pub duplicate_count: usize,
    /// Count of PDFs that failed to import.
    pub failed_count: usize,
    /// Human-readable destination folder / library label.
    pub destination_label: String,
    /// Tag suggestions offered for the imported set.
    pub suggested_tags: Vec<String>,
    /// Per-file or summary error messages from the import.
    pub errors: Vec<String>,
}

/// Export dialog configuration: source set, packaging, naming, conflicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportDialog {
    /// Which library entries the export will include.
    pub source: ExportSource,
    /// User-chosen destination directory (`None` until a path is picked).
    pub destination: Option<PathBuf>,
    /// Flat copy, preserve folders, or ZIP packaging.
    pub mode: ExportMode,
    /// How exported PDF filenames are derived.
    pub filename_template: ExportFilenameTemplate,
    /// Write a companion CSV of library metadata when true.
    pub include_metadata_csv: bool,
    /// Write a companion JSON of library metadata when true.
    pub include_metadata_json: bool,
    /// Include tag membership in companion metadata when true.
    pub include_tags: bool,
    /// Include reading progress in companion metadata when true.
    pub include_reading_progress: bool,
    /// Policy when an exported name already exists at the destination.
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
    /// Export the current multi-selected PDFs.
    SelectedEntries,
    /// Export a single PDF (e.g. from a context menu).
    SingleEntry(EntryId),
    /// Export every PDF under a folder (and nested membership).
    Folder(FolderId),
    /// Export every PDF carrying the given tag.
    Tag(String),
}

/// Export packaging mode (flat copy, preserved folders, or ZIP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    /// Copy PDFs into one destination folder without nesting.
    CopyFlat,
    /// Recreate library folder hierarchy under the destination.
    PreserveFolders,
    /// Package exported files into a ZIP archive.
    Zip,
}

impl ExportMode {
    /// Label shown in the export dialog packaging radio group.
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
    /// Keep each PDF's original on-disk basename.
    OriginalFilename,
    /// Name files from the display title only.
    Title,
    /// Name files as `{author} - {title}.pdf`.
    AuthorTitle,
    /// Name files as `{year} - {author} - {title}.pdf`.
    YearAuthorTitle,
}

impl ExportFilenameTemplate {
    /// Label / pattern preview shown in the export dialog naming menu.
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
    /// Leave the existing file and skip writing this export.
    Skip,
    /// Replace the existing file with the exported PDF.
    Overwrite,
    /// Write under a unique suffix so both files remain.
    KeepBoth,
}

impl ExportConflictBehavior {
    /// Label shown in the export dialog conflict-policy control.
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
    /// User-facing operation label shown on the progress strip.
    pub label: String,
    /// Number of PDFs included in this export.
    pub total: usize,
    /// When the export task began (indeterminate animation timing).
    pub started_at: Instant,
}

/// Summary counts shown after a library export completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportSummary {
    /// Directory or archive path that received the export.
    pub destination: PathBuf,
    /// Count of PDFs successfully written.
    pub exported: usize,
    /// Count of PDFs skipped (e.g. conflicts with skip policy).
    pub skipped: usize,
    /// Per-file or summary error messages from the export.
    pub errors: Vec<String>,
}

/// Restorable library view state captured before drilling into a tag pill.
///
/// Clicking a tag on a card filters the library; restoring this snapshot
/// returns the previous search/filters/scroll without a full reload.
#[derive(Debug, Clone)]
pub struct LibraryViewSnapshot {
    /// Search box contents at capture time.
    pub search_query: String,
    /// Search hit list at capture time (`None` if browse mode).
    pub search_results: Option<Vec<LibraryEntry>>,
    /// Per-entry best-hit pages for the captured search.
    pub search_hit_pages: HashMap<EntryId, u16>,
    /// Tag filter that was active before the pill drill-in.
    pub active_tag_filter: Option<String>,
    /// Reading-progress filter at capture time.
    pub active_reading_filter: Option<LibraryReadingFilter>,
    /// Whether the recently-opened filter was active.
    pub active_recently_opened_filter: bool,
    /// Whether the missing-files filter was active.
    pub missing_filter_active: bool,
    /// Folder scope of the main browser at capture time.
    pub selected_folder: Option<FolderId>,
    /// Folder focused in details/inspector at capture time.
    pub details_folder_id: Option<FolderId>,
    /// Vertical scroll offset of the content pane at capture time.
    pub library_scroll_offset: f32,
}

/// Cross-mode chrome: confirmations, context menu, command palette, cursor.
///
/// Lives on [`PDFolioApp`] rather than library/viewer so overlays can open
/// from either surface without duplicating state.
#[derive(Debug, Clone)]
pub struct ChromeRuntime {
    /// Destructive/overwrite action waiting for the confirmation dialog.
    pub pending_confirmation: Option<ConfirmationAction>,
    /// Session flag: skip folder-delete warnings after the user opts out.
    pub folder_delete_warning_suppressed: bool,
    /// Checkbox state on the folder-delete warning dialog ("don't ask again").
    pub folder_delete_skip_warning_checked: bool,
    /// Currently open right-click menu target and position, if any.
    pub open_context_menu: Option<ContextMenu>,
    /// Whether the command palette overlay is visible.
    pub command_palette_open: bool,
    /// Filter query typed into the command palette.
    pub command_palette_query: String,
    /// Highlighted row index in the filtered command palette list.
    pub command_palette_selected_index: usize,
    /// Last known cursor position in window coordinates (menus, drag, hover).
    pub cursor_position: Point,
}

/// Open right-click contextual menu target and window position.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Surface that was right-clicked (entry, folder, tag, canvas, …).
    pub target: ContextMenuTarget,
    /// Window-space point where the menu should appear.
    pub position: Point,
}

/// Active visual theme and loaded KDL style book.
#[derive(Debug, Clone)]
pub struct AppearanceRuntime {
    /// Selected light/dark theme id.
    pub theme: AppTheme,
    /// Parsed KDL style tokens used by widgets.
    pub style_book: Arc<StyleBook>,
    /// Last style reload error message, if any.
    pub style_load_error: Option<String>,
}

/// One item in a library list/grid render pass (real entry, drag ghost, or drop zone).
#[derive(Debug, Clone)]
pub(crate) enum LibraryRenderItem {
    /// A normal, interactive library entry card/row.
    Entry(LibraryEntry),
    /// Semi-transparent stand-in left behind while an entry is dragged.
    Ghost(LibraryEntry),
    /// Insertion indicator slot for manual reorder drop targets.
    DropZone(LibraryEntry),
}

/// Cached smart counts for a folder tree node (total / in-progress / missing).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FolderSmartCounts {
    /// Number of PDFs under this folder scope.
    pub(crate) total: usize,
    /// Number of PDFs with in-progress reading state.
    pub(crate) in_progress: usize,
    /// Number of PDFs whose source files are missing on disk.
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
    /// Number of remote PDFs fully processed so far.
    pub completed: usize,
    /// Total remote PDFs selected for this import.
    pub total: usize,
    /// Title of the PDF currently being imported.
    pub current_title: String,
    /// Coarse phase of the import pipeline (download, index, …).
    pub phase: RaindropImportPhase,
    /// Optional 0–10000 progress fraction for determinate bars.
    pub progress_basis_points: Option<u16>,
    /// True when the import ended in a hard failure (not cancel).
    pub failed: bool,
    /// When the import task began (indeterminate animation timing).
    pub started_at: Instant,
    /// Local entries created so far (used for rollback on cancel).
    pub imported_entries: Vec<ImportedEntry>,
    /// Local folders created while preserving Raindrop structure.
    pub created_folders: Vec<FolderId>,
    /// iced abort handle for cancelling the running import task.
    pub task_handle: Option<iced::task::Handle>,
}

/// Precomputed masonry column layout for the library grid.
#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryLayout {
    /// Per-column list of card placements from left to right.
    pub(crate) columns: Vec<Vec<LibraryMasonryItem>>,
    /// Total scrollable height of the masonry content area.
    pub(crate) content_height: f32,
}

/// One card position inside a masonry column.
#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryItem {
    /// Index into the visible library entry list.
    pub(crate) index: usize,
    /// Y offset of the card top within the column content.
    pub(crate) top: f32,
    /// Measured card height used for packing and hit-testing.
    pub(crate) height: f32,
}

/// How a library entry card should render during drag reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryEntryRenderMode {
    /// Default interactive card appearance.
    Normal,
    /// Empty slot left at the original index while the card is dragged.
    Placeholder,
    /// Card drawn under the cursor following the pointer.
    Floating,
}

/// How a folder card should render during folder drag/nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderCardRenderMode {
    /// Default interactive folder appearance.
    Normal,
    /// Empty slot left while the folder is dragged.
    Placeholder,
    /// Highlighted as a valid nest-into target under the pointer.
    NestingTarget,
    /// Folder drawn under the cursor following the pointer.
    Floating,
}
