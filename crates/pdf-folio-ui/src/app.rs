//! Top-level application state and launch entrypoint for PDF-Folio.
//!
//! This crate is the main application shell built on the [`iced`] framework.
//! It wires together the library, viewer, and style subsystems into a single
//! [`PDFolioApp`] state machine with an `update`/`view`/`subscription` loop.
//!
//! Key exports:
//!
//! - [`PDFolioApp`] — the root application state holding viewer runtime,
//!   library runtime, chrome state, appearance settings, and the database
//!   handle.
//! - [`run`] — launches the iced application with optional initial file.
//! - [`AppMode`] — switches between the library manager and the PDF viewer.
//! - [`Settings`] — user-configurable application settings.
//! - [`messages`] — the [`Message`] enum and related menu/shortcut types
//!   that drive the update loop.
//!
//! Internal modules are organized into `app/` (state, update, view, layout),
//! `library/` (thumbnails, tasks, filtering, drag-and-drop), `viewer/`
//! (canvas rendering, zoom, outline, text search), and `views/` (top-level
//! view composition).
//!
//! [`iced`]: https://docs.rs/iced

pub use pdf_folio_style as style;
pub use pdf_folio_style::theme;

#[path = "app/commands.rs"]
pub(crate) mod app_commands;
#[path = "app/context_menu.rs"]
mod app_context_menu;
#[path = "app/libraries.rs"]
pub mod app_libraries;
#[path = "app/library_clipboard.rs"]
mod app_library_clipboard;
#[path = "app/library_data.rs"]
mod app_library_data;
#[path = "app/library_drag.rs"]
mod app_library_drag;
#[path = "app/library_folders.rs"]
mod app_library_folders;
#[path = "app/library_layout.rs"]
mod app_library_layout;
#[path = "app/library_selection.rs"]
mod app_library_selection;
#[path = "app/library_view_state.rs"]
mod app_library_view_state;
#[path = "app/session.rs"]
mod app_session;
#[path = "app/sync_auth.rs"]
mod app_sync_auth;
#[path = "app/update.rs"]
mod app_update;
#[path = "app/view.rs"]
mod app_view;
#[path = "app/viewer_layout.rs"]
mod app_viewer_layout;
#[path = "app/viewer_navigation.rs"]
mod app_viewer_navigation;
#[path = "app/viewer_state.rs"]
mod app_viewer_state;
#[path = "library/mod.rs"]
mod library;
#[path = "app/messages.rs"]
pub mod messages;
#[path = "app/platform.rs"]
mod platform;
#[path = "app/shortcuts.rs"]
pub(crate) mod shortcuts;
#[path = "app/subscriptions.rs"]
pub(crate) mod subscriptions;
#[path = "viewer/mod.rs"]
mod viewer;
#[path = "views/mod.rs"]
pub mod views;

use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, container, image, mouse_area, pin, scrollable, text, text_input, tooltip, Svg,
};
use iced::widget::{operation, Id};
use iced::{animation, font, keyboard, Animation, Color, ContentFit, Element, Font, Length, Point};
use iced::{clipboard, mouse};
use iced::{Rectangle, Size};
use iced::{Task, Theme};
use pdf_folio_core::{Annotation, OutlineNode, PageTextLayer, PdfDoc, TileCache, TileKey};
#[cfg(test)]
use pdf_folio_db::NewLibraryEntry;
use pdf_folio_db::{
    Db, EntryId, Folder, FolderId, ImportedEntry, LibraryEntry, LibraryLayoutMode,
    LibraryOrganizationSnapshot, LibraryPreferences, LibrarySortMode, LibraryWatchEvent,
};
use pdf_folio_raindrop::{
    RaindropImportDestination, RaindropImportPhase, RaindropImportPreview, RaindropImportProgress,
    RaindropPdfCandidate,
};

use crate::library::drag::{
    active_folder_drop_target, can_drag_reorder_library as can_drag_reorder_library_for_state,
    drag_auto_scroll_velocity, folder_can_move_into, folder_card_target_at_cursor,
    folder_drop_flash_active_at, reorder_folder_ids_before_target, FolderDragState,
    LibraryDragState, LIBRARY_DRAG_AUTOSCROLL_MAX_DT, LIBRARY_FOLDER_DROP_FLASH_MS,
};
#[cfg(test)]
use crate::library::drag::{
    folder_drop_target_at_cursor, parent_directory_target_at_cursor,
    LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED, LIBRARY_FOLDER_DROP_DWELL_MS,
};
#[cfg(test)]
use crate::library::filters::{
    entry_search_fields_match, library_reading_state, search_match_source_label_for_fields,
};
use crate::library::filters::{
    entry_visible_in_folder_scope, library_entry_reading_state, search_match_source_label,
};
use crate::library::metadata::{
    clean_metadata_input, entry_author, entry_title, file_size_label, last_opened_label,
    library_card_metadata_label, library_row_metadata_label, page_count_label, progress_fraction,
    total_file_size_label,
};
#[cfg(test)]
use crate::library::selection::dragged_placeholder_count;
use crate::library::selection::{
    master_checkbox_state_for_counts, range_selection_ids, reorder_entry_ids_for_drag,
    toggle_selection_entry_id,
};
use crate::library::state::{LibraryMetadataDensity, LibraryReadingFilter};
use crate::library::tasks::{
    add_entries_to_folder_task, apply_watch_event, attribute_pending_metadata_task,
    bulk_delete_metadata_task, bulk_operation_task, bulk_permanently_delete_entries_task,
    bulk_refresh_metadata_task, bulk_reindex_task, bulk_reset_metadata_task,
    bulk_restore_trash_items_task, create_folder_task, delete_folder_task, delete_tag_task,
    edit_metadata_task, export_library_entries_task, import_folder_with_index,
    import_pdf_with_index, move_entries_to_folder_task, move_folder_task,
    paste_library_clipboard_task, permanently_delete_folder_from_trash_task,
    persist_manual_entry_order_task, persist_manual_folder_order_task, relink_entry_task,
    rename_folder_task, rename_tag_task, reset_metadata_task,
    restore_library_history_snapshot_task, search_library_task,
};
#[cfg(test)]
use crate::library::tasks::{clean_import_title, title_from_path};
use crate::library::thumbnails::{
    bulk_thumbnail_task, load_cached_thumbnail, load_or_render_thumbnail, ThumbnailCacheKey,
    ThumbnailSize, ThumbnailView,
};
#[cfg(test)]
use crate::library::view::{
    duplicate_status_label_for_count, folder_meta_label, folder_sidebar_count_label,
    indeterminate_progress_value,
};
use crate::library::view::{
    folder_cards_per_row, folder_cards_section_height, format_count, masonry_target_index,
    parent_directory_drop_box_height, scroll_library_to_offset_task, shortest_column_index,
};
use crate::messages::{
    ConfirmationAction, ContextMenuAction, ContextMenuTarget, LibrarySidebarTab, Message, Shortcut,
    ViewerSidebarTab,
};
use crate::platform::file_manager_commands;
#[cfg(test)]
use crate::platform::{file_manager_command, file_uri};
use crate::style::{
    button_style, container_style, display_font, empty_state, icon_button, master_checkbox,
    mix_color, progress_bar, scrollable_style, search_input_with_class, section_heading,
    selection_checkbox, side_border, side_border_for_class, sidebar_scrollable_style, tag_pill,
    text_input_style, toc_entry, toolbar_button, ui_font, viewer_primitives, Class, ComponentState,
    FontSize, FontWeight, MasterCheckboxState, Spacing, StyleBook, ThemeTokens, VisualOverride,
    UI_FONT_FAMILY,
};
#[cfg(test)]
use crate::subscriptions::style_watch_event_should_reload;
use crate::subscriptions::subscription;
use crate::theme::AppTheme;
use crate::viewer::canvas::ZoomRenderPolicy;
use crate::viewer::state::{
    RenderedPageView, ViewerFindMatch, ViewerFindState, ViewerScrollMode, ViewerSpreadMode,
    ViewerTextAnchor, ViewerTextSelection,
};
use crate::viewer::tasks::{
    open_document_task, open_library_document_task, render_page, schedule_zoom_render,
};
use crate::viewer::zoom::{
    width_from_percent_input, zoom_percent_label, ZoomPreset, MAX_ZOOM_WIDTH, MIN_ZOOM_WIDTH,
    ZOOM_INPUT_ID,
};
#[cfg(test)]
use notify::EventKind;

use app_libraries::{
    load_library_registry, LibraryNameDialog, LibraryProfile, LibraryRegistryRuntime,
};
use app_session::{load_app_session, save_app_session, AppSession};
use app_sync_auth::{SyncAuthRuntime, SyncAuthState};
use app_update::{pending_raindrop_rollback_check_task, update};
use app_view::view;
use app_viewer_layout::*;
use pdf_folio_ui_components::library::view::with_alpha;

const PAGE_INPUT_ID: &str = "viewer-toolbar-page-input";
const CHEVRON_LEFT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"##;
const CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"##;
const CHEVRON_UP_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"##;
const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;
const UNDO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 1 1 0 11H11"/></svg>"##;
const REDO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 1 0 0 11H13"/></svg>"##;
const GRID_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/></svg>"##;
const LIST_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>"##;
const TRASH_CAN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v5"/><path d="M14 11v5"/></svg>"##;
const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
const VIEWER_SCROLLABLE_ID: &str = "viewer-scrollable";
const LIBRARY_SEARCH_INPUT_ID: &str = "library-search-input";
const LIBRARY_TAG_RENAME_INPUT_ID: &str = "library-tag-rename-input";
const LIBRARY_NAME_DIALOG_INPUT_ID: &str = "library-name-dialog-input";
const LIBRARY_CREATE_FOLDER_INPUT_ID: &str = "library-create-folder-input";
const VIEWER_FIND_INPUT_ID: &str = "viewer-find-input";
const LIBRARY_FOLDER_RENAME_INPUT_ID: &str = "library-folder-rename-input";
const LIBRARY_DETAILS_TITLE_INPUT_ID: &str = "library-details-title-input";
const LIBRARY_CARD_HOVER_TICK_MS: u64 = 16;
const LIBRARY_CARD_HOVER_DURATION_MS: u64 = 180;
const VIEWER_THUMBNAIL_WIDTH_PX: u16 = 128;
pub(crate) const VIEWER_ANIMATION_TICK_MS: u64 = 16;
const VIEWER_PAGE_FADE_MS: u64 = 50;
const LIBRARY_SORT_OPTIONS: [LibrarySortMode; 10] = [
    LibrarySortMode::Manual,
    LibrarySortMode::TitleAsc,
    LibrarySortMode::TitleDesc,
    LibrarySortMode::AuthorAsc,
    LibrarySortMode::AuthorDesc,
    LibrarySortMode::RecentlyAdded,
    LibrarySortMode::RecentlyOpened,
    LibrarySortMode::ReadingProgress,
    LibrarySortMode::PageCount,
    LibrarySortMode::MissingFiles,
];
const LIBRARY_METADATA_DENSITY_OPTIONS: [LibraryMetadataDensity; 3] = [
    LibraryMetadataDensity::Minimal,
    LibraryMetadataDensity::Standard,
    LibraryMetadataDensity::Detailed,
];

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
    pending_session_restore: Option<AppSession>,
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
    fn new(source: ExportSource) -> Self {
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
enum LibraryRenderItem {
    Entry(LibraryEntry),
    Ghost(LibraryEntry),
    DropZone(LibraryEntry),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FolderSmartCounts {
    total: usize,
    in_progress: usize,
    missing: usize,
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
struct LibraryMasonryLayout {
    columns: Vec<Vec<LibraryMasonryItem>>,
    content_height: f32,
}

#[derive(Debug, Clone)]
struct LibraryMasonryItem {
    index: usize,
    top: f32,
    height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryEntryRenderMode {
    Normal,
    Placeholder,
    Floating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderCardRenderMode {
    Normal,
    Placeholder,
    NestingTarget,
    Floating,
}

/// Launches the PDF-Folio UI.
///
/// # Errors
///
/// Returns an error when startup state cannot be created.
pub fn run(initial_file: Option<PathBuf>) -> Result<()> {
    let launch_started_at = Instant::now();
    let startup_probe_enabled = std::env::var_os("PDF_FOLIO_STARTUP_PROBE").is_some();
    let startup_file = initial_file.clone();
    let startup_session = if startup_file.is_none() {
        match load_app_session() {
            Ok(session) => session,
            Err(error) => {
                tracing::warn!(%error, "Failed to load previous PDF-Folio session");
                None
            }
        }
    } else {
        None
    };
    let initial_size = startup_session
        .as_ref()
        .map(AppSession::window_size)
        .unwrap_or_else(initial_window_size);
    let app = PDFolioApp::with_initial_file_and_session(initial_file, startup_session.clone())?;
    tracing::info!(
        elapsed_ms = launch_started_at.elapsed().as_millis(),
        startup_probe_enabled,
        "PDF-Folio local startup state constructed"
    );

    tracing::info!(
        mode = ?app.mode,
        has_document = app.viewer.doc.is_some(),
        "Initialized PDF-Folio application state"
    );

    let mut application = iced::application(
        move || {
            let app = app.clone();
            let open_task = if app.sync_auth.is_signed_in() {
                startup_file
                    .clone()
                    .or_else(|| startup_session.as_ref()?.viewer.document_path.clone())
                    .map(open_document_task)
                    .unwrap_or_else(Task::none)
            } else {
                Task::none()
            };
            let rollback_task = if app.sync_auth.is_signed_in() {
                pending_raindrop_rollback_check_task()
            } else {
                Task::none()
            };
            let startup_probe_task = if startup_probe_enabled {
                Task::perform(
                    {
                        let launch_started_at = launch_started_at;
                        async move {
                            let probe_started_at = Instant::now();
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            (launch_started_at, probe_started_at, Instant::now())
                        }
                    },
                    |(launch_started_at, probe_started_at, emitted_at)| {
                        Message::StartupResponsivenessProbe {
                            launch_started_at,
                            probe_started_at,
                            emitted_at,
                        }
                    },
                )
            } else {
                Task::none()
            };
            let startup_background_ready_task = Task::perform(
                async {
                    tokio::time::sleep(Duration::from_millis(750)).await;
                },
                |_| Message::StartupBackgroundReady,
            );
            (
                app,
                Task::batch([
                    open_task,
                    rollback_task,
                    startup_probe_task,
                    startup_background_ready_task,
                ]),
            )
        },
        update,
        view,
    )
    .title(PDFolioApp::title)
    .theme(|app: &PDFolioApp| match app.appearance.theme {
        AppTheme::Light => Theme::Light,
        AppTheme::Dark => Theme::Dark,
    });

    for font in pdf_folio_style::BUNDLED_FONT_BYTES {
        application = application.font(*font);
    }

    application
        .default_font(iced::Font::with_name(UI_FONT_FAMILY))
        .antialiasing(false)
        .subscription(subscription)
        .scale_factor(|app| app.viewer.scale_factor)
        .window(iced::window::Settings {
            size: Size::new(initial_size[0], initial_size[1]),
            maximized: true,
            position: iced::window::Position::Centered,
            ..iced::window::Settings::default()
        })
        .run()?;

    Ok(())
}

fn initial_window_size() -> [f32; 2] {
    StyleBook::load()
        .unwrap_or_else(|_| StyleBook::bundled())
        .layout()
        .window_size()
}

fn save_app_session_task(app: &PDFolioApp) -> Task<Message> {
    let session = app.snapshot_session();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || save_app_session(&session)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::SessionSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn with_session_save(task: Task<Message>, app: &PDFolioApp) -> Task<Message> {
    Task::batch([task, save_app_session_task(app)])
}

fn open_file_manager_task(path: PathBuf, reveal: bool) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let commands = file_manager_commands(&path, reveal);
                if commands.is_empty() {
                    anyhow::bail!(
                        "Could not determine a containing folder for {}.",
                        path.display()
                    );
                }

                let mut errors = Vec::new();
                for (program, args) in commands {
                    match std::process::Command::new(&program).args(&args).status() {
                        Ok(status) if status.success() => return Ok::<_, anyhow::Error>(()),
                        Ok(status) => {
                            errors.push(format!("{program} exited with status {status}"));
                        }
                        Err(error) => {
                            errors.push(format!("{program}: {error}"));
                        }
                    }
                }

                anyhow::bail!(
                    "File manager command failed for {}. {}",
                    path.display(),
                    errors.join("; ")
                );
            })
            .await?
        },
        |result| match result {
            Ok(()) => Message::LibraryStatus(String::from("File manager opened.")),
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn open_file_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::FileSelected),
    )
}

fn import_folder_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::ImportFolderSelected),
    )
}

fn import_pdf_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::ImportPdfSelected),
    )
}

fn export_destination_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        |path| {
            path.map_or(
                Message::FileDialogCanceled,
                Message::ExportDestinationSelected,
            )
        },
    )
}

fn relink_file_dialog_task(entry_id: EntryId) -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        move |path| {
            path.map_or(Message::FileDialogCanceled, |path| {
                Message::RelinkFileSelected {
                    entry_id: entry_id.clone(),
                    path,
                }
            })
        },
    )
}

fn save_library_preferences_task(app: &PDFolioApp) -> Task<Message> {
    let db = Arc::clone(&app.db);
    let preferences = LibraryPreferences {
        sort_mode: app.library.library_sort_mode,
        layout_mode: if app.library.compact_view_mode {
            LibraryLayoutMode::List
        } else {
            LibraryLayoutMode::Grid
        },
        selected_folder: app.library.selected_folder.clone(),
        sidebar_width: app.library.library_tag_sidebar_width,
        grid_zoom: LibraryPreferences::default().grid_zoom,
        visible_metadata_fields: app.library.library_metadata_density.visible_fields(),
        library_tree_root_expanded: app.library.library_tree_root_expanded,
        collapsed_folder_ids: app
            .library
            .collapsed_library_tree_folders
            .iter()
            .cloned()
            .collect(),
    };

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.save_library_preferences(&preferences))
                .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::LibraryPreferencesSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn library_search_match_label(
    app: &PDFolioApp,
    entry: &LibraryEntry,
    entry_id: &EntryId,
) -> Option<String> {
    let query = app.library.search_query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }

    app.library
        .search_hit_pages
        .get(entry_id)
        .map(|page| format!("Match on page {}", u32::from(*page) + 1))
        .or_else(|| search_match_source_label(entry, &query))
}

fn truncated_title<'a>(
    title: String,
    width: f32,
    tokens: ThemeTokens,
    alpha: f32,
    font_size: u32,
) -> Element<'a, Message> {
    let visible = truncate_for_width_with_font(&title, width, 0.0, font_size);
    let is_truncated = visible != title;
    let text_color = with_alpha(tokens.text_primary, alpha);
    let label = text(visible)
        .size(font_size)
        .font(display_font(FontWeight::BOLD))
        .color(text_color)
        .wrapping(Wrapping::None)
        .width(width);

    if !is_truncated {
        return label.into();
    }

    tooltip(
        label,
        container(
            text(title)
                .size(FontSize::SM)
                .color(text_color)
                .wrapping(Wrapping::None),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn truncate_for_width(label: &str, width: f32, reserved_width: f32) -> String {
    truncate_for_width_with_font(label, width, reserved_width, FontSize::SM)
}

fn file_tree_label(label: &str, width: f32, font_size: u32) -> String {
    truncate_for_width_with_font(label, width, 0.0, font_size)
}

fn file_tree_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(UI_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

fn truncate_for_width_with_font(
    label: &str,
    width: f32,
    reserved_width: f32,
    font_size: u32,
) -> String {
    const ELLIPSIS: &str = "...";

    let available = (width - reserved_width).max(0.0);
    let approx_char_width = (font_size as f32 * 0.42).max(4.8);
    let max_chars = (available / approx_char_width).floor().max(0.0) as usize;
    let char_count = label.chars().count();

    if char_count <= max_chars {
        return label.to_owned();
    }

    if max_chars <= ELLIPSIS.len() {
        return ELLIPSIS.chars().take(max_chars).collect();
    }

    let keep = max_chars - ELLIPSIS.len();
    let mut truncated: String = label.chars().take(keep).collect();
    truncated.push_str(ELLIPSIS);
    truncated
}

fn schedule_search(query: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            query
        },
        Message::SearchDebounced,
    )
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;
