use super::*;

/// Primary app mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Sync sign-in gate.
    SignedOut,
    /// Library manager view.
    Library,
    /// PDF viewer view.
    Viewer,
    /// Top-level library/vault selector.
    LibrarySwitcher,
}

/// User-configurable application settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Default rendered page width.
    pub default_zoom_width: u16,
    /// Number of rendered pages held in the tile cache.
    pub tile_cache_pages: usize,
    /// Directories watched for PDFs.
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

/// PDF-Folio application state.
#[derive(Debug, Clone)]
pub struct PDFolioApp {
    /// Current view mode.
    pub mode: AppMode,
    /// PDF viewer document/runtime state.
    pub viewer: ViewerRuntime,
    /// Library browsing, selection, filtering, and drag state.
    pub library: LibraryRuntime,
    /// User-created discrete libraries.
    pub libraries: LibraryRegistryRuntime,
    /// App chrome state shared by menus, dialogs, and overlays.
    pub chrome: ChromeRuntime,
    /// Runtime styling and theme state.
    pub appearance: AppearanceRuntime,
    /// User settings.
    pub settings: Settings,
    /// Sync sign-in state that gates access to the library.
    pub sync_auth: SyncAuthRuntime,
    /// Library database handle.
    pub db: Arc<Db>,
    /// Library currently being synchronized.
    pub sync_in_progress: Option<String>,
    /// Libraries that should sync after the current pass finishes.
    pub sync_queued_libraries: HashSet<String>,
    /// Last automatic sync start time.
    pub last_sync_started_at: Option<Instant>,
    /// Last successful automatic sync/check completion time.
    pub last_sync_completed_at: Option<SystemTime>,
    /// Whether background subscriptions may start after the first local frame.
    pub startup_background_ready: bool,
    /// Last-run state that is waiting for library/document prerequisites.
    pub(crate) pending_session_restore: Option<AppSession>,
}

/// Runtime state owned by the PDF viewer surface.
#[derive(Debug, Clone)]
pub struct ViewerRuntime {
    pub doc: Option<Arc<PdfDoc>>,
    pub current_entry_id: Option<EntryId>,
    pub current_document_path: Option<PathBuf>,
    pub rendered_pages: HashMap<TileKey, RenderedPageView>,
    pub page_aspect_ratios: Vec<f32>,
    pub viewport_height: f32,
    pub viewport_width: f32,
    pub viewer_viewport_height: f32,
    pub viewer_viewport_width: f32,
    pub document_error: Option<String>,
    pub pending_document_open: bool,
    pub document_open_started_at: Option<Instant>,
    pub dismissed_document_errors: HashSet<String>,
    pub cache: TileCache,
    pub page_scroll_page: u16,
    pub scroll_offset: f32,
    pub horizontal_offset: f32,
    pub viewer_scroll_mode: ViewerScrollMode,
    pub viewer_spread_mode: ViewerSpreadMode,
    pub zoom_width: u16,
    pub active_zoom_preset: Option<ZoomPreset>,
    pub zoom_editing: bool,
    pub zoom_input: String,
    pub zoom_menu_open: bool,
    pub zoom_preview_width_px: Option<u16>,
    pub zoom_generation: u64,
    pub last_scroll_offset: f32,
    pub scale_factor: f32,
    pub modifiers: keyboard::Modifiers,
    pub viewer_text_selection: Option<ViewerTextSelection>,
    pub viewer_text_layers: HashMap<u16, Arc<PageTextLayer>>,
    pub pending_text_layers: HashSet<u16>,
    pub viewer_copy_pending: bool,
    pub viewer_find: ViewerFindState,
    pub pending_renders: HashMap<TileKey, Option<u64>>,
    pub page_fade_started: HashMap<TileKey, Instant>,
    pub toc_open: bool,
    pub viewer_sidebar_tab: ViewerSidebarTab,
    pub outline: Vec<OutlineNode>,
    pub expanded_outline_paths: HashSet<Vec<usize>>,
    pub jump_dialog_open: bool,
    pub page_input_editing: bool,
    pub jump_input: String,
    pub annotations: Vec<Annotation>,
}

/// Runtime state owned by the library surface.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryClipboard {
    pub mode: LibraryClipboardMode,
    pub target: LibraryClipboardTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryClipboardMode {
    Cut,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryClipboardTarget {
    Entries(Vec<EntryId>),
    Folder(FolderId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistory {
    pub nodes: Vec<LibraryHistoryNode>,
    pub current: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryNode {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub action: Option<LibraryHistoryAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHistoryAction {
    pub label: String,
    pub before: LibraryOrganizationSnapshot,
    pub after: LibraryOrganizationSnapshot,
    pub refresh_search_on_restore: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryMovePicker {
    pub target: LibraryMoveTarget,
    pub selected_destination: Option<FolderId>,
    pub expanded_folders: HashSet<FolderId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryMoveTarget {
    SelectedEntries,
    Folder(FolderId),
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportSource {
    SelectedEntries,
    SingleEntry(EntryId),
    Folder(FolderId),
    Tag(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMode {
    CopyFlat,
    PreserveFolders,
    Zip,
}

impl ExportMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::CopyFlat => "Copy PDFs to folder",
            Self::PreserveFolders => "Preserve folder structure",
            Self::Zip => "Export as ZIP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFilenameTemplate {
    OriginalFilename,
    Title,
    AuthorTitle,
    YearAuthorTitle,
}

impl ExportFilenameTemplate {
    pub fn label(self) -> &'static str {
        match self {
            Self::OriginalFilename => "Original filename",
            Self::Title => "{title}.pdf",
            Self::AuthorTitle => "{author} - {title}.pdf",
            Self::YearAuthorTitle => "{year} - {author} - {title}.pdf",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportConflictBehavior {
    Skip,
    Overwrite,
    KeepBoth,
}

impl ExportConflictBehavior {
    pub fn label(self) -> &'static str {
        match self {
            Self::Skip => "Skip existing",
            Self::Overwrite => "Overwrite",
            Self::KeepBoth => "Keep both",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportProgress {
    pub label: String,
    pub total: usize,
    pub started_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryExportSummary {
    pub destination: PathBuf,
    pub exported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Restorable library view state captured before drilling into a tag pill.
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

/// Runtime state owned by app chrome and modal overlays.
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

/// Runtime state for a right-click contextual menu.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    pub target: ContextMenuTarget,
    pub position: Point,
}

/// Runtime state for the active visual theme and loaded style book.
#[derive(Debug, Clone)]
pub struct AppearanceRuntime {
    pub theme: AppTheme,
    pub style_book: Arc<StyleBook>,
    pub style_load_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum LibraryRenderItem {
    Entry(LibraryEntry),
    Ghost(LibraryEntry),
    DropZone(LibraryEntry),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FolderSmartCounts {
    pub(crate) total: usize,
    pub(crate) in_progress: usize,
    pub(crate) missing: usize,
}

#[derive(Debug, Clone)]
pub struct BulkOperationProgress {
    /// User-facing operation label.
    pub label: String,
    /// Number of PDFs included in the operation.
    pub total: usize,
    /// Time when the operation began, used for indeterminate animation.
    pub started_at: Instant,
}

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

#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryLayout {
    pub(crate) columns: Vec<Vec<LibraryMasonryItem>>,
    pub(crate) content_height: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryMasonryItem {
    pub(crate) index: usize,
    pub(crate) top: f32,
    pub(crate) height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibraryEntryRenderMode {
    Normal,
    Placeholder,
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderCardRenderMode {
    Normal,
    Placeholder,
    NestingTarget,
    Floating,
}
