//! Top-level application state and launch entrypoint.

#[path = "library/mod.rs"]
mod library;
#[path = "app/menu.rs"]
mod menu;
#[path = "app/messages.rs"]
pub mod messages;
#[path = "app/platform.rs"]
mod platform;
#[path = "app/shortcuts.rs"]
pub(crate) mod shortcuts;
#[path = "style/mod.rs"]
pub mod style;
#[path = "app/subscriptions.rs"]
pub(crate) mod subscriptions;
#[path = "style/theme.rs"]
pub mod theme;
#[path = "viewer/mod.rs"]
mod viewer;
#[path = "views/mod.rs"]
pub mod views;

use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::time::SystemTime;
use std::time::{Duration, Instant};

use anyhow::Result;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, container, image, mouse_area, pick_list, pin, scrollable, slider, text,
    text_input, tooltip, Svg,
};
use iced::widget::{operation, Id};
use iced::{animation, font, keyboard, Animation, Color, ContentFit, Element, Font, Length, Point};
use iced::{clipboard, mouse};
use iced::{Rectangle, Size};
use iced::{Task, Theme};
use pdf_folio_core::{Annotation, OutlineNode, PageTextLayer, PdfDoc, TileCache, TileKey};
#[cfg(test)]
use pdf_folio_library::NewLibraryEntry;
use pdf_folio_library::{
    Db, EntryId, Folder, FolderId, LibraryEntry, LibraryLayoutMode, LibraryPreferences,
    LibrarySortMode, LibraryWatchEvent,
};

use crate::library::drag::{
    active_folder_drop_target, can_drag_reorder_library as can_drag_reorder_library_for_state,
    drag_auto_scroll_velocity, folder_can_move_into, folder_card_target_at_cursor,
    folder_drop_flash_active_at, reorder_folder_ids_before_target, FolderDragState,
    LibraryDragState, LIBRARY_DRAG_AUTOSCROLL_MAX_DT, LIBRARY_FOLDER_DROP_FLASH_MS,
};
#[cfg(test)]
use crate::library::drag::{
    folder_drop_target_at_cursor, LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED, LIBRARY_FOLDER_DROP_DWELL_MS,
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
    apply_watch_event, attribute_pending_metadata_task, bulk_delete_metadata_task,
    bulk_operation_task, bulk_refresh_metadata_task, bulk_reindex_task, bulk_reset_metadata_task,
    delete_folder_task, edit_metadata_task, import_folder_with_index, move_entries_to_folder_task,
    move_folder_task, persist_manual_entry_order_task, persist_manual_folder_order_task,
    relink_entry_task, rename_folder_task, reset_metadata_task, search_library_task,
};
#[cfg(test)]
use crate::library::tasks::{clean_import_title, title_from_path};
use crate::library::thumbnails::{
    bulk_thumbnail_task, load_or_render_thumbnail, ThumbnailCacheKey, ThumbnailSize, ThumbnailView,
};
#[cfg(test)]
use crate::library::view::{
    duplicate_status_label_for_count, folder_meta_label, folder_sidebar_count_label,
    indeterminate_progress_value,
};
use crate::library::view::{
    folder_cards_per_row, folder_cards_section_height, format_count, masonry_target_index,
    scroll_library_to_offset_task, shortest_column_index, view, with_alpha,
};
use crate::menu::{app_menu_action_message, app_menu_bar_height};
use crate::messages::{
    AppMenu, AppMenuAction, ConfirmationAction, LibrarySidebarTab, Message, SelectionMenu,
    SelectionToolbarAction, Shortcut, ViewMenuFlyout, ViewerSidebarTab,
};
use crate::platform::file_manager_commands;
#[cfg(test)]
use crate::platform::{file_manager_command, file_uri};
use crate::style::{
    container_style, display_font, empty_state, icon_button, master_checkbox, menu_style_for_class,
    mix_color, pick_list_style, progress_bar, scrollable_style, search_input_with_class,
    section_heading, selection_checkbox, side_border, side_border_for_class,
    sidebar_scrollable_style, slider_style, tag_pill, text_input_style, toc_entry, toolbar_button,
    ui_font, viewer_primitives, Class, ComponentState, FontSize, FontWeight, LabelSection,
    MasterCheckboxState, Spacing, StyleBook, ThemeTokens, VisualOverride, UI_FONT_FAMILY,
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

const PAGE_INPUT_ID: &str = "viewer-toolbar-page-input";
const CHEVRON_LEFT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"##;
const CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"##;
const CHEVRON_UP_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"##;
const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;
const GRID_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/></svg>"##;
const LIST_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>"##;
const IBM_PLEX_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
const IBM_PLEX_SANS_MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
const IBM_PLEX_SANS_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
const IBM_PLEX_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");
const FILE_TREE_LABEL_SIZE: u32 = FontSize::MD;
const FILE_TREE_ROW_HEIGHT: f32 = 26.0;
const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
const LIBRARY_SEARCH_INPUT_ID: &str = "library-search-input";
const VIEWER_FIND_INPUT_ID: &str = "viewer-find-input";
const LIBRARY_FOLDER_RENAME_INPUT_ID: &str = "library-folder-rename-input";
const LIBRARY_DETAILS_TITLE_INPUT_ID: &str = "library-details-title-input";
const LIBRARY_CARD_HOVER_TICK_MS: u64 = 16;
const LIBRARY_CARD_HOVER_DURATION_MS: u64 = 180;
const LIBRARY_CARD_HOVER_LIFT: f32 = 2.0;
const LIBRARY_ROW_HOVER_LIFT: f32 = 1.0;
const LIBRARY_GRID_ZOOM_MIN: f32 = 0.25;
const LIBRARY_GRID_ZOOM_MAX: f32 = 12.0;
const VIEWER_THUMBNAIL_WIDTH_PX: u16 = 128;
pub(crate) const VIEWER_ANIMATION_TICK_MS: u64 = 16;
const VIEWER_PAGE_FADE_MS: u64 = 50;
const LIBRARY_GRID_ZOOM_STEP: f32 = 0.05;
const LIBRARY_GRID_ZOOM_DENSE_COLUMN_CAP: usize = 28;
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
const BULK_TAG_ACTIONS: [SelectionToolbarAction; 2] = [
    SelectionToolbarAction::AddTag,
    SelectionToolbarAction::RemoveTag,
];
const BULK_FOLDER_ACTIONS: [SelectionToolbarAction; 2] = [
    SelectionToolbarAction::AddToFolder,
    SelectionToolbarAction::RemoveFromFolder,
];
const BULK_METADATA_ACTIONS: [SelectionToolbarAction; 4] = [
    SelectionToolbarAction::SortTitles,
    SelectionToolbarAction::RefreshMetadata,
    SelectionToolbarAction::ResetMetadata,
    SelectionToolbarAction::Reindex,
];
const BULK_MAINTENANCE_ACTIONS: [SelectionToolbarAction; 2] = [
    SelectionToolbarAction::RebuildThumbnails,
    SelectionToolbarAction::DeleteMetadata,
];
const SINGLE_MORE_ACTIONS: [SelectionToolbarAction; 2] = [
    SelectionToolbarAction::ResetDetails,
    SelectionToolbarAction::RefreshMetadata,
];

/// Primary app mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Library manager view.
    Library,
    /// PDF viewer view.
    Viewer,
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
    /// Open document.
    pub doc: Option<Arc<PdfDoc>>,
    /// Current library entry opened in the viewer.
    pub current_entry_id: Option<EntryId>,
    /// Rendered page images keyed by page and zoom width.
    pub rendered_pages: HashMap<TileKey, RenderedPageView>,
    /// Pre-computed page aspect ratios, indexed by zero-based page.
    pub page_aspect_ratios: Vec<f32>,
    /// Last known viewer viewport height.
    pub viewport_height: f32,
    /// Last known viewer viewport width.
    pub viewport_width: f32,
    /// Last known PDF canvas viewport height.
    pub viewer_viewport_height: f32,
    /// Last known PDF canvas viewport width.
    pub viewer_viewport_width: f32,
    /// Last document error shown in the viewer.
    pub document_error: Option<String>,
    /// Whether a PDF open operation has started and has not resolved yet.
    pub pending_document_open: bool,
    /// Document errors dismissed for the current app session.
    pub dismissed_document_errors: HashSet<String>,
    /// Rendered tile cache.
    pub cache: TileCache,
    /// Current vertical scroll offset.
    pub scroll_offset: f32,
    /// Current horizontal pan offset for wide/zoomed pages.
    pub horizontal_offset: f32,
    /// Page arrangement and wheel behavior in the open-PDF viewer.
    pub viewer_scroll_mode: ViewerScrollMode,
    /// Two-page spread pairing behavior in the open-PDF viewer.
    pub viewer_spread_mode: ViewerSpreadMode,
    /// Current rendered page width.
    pub zoom_width: u16,
    /// Current semantic zoom preset when zoom should follow viewport dimensions.
    pub active_zoom_preset: Option<ZoomPreset>,
    /// Whether the toolbar zoom percentage is currently editable.
    pub zoom_editing: bool,
    /// Current toolbar zoom percentage input.
    pub zoom_input: String,
    /// Whether the toolbar zoom preset menu is open.
    pub zoom_menu_open: bool,
    /// Render width used as the stable preview source during an active zoom gesture.
    pub zoom_preview_width_px: Option<u16>,
    /// Monotonic token used to debounce wheel zoom rendering.
    pub zoom_generation: u64,
    /// Previous vertical scroll offset used to bias render prefetching.
    pub last_scroll_offset: f32,
    /// UI scale factor used to render pages at physical-pixel resolution.
    pub scale_factor: f32,
    /// Last known keyboard modifiers.
    pub modifiers: keyboard::Modifiers,
    /// Current page-level text selection in the viewer.
    pub viewer_text_selection: Option<ViewerTextSelection>,
    /// Extracted text layers keyed by zero-based page index.
    pub viewer_text_layers: HashMap<u16, Arc<PageTextLayer>>,
    /// Text-layer extraction jobs currently in flight.
    pub pending_text_layers: HashSet<u16>,
    /// Whether copy was requested while selected page text was still loading.
    pub viewer_copy_pending: bool,
    /// Find-in-text UI and match state for the open PDF.
    pub viewer_find: ViewerFindState,
    /// Tile render jobs currently in flight.
    pub pending_renders: HashMap<TileKey, Option<u64>>,
    /// Newly sharpened page renders that should fade in over the preview image.
    pub page_fade_started: HashMap<TileKey, Instant>,
    /// Whether the table-of-contents panel is open.
    pub toc_open: bool,
    /// Active navigation tab in the viewer sidebar.
    pub viewer_sidebar_tab: ViewerSidebarTab,
    /// Loaded table-of-contents outline for the open document.
    pub outline: Vec<OutlineNode>,
    /// Expanded table-of-contents node paths.
    pub expanded_outline_paths: HashSet<Vec<usize>>,
    /// Whether the placeholder view-mode toggle is in list mode.
    pub compact_view_mode: bool,
    /// Card scale applied to the masonry library grid.
    pub library_grid_zoom: f32,
    /// Metadata density applied to library cards and rows.
    pub library_metadata_density: LibraryMetadataDensity,
    /// Whether the jump-to-page overlay is open.
    pub jump_dialog_open: bool,
    /// Whether the toolbar page number is currently editable.
    pub page_input_editing: bool,
    /// Current jump-to-page input text.
    pub jump_input: String,
    /// In-memory annotations for the open document.
    pub annotations: Vec<Annotation>,
    /// Loaded library entries.
    pub library_entries: Vec<LibraryEntry>,
    /// Loaded user-managed library folders.
    pub library_folders: Vec<Folder>,
    /// Active library sort mode.
    pub library_sort_mode: LibrarySortMode,
    /// Selected library folder filter.
    pub selected_folder: Option<FolderId>,
    /// Inline new-folder input text.
    pub new_folder_name: String,
    /// Whether the new-folder dialog is open.
    pub create_folder_dialog_open: bool,
    /// Inline selected-folder rename input text.
    pub folder_rename_input: String,
    /// Current library search query.
    pub search_query: String,
    /// Search results, if search mode is active.
    pub search_results: Option<Vec<LibraryEntry>>,
    /// Matching page for full-text search results.
    pub search_hit_pages: HashMap<EntryId, u16>,
    /// Monotonic token used to debounce library search.
    pub search_generation: u64,
    /// Current library scroll offset in logical pixels.
    pub library_scroll_offset: f32,
    /// Last known library viewport height.
    pub library_viewport_height: f32,
    /// Last known library viewport left in window coordinates.
    pub library_viewport_x: f32,
    /// Last known library viewport top in window coordinates.
    pub library_viewport_y: f32,
    /// Last known library viewport width.
    pub library_viewport_width: f32,
    /// Current width of the library tag sidebar.
    pub library_tag_sidebar_width: f32,
    /// Whether the library tag sidebar is open.
    pub library_tag_sidebar_open: bool,
    /// Whether the library tag sidebar is being resized.
    pub resizing_library_tag_sidebar: bool,
    /// Active navigation tab in the library sidebar.
    pub library_sidebar_tab: LibrarySidebarTab,
    /// Whether the library root node is expanded in the sidebar file tree.
    pub library_tree_root_expanded: bool,
    /// Folder nodes collapsed in the sidebar file tree.
    pub collapsed_library_tree_folders: HashSet<FolderId>,
    /// Lazily loaded cover thumbnails keyed by entry id and resolution tier.
    pub thumbnails: HashMap<ThumbnailCacheKey, ThumbnailView>,
    /// Thumbnail loads/renders currently in flight.
    pub pending_thumbnails: HashSet<ThumbnailCacheKey>,
    /// Active tag filter.
    pub active_tag_filter: Option<String>,
    /// Active reading-progress filter.
    pub active_reading_filter: Option<LibraryReadingFilter>,
    /// Whether the library is filtered to missing source files.
    pub missing_filter_active: bool,
    /// Entry currently showing inline tag input.
    pub tag_entry_id: Option<EntryId>,
    /// Current inline tag text.
    pub tag_input: String,
    /// Selected library entries for bulk operations.
    pub selected_library_entries: HashSet<EntryId>,
    /// Anchor entry used for shift-click range selection.
    pub library_selection_anchor: Option<EntryId>,
    /// Current bulk tag input.
    pub bulk_tag_input: String,
    /// Entry currently loaded into the details metadata editor.
    pub details_entry_id: Option<EntryId>,
    /// Details-panel display title input.
    pub details_title_input: String,
    /// Details-panel display author input.
    pub details_author_input: String,
    /// Pending action waiting for explicit user confirmation.
    pub pending_confirmation: Option<ConfirmationAction>,
    /// Latest library/import status.
    pub library_status: Option<String>,
    /// Latest library error shown in the shared error banner.
    pub library_error: Option<String>,
    /// Library errors dismissed for the current app session.
    pub dismissed_library_errors: HashSet<String>,
    /// Active long-running bulk operation shown with shared progress styling.
    pub bulk_operation_progress: Option<BulkOperationProgress>,
    /// Recently completed folder drop target and timestamp for success flash.
    pub folder_drop_flash: Option<(FolderId, Instant)>,
    /// Last library entry click used to detect double-click opens.
    pub last_library_click: Option<(EntryId, Instant)>,
    /// Hover tween state for library cards keyed by entry id.
    pub library_card_hover_animations: HashMap<EntryId, Animation<bool>>,
    /// Current time used to sample active library card tweens.
    pub animation_now: Instant,
    /// Active library entry drag state.
    pub library_drag: Option<LibraryDragState>,
    /// Active folder drag state.
    pub folder_drag: Option<FolderDragState>,
    /// Current visual theme.
    pub theme: AppTheme,
    /// Runtime style book loaded from bundled KDL and user overrides.
    pub style_book: Arc<StyleBook>,
    /// Last style loading error, if a reload failed.
    pub style_load_error: Option<String>,
    /// Open top-level application menu.
    pub open_app_menu: Option<AppMenu>,
    /// Open right-side flyout in the View menu.
    pub open_view_menu_flyout: Option<ViewMenuFlyout>,
    /// Open selected-PDF contextual menu.
    pub open_selection_menu: Option<SelectionMenu>,
    /// User settings.
    pub settings: Settings,
    /// Library database handle.
    pub db: Arc<Db>,
}

#[derive(Debug, Clone)]
enum LibraryRenderItem {
    Entry(LibraryEntry),
    Ghost(LibraryEntry),
    DropZone(LibraryEntry),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FolderSmartCounts {
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

impl PDFolioApp {
    fn layout(&self) -> &crate::style::AppLayoutTokens {
        self.style_book.layout()
    }

    fn labels(&self) -> &crate::style::AppLabelTokens {
        self.style_book.labels()
    }

    fn estimated_viewer_viewport_width(&self) -> f32 {
        let sidebar_width = if self.toc_open {
            self.layout().viewer_sidebar_width
        } else {
            0.0
        };
        (self.viewport_width - sidebar_width).max(1.0)
    }

    fn estimated_viewer_viewport_height(&self) -> f32 {
        (self.viewport_height - app_menu_bar_height(self) - self.layout().toolbar_height).max(1.0)
    }

    fn apply_active_dimension_zoom(&mut self) -> Task<Message> {
        let Some(preset) = self.active_zoom_preset else {
            return Task::none();
        };
        if !preset.is_dimension_dependent() {
            return Task::none();
        }

        let width = preset.width_for(self);
        self.zoom_input = zoom_percent_label(width);
        let task = self.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        if matches!(preset, ZoomPreset::PageWidth) {
            self.horizontal_offset = 0.0;
        }
        self.clamp_horizontal_offset();
        self.clamp_scroll_offset();
        task
    }

    /// Creates application state using the default database location.
    ///
    /// # Errors
    ///
    /// Returns an error when the library database cannot be opened.
    pub fn new() -> Result<Self> {
        let settings = Settings::default();
        let db = Arc::new(Db::open_default()?);
        let preferences = db.library_preferences().unwrap_or_default();
        let (style_book, style_load_error) = match StyleBook::load() {
            Ok(style_book) => (style_book, None),
            Err(error) => {
                tracing::warn!(%error, "Failed to load external styles; using bundled defaults");
                (StyleBook::bundled(), Some(error))
            }
        };
        let layout = style_book.layout();
        Ok(Self {
            mode: AppMode::Library,
            doc: None,
            current_entry_id: None,
            rendered_pages: std::collections::HashMap::new(),
            page_aspect_ratios: Vec::new(),
            viewport_height: 900.0,
            viewport_width: 960.0,
            viewer_viewport_height: 900.0,
            viewer_viewport_width: 732.0,
            document_error: None,
            pending_document_open: false,
            dismissed_document_errors: HashSet::new(),
            cache: TileCache::with_default_capacity(),
            scroll_offset: 0.0,
            horizontal_offset: 0.0,
            viewer_scroll_mode: ViewerScrollMode::Vertical,
            viewer_spread_mode: ViewerSpreadMode::None,
            zoom_width: settings.default_zoom_width,
            active_zoom_preset: None,
            zoom_editing: false,
            zoom_input: zoom_percent_label(settings.default_zoom_width),
            zoom_menu_open: false,
            zoom_preview_width_px: None,
            zoom_generation: 0,
            last_scroll_offset: 0.0,
            scale_factor: 1.0,
            modifiers: keyboard::Modifiers::default(),
            viewer_text_selection: None,
            viewer_text_layers: HashMap::new(),
            pending_text_layers: HashSet::new(),
            viewer_copy_pending: false,
            viewer_find: ViewerFindState::default(),
            pending_renders: HashMap::new(),
            page_fade_started: HashMap::new(),
            toc_open: true,
            viewer_sidebar_tab: ViewerSidebarTab::Contents,
            outline: Vec::new(),
            expanded_outline_paths: HashSet::new(),
            compact_view_mode: matches!(preferences.layout_mode, LibraryLayoutMode::List),
            library_grid_zoom: preferences
                .grid_zoom
                .clamp(LIBRARY_GRID_ZOOM_MIN, LIBRARY_GRID_ZOOM_MAX),
            library_metadata_density: LibraryMetadataDensity::from_visible_fields(
                &preferences.visible_metadata_fields,
            ),
            jump_dialog_open: false,
            page_input_editing: false,
            jump_input: String::new(),
            annotations: Vec::new(),
            library_entries: Vec::new(),
            library_folders: Vec::new(),
            library_sort_mode: preferences.sort_mode,
            selected_folder: preferences.selected_folder,
            new_folder_name: String::new(),
            create_folder_dialog_open: false,
            folder_rename_input: String::new(),
            search_query: String::new(),
            search_results: None,
            search_hit_pages: HashMap::new(),
            search_generation: 0,
            library_scroll_offset: 0.0,
            library_viewport_height: 720.0,
            library_viewport_x: 0.0,
            library_viewport_y: 0.0,
            library_viewport_width: 960.0,
            library_tag_sidebar_width: preferences.sidebar_width.clamp(
                layout.library_sidebar_min_width,
                layout.library_sidebar_max_width,
            ),
            library_tag_sidebar_open: true,
            resizing_library_tag_sidebar: false,
            library_sidebar_tab: LibrarySidebarTab::Files,
            library_tree_root_expanded: preferences.library_tree_root_expanded,
            collapsed_library_tree_folders: preferences
                .collapsed_folder_ids
                .into_iter()
                .collect::<HashSet<_>>(),
            thumbnails: HashMap::new(),
            pending_thumbnails: HashSet::new(),
            active_tag_filter: None,
            active_reading_filter: None,
            missing_filter_active: false,
            tag_entry_id: None,
            tag_input: String::new(),
            selected_library_entries: HashSet::new(),
            library_selection_anchor: None,
            bulk_tag_input: String::new(),
            details_entry_id: None,
            details_title_input: String::new(),
            details_author_input: String::new(),
            pending_confirmation: None,
            library_status: None,
            library_error: None,
            dismissed_library_errors: HashSet::new(),
            bulk_operation_progress: None,
            folder_drop_flash: None,
            last_library_click: None,
            library_card_hover_animations: HashMap::new(),
            animation_now: Instant::now(),
            library_drag: None,
            folder_drag: None,
            theme: AppTheme::Dark,
            style_book,
            style_load_error,
            open_app_menu: None,
            open_view_menu_flyout: None,
            open_selection_menu: None,
            settings,
            db,
        })
    }

    /// Creates application state and records the startup PDF path when available.
    pub fn with_initial_file(initial_file: Option<PathBuf>) -> Result<Self> {
        let mut app = Self::new()?;
        let Some(path) = initial_file else {
            return Ok(app);
        };

        app.mode = AppMode::Viewer;
        app.document_error = Some(format!("Opening {}...", path.display()));
        app.pending_document_open = true;

        Ok(app)
    }

    fn open_document(&mut self, doc: Arc<PdfDoc>) -> Task<Message> {
        self.mode = AppMode::Viewer;
        self.clear_library_transient_interactions();
        self.doc = Some(Arc::clone(&doc));
        self.cache.clear();
        self.rendered_pages.clear();
        self.page_aspect_ratios = (0..doc.page_count())
            .map(|page| doc.page_aspect_ratio(page).unwrap_or(11.0 / 8.5))
            .collect();
        self.outline = doc.outline().unwrap_or_default();
        self.viewer_sidebar_tab = ViewerSidebarTab::Contents;
        self.expanded_outline_paths.clear();
        self.pending_renders.clear();
        self.page_fade_started.clear();
        self.scroll_offset = 0.0;
        self.last_scroll_offset = 0.0;
        self.horizontal_offset = 0.0;
        self.viewer_viewport_width = self.estimated_viewer_viewport_width();
        self.viewer_viewport_height = self.estimated_viewer_viewport_height();
        self.active_zoom_preset = Some(ZoomPreset::Automatic);
        self.zoom_width = ZoomPreset::Automatic.width_for(self);
        self.zoom_editing = false;
        self.zoom_input = zoom_percent_label(self.zoom_width);
        self.zoom_menu_open = false;
        self.zoom_preview_width_px = None;
        self.zoom_generation = self.zoom_generation.wrapping_add(1);
        self.viewer_text_selection = None;
        self.viewer_text_layers.clear();
        self.pending_text_layers.clear();
        self.viewer_copy_pending = false;
        self.viewer_find = ViewerFindState::default();
        self.pending_document_open = false;
        self.document_error = None;
        self.jump_dialog_open = false;
        self.page_input_editing = false;
        self.jump_input.clear();

        self.request_visible_pages()
    }

    fn return_to_library(&mut self) -> Task<Message> {
        self.mode = AppMode::Library;
        self.document_error = None;
        self.jump_dialog_open = false;
        self.page_input_editing = false;
        self.jump_input.clear();
        Task::batch([
            self.refresh_library(),
            self.refresh_folders(),
            self.request_visible_thumbnails(),
        ])
    }

    fn return_to_viewer(&mut self) -> Task<Message> {
        if self.doc.is_none() {
            return Task::none();
        }

        self.mode = AppMode::Viewer;
        self.clear_library_transient_interactions();
        self.request_visible_pages()
    }

    fn open_library_document(&mut self, entry_id: EntryId, doc: Arc<PdfDoc>) -> Task<Message> {
        self.current_entry_id = Some(entry_id.clone());
        let last_page = self
            .library_entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .map_or(0, |entry| entry.last_page);
        let task = self.open_document(doc);
        self.last_scroll_offset = self.scroll_offset;
        self.scroll_offset = self.page_top(last_page);
        self.clamp_scroll_offset();
        Task::batch([task, self.request_visible_pages()])
    }

    fn request_visible_pages(&mut self) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        let generation = self.zoom_generation;
        for page in self.prefetch_page_order() {
            let key = TileKey {
                page,
                width_px: self.render_width_px(),
            };

            if self.rendered_pages.contains_key(&key)
                || self.pending_renders.get(&key) == Some(&Some(generation))
            {
                continue;
            }

            if let Some(data) = self.cache.get(&key) {
                let width = key.width_px;
                let height = self.render_height_px(page);
                let expected_len = usize::from(width) * usize::from(height) * 4;

                if data.len() == expected_len {
                    let handle = image::Handle::from_rgba(
                        u32::from(width),
                        u32::from(height),
                        data.as_ref().clone(),
                    );
                    self.rendered_pages.insert(
                        key,
                        RenderedPageView {
                            width,
                            height,
                            handle,
                        },
                    );
                    continue;
                }
            }

            self.pending_renders.insert(key, Some(generation));
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                render_page(doc, key),
                move |result| match result {
                    Ok((key, page)) => Message::PageRendered {
                        key,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                        generation: Some(generation),
                    },
                    Err(error) => Message::DocumentError(error.to_string()),
                },
            ));
        }

        Task::batch([Task::batch(tasks), self.request_visible_text_layers()])
    }

    fn request_viewer_thumbnail_pages(&mut self) -> Task<Message> {
        if self.viewer_sidebar_tab != ViewerSidebarTab::Thumbnails {
            return Task::none();
        }

        let Some(doc) = &self.doc else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        for page in 0..doc.page_count() {
            let key = TileKey {
                page,
                width_px: VIEWER_THUMBNAIL_WIDTH_PX,
            };

            if self.rendered_pages.contains_key(&key) || self.pending_renders.contains_key(&key) {
                continue;
            }

            if let Some(data) = self.cache.get(&key) {
                let height = (f32::from(key.width_px) * self.page_aspect_ratios[usize::from(page)])
                    .round()
                    .clamp(1.0, f32::from(u16::MAX)) as u16;
                let expected_len = usize::from(key.width_px) * usize::from(height) * 4;

                if data.len() == expected_len {
                    let handle = image::Handle::from_rgba(
                        u32::from(key.width_px),
                        u32::from(height),
                        data.as_ref().clone(),
                    );
                    self.rendered_pages.insert(
                        key,
                        RenderedPageView {
                            width: key.width_px,
                            height,
                            handle,
                        },
                    );
                    continue;
                }
            }

            self.pending_renders.insert(key, None);
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                render_page(doc, key),
                |result| match result {
                    Ok((key, page)) => Message::PageRendered {
                        key,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                        generation: None,
                    },
                    Err(error) => Message::DocumentError(error.to_string()),
                },
            ));
        }

        Task::batch(tasks)
    }

    fn request_visible_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let pages = self.visible_page_range();

        self.request_text_layers(pages, doc)
    }

    fn request_all_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let page_count = doc.page_count();

        self.request_text_layers(0..page_count, doc)
    }

    fn request_text_layers(
        &mut self,
        pages: std::ops::Range<u16>,
        doc: Arc<PdfDoc>,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        for page in pages {
            if self.viewer_text_layers.contains_key(&page)
                || self.pending_text_layers.contains(&page)
            {
                continue;
            }

            self.pending_text_layers.insert(page);
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || doc.text_layer(page))
                        .await
                        .map_err(anyhow::Error::from)?
                },
                move |result| match result {
                    Ok(layer) => Message::ViewerTextLayerLoaded {
                        page,
                        layer: Arc::new(layer),
                    },
                    Err(error) => Message::ViewerTextLayerError {
                        page,
                        error: error.to_string(),
                    },
                },
            ));
        }

        Task::batch(tasks)
    }

    fn refresh_viewer_find_matches(&mut self) {
        self.viewer_find.refresh_matches(
            self.viewer_text_layers
                .iter()
                .map(|(page, layer)| (page, layer.as_ref())),
        );
    }

    fn open_viewer_find(&mut self) -> Task<Message> {
        if self.mode != AppMode::Viewer || self.doc.is_none() {
            return Task::none();
        }

        self.viewer_find.open = true;
        self.open_app_menu = None;
        self.open_selection_menu = None;
        self.zoom_menu_open = false;
        self.refresh_viewer_find_matches();

        Task::batch([
            self.request_all_text_layers(),
            operation::focus(Id::new(VIEWER_FIND_INPUT_ID)),
        ])
    }

    fn set_viewer_find_query(&mut self, query: String) -> Task<Message> {
        self.viewer_find.query = query;
        self.refresh_viewer_find_matches();
        Task::batch([
            self.request_all_text_layers(),
            self.scroll_to_selected_viewer_find_match(),
        ])
    }

    fn scroll_to_selected_viewer_find_match(&mut self) -> Task<Message> {
        let Some(selected) = self.viewer_find.selected_match() else {
            return Task::none();
        };

        self.scroll_to_viewer_find_match(selected)
    }

    fn scroll_to_viewer_find_match(&mut self, selected: ViewerFindMatch) -> Task<Message> {
        let Some(layer) = self.viewer_text_layers.get(&selected.page) else {
            return Task::none();
        };
        let Some(character) = layer.chars.get(selected.start) else {
            return Task::none();
        };

        self.scroll_to_page_rect(selected.page, character.bounds.x, character.bounds.y);
        self.clamp_scroll_offset();
        self.clamp_horizontal_offset();
        self.request_visible_pages()
    }

    fn start_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        self.viewer_text_selection = Some(ViewerTextSelection::new(ViewerTextAnchor::new(
            page, char_index,
        )));
        self.viewer_copy_pending = false;
    }

    fn update_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        let Some(selection) = &mut self.viewer_text_selection else {
            return;
        };

        selection.focus = ViewerTextAnchor::new(page, char_index);
        self.viewer_copy_pending = false;
    }

    fn finish_viewer_text_selection(&mut self) {
        if let Some(selection) = &mut self.viewer_text_selection {
            selection.dragging = false;
        }
    }

    fn clear_viewer_text_selection(&mut self) {
        self.viewer_text_selection = None;
        self.viewer_copy_pending = false;
    }

    fn selected_text_layers_ready(&self) -> bool {
        let Some(selection) = self.viewer_text_selection else {
            return false;
        };

        let (start, end) = selection.ordered();
        (start.page..=end.page).all(|page| self.viewer_text_layers.contains_key(&page))
    }

    fn selected_viewer_text(&self) -> Option<String> {
        let selection = self.viewer_text_selection?;
        let (start, end) = selection.ordered();
        let mut text = String::new();

        for page in start.page..=end.page {
            let layer = self.viewer_text_layers.get(&page)?;
            let Some(range) = selection.char_range_for_page(page, layer.chars.len()) else {
                continue;
            };

            if !text.is_empty() {
                text.push('\n');
            }
            for index in range {
                if let Some(character) = layer.chars.get(index) {
                    text.push_str(&character.text);
                }
            }
        }

        (!text.is_empty()).then_some(text)
    }

    fn copy_selected_viewer_text(&mut self) -> Task<Message> {
        if self.viewer_text_selection.is_none() {
            return Task::none();
        }

        if self.selected_text_layers_ready() {
            self.viewer_copy_pending = false;
            self.selected_viewer_text()
                .map_or_else(Task::none, clipboard::write)
        } else {
            self.viewer_copy_pending = true;
            self.request_visible_text_layers()
        }
    }

    fn visible_page_range(&self) -> std::ops::Range<u16> {
        let Some(doc) = &self.doc else {
            return 0..0;
        };

        let viewport = Rectangle {
            x: self.horizontal_offset.max(0.0),
            y: self.scroll_offset.max(0.0),
            width: self.viewer_viewport_width.max(1.0),
            height: self.viewer_viewport_height.max(1.0),
        };
        let mut first = None;
        let mut end = 0;

        for (page, rect) in self.viewer_page_rects_content(self.viewer_viewport_width) {
            if rects_intersect(rect, viewport) {
                first.get_or_insert(page);
                end = page.saturating_add(1);
            }
        }

        let page_count = doc.page_count();
        first.unwrap_or(0)..end.max(first.unwrap_or(0).saturating_add(1).min(page_count))
    }

    fn prefetch_page_order(&self) -> Vec<u16> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        let page_count = doc.page_count();
        if page_count == 0 {
            return Vec::new();
        }

        prefetch_page_order_for_range(
            self.visible_page_range(),
            page_count,
            self.scroll_offset >= self.last_scroll_offset,
        )
    }

    fn page_height(&self, page: u16) -> f32 {
        let ratio = self
            .page_aspect_ratios
            .get(usize::from(page))
            .copied()
            .unwrap_or(11.0 / 8.5)
            .max(0.01);
        f32::from(self.zoom_width) / ratio
    }

    fn render_width_px(&self) -> u16 {
        (f32::from(self.zoom_width) * self.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    fn render_height_px(&self, page: u16) -> u16 {
        (self.page_height(page) * self.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    fn content_height(&self) -> f32 {
        self.viewer_content_size(self.viewer_viewport_width).height
    }

    fn content_width(&self) -> f32 {
        self.viewer_content_size(self.viewer_viewport_width).width
    }

    pub(crate) fn viewer_page_rects_screen(
        &self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Vec<(u16, Rectangle)> {
        self.viewer_page_rects_content(viewport_width)
            .into_iter()
            .map(|(page, rect)| {
                (
                    page,
                    Rectangle::new(
                        Point::new(rect.x - self.horizontal_offset, rect.y - self.scroll_offset),
                        rect.size(),
                    ),
                )
            })
            .filter(|(_, rect)| {
                rects_intersect(
                    *rect,
                    Rectangle {
                        x: 0.0,
                        y: 0.0,
                        width: viewport_width.max(1.0),
                        height: viewport_height.max(1.0),
                    },
                )
            })
            .collect()
    }

    fn viewer_page_rect_for_page(&self, target_page: u16) -> Option<Rectangle> {
        self.viewer_page_rects_content(self.viewer_viewport_width)
            .into_iter()
            .find_map(|(page, rect)| (page == target_page).then_some(rect))
    }

    fn viewer_page_rects_content(&self, viewport_width: f32) -> Vec<(u16, Rectangle)> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };

        let groups = viewer_spread_groups(doc.page_count(), self.viewer_spread_mode);
        match self.viewer_scroll_mode {
            ViewerScrollMode::Horizontal => self.horizontal_page_rects(&groups),
            ViewerScrollMode::Wrapped => self.wrapped_page_rects(&groups, viewport_width),
            ViewerScrollMode::Page | ViewerScrollMode::Vertical => {
                self.vertical_page_rects(&groups)
            }
        }
    }

    fn vertical_page_rects(&self, groups: &[Vec<u16>]) -> Vec<(u16, Rectangle)> {
        let content_width = viewer_groups_max_width(self, groups)
            .max(self.viewer_viewport_width)
            .max(1.0);
        let mut rects = Vec::new();
        let mut y = Spacing::PAGE_GUTTER;

        for group in groups {
            let group_width = viewer_group_width(self, group);
            let group_height = viewer_group_height(self, group);
            let mut x = ((content_width - group_width) / 2.0).max(Spacing::PAGE_GUTTER);

            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(x, y + (group_height - height) / 2.0),
                        Size::new(f32::from(self.zoom_width), height),
                    ),
                ));
                x += f32::from(self.zoom_width) + Spacing::PAGE_GAP;
            }

            y += group_height + Spacing::PAGE_GAP;
        }

        rects
    }

    fn horizontal_page_rects(&self, groups: &[Vec<u16>]) -> Vec<(u16, Rectangle)> {
        let content_size = self.viewer_content_size_for_groups(groups, self.viewer_viewport_width);
        let total_width = viewer_groups_inline_width(self, groups);
        let mut rects = Vec::new();
        let mut x = ((content_size.width - total_width) / 2.0).max(Spacing::PAGE_GUTTER);

        for group in groups {
            let group_height = viewer_group_height(self, group);
            let mut page_x = x;
            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(page_x, (content_size.height - height) / 2.0),
                        Size::new(f32::from(self.zoom_width), height),
                    ),
                ));
                page_x += f32::from(self.zoom_width) + Spacing::PAGE_GAP;
            }
            x += viewer_group_width(self, group).max(group_height * 0.0) + Spacing::PAGE_GAP;
        }

        rects
    }

    fn wrapped_page_rects(
        &self,
        groups: &[Vec<u16>],
        viewport_width: f32,
    ) -> Vec<(u16, Rectangle)> {
        let max_row_width = (viewport_width - Spacing::PAGE_GUTTER * 2.0)
            .max(viewer_groups_max_width(self, groups))
            .max(f32::from(self.zoom_width));
        let content_width = (max_row_width + Spacing::PAGE_GUTTER * 2.0)
            .max(self.viewer_viewport_width)
            .max(1.0);
        let mut rects = Vec::new();
        let mut x = Spacing::PAGE_GUTTER;
        let mut y = Spacing::PAGE_GUTTER;
        let mut row_height: f32 = 0.0;

        for group in groups {
            let group_width = viewer_group_width(self, group);
            let group_height = viewer_group_height(self, group);
            if x > Spacing::PAGE_GUTTER && x + group_width > Spacing::PAGE_GUTTER + max_row_width {
                y += row_height + Spacing::PAGE_GAP;
                x = Spacing::PAGE_GUTTER;
                row_height = 0.0;
            }

            let mut page_x = x;
            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(page_x, y + (group_height - height) / 2.0),
                        Size::new(f32::from(self.zoom_width), height),
                    ),
                ));
                page_x += f32::from(self.zoom_width) + Spacing::PAGE_GAP;
            }

            x += group_width + Spacing::PAGE_GAP;
            row_height = row_height.max(group_height);
        }

        let horizontal_padding = if content_width > max_row_width + Spacing::PAGE_GUTTER * 2.0 {
            (content_width - (max_row_width + Spacing::PAGE_GUTTER * 2.0)) / 2.0
        } else {
            0.0
        };

        if horizontal_padding > 0.0 {
            for (_, rect) in &mut rects {
                rect.x += horizontal_padding;
            }
        }

        rects
    }

    fn viewer_content_size(&self, viewport_width: f32) -> Size {
        let Some(doc) = &self.doc else {
            return Size::new(
                viewport_width.max(1.0),
                self.viewer_viewport_height.max(1.0),
            );
        };
        let groups = viewer_spread_groups(doc.page_count(), self.viewer_spread_mode);
        self.viewer_content_size_for_groups(&groups, viewport_width)
    }

    fn viewer_content_size_for_groups(&self, groups: &[Vec<u16>], viewport_width: f32) -> Size {
        match self.viewer_scroll_mode {
            ViewerScrollMode::Horizontal => Size::new(
                viewer_groups_inline_width(self, groups)
                    .max(viewport_width)
                    .max(1.0),
                (viewer_groups_max_height(self, groups) + Spacing::PAGE_GUTTER * 2.0)
                    .max(self.viewer_viewport_height)
                    .max(1.0),
            ),
            ViewerScrollMode::Wrapped => {
                let rects = self.wrapped_page_rects(groups, viewport_width);
                let height = rects
                    .iter()
                    .map(|(_, rect)| rect.y + rect.height)
                    .fold(0.0, f32::max)
                    + Spacing::PAGE_GUTTER;
                Size::new(
                    viewport_width
                        .max(viewer_groups_max_width(self, groups))
                        .max(1.0),
                    height.max(self.viewer_viewport_height).max(1.0),
                )
            }
            ViewerScrollMode::Page | ViewerScrollMode::Vertical => {
                let height: f32 = groups
                    .iter()
                    .map(|group| viewer_group_height(self, group) + Spacing::PAGE_GAP)
                    .sum();
                Size::new(
                    viewer_groups_max_width(self, groups)
                        .max(viewport_width)
                        .max(1.0),
                    (height + Spacing::PAGE_GUTTER * 2.0)
                        .max(self.viewer_viewport_height)
                        .max(1.0),
                )
            }
        }
    }

    pub(crate) fn current_page(&self) -> u16 {
        self.visible_page_range().start
    }

    fn visible_library_entries(&self) -> Vec<LibraryEntry> {
        let source = self
            .search_results
            .as_ref()
            .unwrap_or(&self.library_entries);
        source
            .iter()
            .filter(|entry| {
                self.active_tag_filter
                    .as_ref()
                    .is_none_or(|tag| entry.tags.iter().any(|entry_tag| entry_tag == tag))
            })
            .filter(|entry| entry_visible_in_folder_scope(entry, self.selected_folder.as_ref()))
            .filter(|entry| {
                self.active_reading_filter
                    .is_none_or(|filter| library_entry_reading_state(entry) == filter)
            })
            .filter(|entry| !self.missing_filter_active || entry.missing)
            .cloned()
            .collect()
    }

    fn library_grid_zoom(&self) -> f32 {
        self.library_grid_zoom
            .clamp(LIBRARY_GRID_ZOOM_MIN, self.library_grid_zoom_max())
    }

    fn library_grid_zoom_max(&self) -> f32 {
        let width = self.library_available_grid_width();
        (width / self.layout().library_grid_card_width)
            .max(1.0)
            .clamp(1.0, LIBRARY_GRID_ZOOM_MAX)
    }

    fn library_available_grid_width(&self) -> f32 {
        let sidebar_width = if self.library_tag_sidebar_open {
            self.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        let window_main_width = (self.viewport_width - sidebar_width).max(1.0);
        self.library_viewport_width
            .max(window_main_width)
            .max(self.layout().window_size()[0] - sidebar_width)
            - Spacing::LG * 2.0
            - self.layout().library_scrollbar_gutter
    }

    fn recalculate_library_viewport_width(&mut self) {
        let sidebar_width = if self.library_tag_sidebar_open {
            self.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        self.library_viewport_width =
            (self.viewport_width - sidebar_width - Spacing::LG * 2.0).max(1.0);
    }

    fn fit_library_grid_zoom_to_columns(&mut self, columns: usize) {
        if self.compact_view_mode || columns == 0 {
            return;
        }
        let columns = columns.min(LIBRARY_GRID_ZOOM_DENSE_COLUMN_CAP);
        let available_width = self.library_available_grid_width().max(1.0);
        let total_gap = columns.saturating_sub(1) as f32 * self.layout().library_masonry_gap;
        let card_width = ((available_width - total_gap) / columns as f32).max(1.0);
        self.library_grid_zoom = (card_width / self.layout().library_grid_card_width)
            .clamp(LIBRARY_GRID_ZOOM_MIN, self.library_grid_zoom_max());
    }

    fn library_grid_card_width(&self) -> f32 {
        self.layout().library_grid_card_width * self.library_grid_zoom()
    }

    fn library_card_info_height(&self) -> f32 {
        (self.layout().library_card_info_height * self.library_grid_zoom()).clamp(88.0, 176.0)
    }

    fn library_card_media_max_height(&self) -> f32 {
        self.layout().library_card_media_max_height * self.library_grid_zoom()
    }

    fn library_card_title_width(&self) -> f32 {
        self.layout().library_card_title_width * self.library_grid_zoom()
    }

    fn library_card_text_scale(&self) -> f32 {
        self.library_grid_zoom().clamp(0.55, 1.35)
    }

    fn library_card_font_size(&self, base_size: u32) -> u32 {
        ((base_size as f32) * self.library_card_text_scale())
            .round()
            .clamp(8.0, 28.0) as u32
    }

    fn library_card_padding(&self) -> f32 {
        (Spacing::LG * self.library_card_text_scale()).clamp(4.0, 24.0)
    }

    fn library_card_spacing(&self) -> f32 {
        (Spacing::SM * self.library_card_text_scale()).clamp(2.0, Spacing::SM)
    }

    fn library_card_title_font_size(&self) -> u32 {
        self.library_card_font_size(16)
    }

    fn thumbnail_size_for_grid_zoom(&self) -> ThumbnailSize {
        let width = self.library_grid_card_width();
        if width <= 140.0 {
            ThumbnailSize::Small
        } else if width >= 340.0 {
            ThumbnailSize::Large
        } else {
            ThumbnailSize::Default
        }
    }

    fn thumbnail_for_entry(
        &self,
        entry_id: &EntryId,
        preferred_size: ThumbnailSize,
    ) -> Option<&ThumbnailView> {
        [
            preferred_size,
            ThumbnailSize::Default,
            ThumbnailSize::Large,
            ThumbnailSize::Small,
        ]
        .into_iter()
        .find_map(|size| {
            self.thumbnails.get(&ThumbnailCacheKey {
                entry_id: entry_id.clone(),
                size,
            })
        })
    }

    fn library_grid_zoom_label(&self) -> String {
        format!("{:.0}%", self.library_grid_zoom() * 100.0)
    }

    fn child_folders(&self) -> Vec<Folder> {
        let mut folders = self
            .library_folders
            .iter()
            .filter(|folder| folder.parent_id == self.selected_folder)
            .cloned()
            .collect::<Vec<_>>();
        folders.sort_by_key(|folder| (folder.manual_order, folder.name.to_lowercase()));
        folders
    }

    fn folder_smart_counts(&self, folder_id: Option<&FolderId>) -> FolderSmartCounts {
        let folder_ids = folder_id.map(|id| self.folder_subtree_ids(id));
        let entries = self.library_entries.iter().filter(|entry| {
            folder_ids.as_ref().map_or(true, |folder_ids| {
                entry
                    .folders
                    .iter()
                    .any(|folder| folder_ids.contains(&folder.id))
            })
        });
        let mut counts = FolderSmartCounts::default();
        for entry in entries {
            counts.total += 1;
            if entry.missing {
                counts.missing += 1;
            }
            if entry.page_count.is_some_and(|page_count| {
                page_count > 0 && entry.last_page.saturating_add(1) < page_count
            }) {
                counts.in_progress += 1;
            }
        }
        counts
    }

    fn folder_subtree_ids(&self, folder_id: &FolderId) -> HashSet<FolderId> {
        let mut folder_ids = HashSet::new();
        self.collect_folder_subtree_ids(folder_id, &mut folder_ids);
        folder_ids
    }

    fn collect_folder_subtree_ids(&self, folder_id: &FolderId, folder_ids: &mut HashSet<FolderId>) {
        if !folder_ids.insert(folder_id.clone()) {
            return;
        }
        for child in self
            .library_folders
            .iter()
            .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
        {
            self.collect_folder_subtree_ids(&child.id, folder_ids);
        }
    }

    fn selected_folder_name(&self) -> Option<String> {
        self.selected_folder().map(|folder| folder.name.clone())
    }

    fn selected_folder(&self) -> Option<&Folder> {
        self.selected_folder.as_ref().and_then(|selected| {
            self.library_folders
                .iter()
                .find(|folder| &folder.id == selected)
        })
    }

    fn selected_folder_sibling_order(&self) -> Option<(Option<FolderId>, Vec<FolderId>, usize)> {
        let folder = self.selected_folder()?;
        let parent_id = folder.parent_id.clone();
        let mut siblings = self
            .library_folders
            .iter()
            .filter(|candidate| candidate.parent_id == parent_id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| (candidate.manual_order, candidate.name.to_lowercase()));
        let folder_ids = siblings
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let index = folder_ids
            .iter()
            .position(|folder_id| folder_id == &folder.id)?;
        Some((parent_id, folder_ids, index))
    }

    fn selected_folder_manual_reorder(
        &self,
        direction: isize,
    ) -> Option<(Option<FolderId>, Vec<FolderId>)> {
        let (parent_id, mut folder_ids, index) = self.selected_folder_sibling_order()?;
        let next_index = index.checked_add_signed(direction)?;
        if next_index >= folder_ids.len() {
            return None;
        }
        folder_ids.swap(index, next_index);
        Some((parent_id, folder_ids))
    }

    fn folder_drag_manual_reorder(
        &self,
        folder_id: &FolderId,
        target_id: &FolderId,
    ) -> Option<(Option<FolderId>, Vec<FolderId>)> {
        let folder = self
            .library_folders
            .iter()
            .find(|folder| &folder.id == folder_id)?;
        let target = self
            .library_folders
            .iter()
            .find(|folder| &folder.id == target_id)?;
        if folder.parent_id != target.parent_id || folder.id == target.id {
            return None;
        }

        let mut siblings = self
            .library_folders
            .iter()
            .filter(|candidate| candidate.parent_id == folder.parent_id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| (candidate.manual_order, candidate.name.to_lowercase()));
        let folder_ids = siblings
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let next_order = reorder_folder_ids_before_target(&folder_ids, folder_id, target_id)?;
        (next_order != folder_ids).then_some((folder.parent_id.clone(), next_order))
    }

    fn sync_folder_rename_input(&mut self) {
        self.folder_rename_input = self
            .selected_folder()
            .map_or_else(String::new, |folder| folder.name.clone());
    }

    fn folder_breadcrumbs(&self) -> Vec<(String, Option<FolderId>)> {
        let mut breadcrumbs = vec![(String::from("Library"), None)];
        let mut current = self.selected_folder.clone();
        let mut path = Vec::new();
        let mut seen = HashSet::new();

        while let Some(folder_id) = current {
            if !seen.insert(folder_id.clone()) {
                break;
            }

            let Some(folder) = self
                .library_folders
                .iter()
                .find(|folder| folder.id == folder_id)
            else {
                break;
            };

            path.push((folder.name.clone(), Some(folder.id.clone())));
            current = folder.parent_id.clone();
        }

        path.reverse();
        breadcrumbs.extend(path);
        breadcrumbs
    }

    fn select_library_entry(&mut self, entry_id: EntryId) {
        let visible_entries = self.visible_library_entries();
        if self.modifiers.shift() {
            self.select_library_range(entry_id, &visible_entries);
        } else if self.modifiers.control() {
            if !self.selected_library_entries.insert(entry_id.clone()) {
                self.selected_library_entries.remove(&entry_id);
            }
            self.library_selection_anchor = Some(entry_id);
        } else {
            self.selected_library_entries.clear();
            self.selected_library_entries.insert(entry_id.clone());
            self.library_selection_anchor = Some(entry_id);
        }

        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    fn toggle_library_entry_selection(&mut self, entry_id: EntryId) {
        toggle_selection_entry_id(&mut self.selected_library_entries, entry_id.clone());
        self.library_selection_anchor = Some(entry_id);
        let visible_entries = self.visible_library_entries();
        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    fn master_checkbox_state(&self) -> MasterCheckboxState {
        let visible_entries = self.visible_library_entries();
        if visible_entries.is_empty() {
            return MasterCheckboxState::None;
        }

        let selected_visible = visible_entries
            .iter()
            .filter(|entry| self.selected_library_entries.contains(&entry.id))
            .count();

        master_checkbox_state_for_counts(selected_visible, visible_entries.len())
    }

    fn select_library_range(&mut self, entry_id: EntryId, visible_entries: &[LibraryEntry]) {
        let anchor = self
            .library_selection_anchor
            .clone()
            .or_else(|| self.selected_library_entries.iter().next().cloned())
            .unwrap_or_else(|| entry_id.clone());
        let Some(anchor_index) = visible_entries.iter().position(|entry| entry.id == anchor) else {
            self.selected_library_entries.clear();
            self.selected_library_entries.insert(entry_id.clone());
            self.library_selection_anchor = Some(entry_id);
            return;
        };
        let Some(entry_index) = visible_entries
            .iter()
            .position(|entry| entry.id == entry_id)
        else {
            return;
        };

        self.selected_library_entries.clear();
        let visible_ids = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        self.selected_library_entries.extend(range_selection_ids(
            anchor_index,
            entry_index,
            &visible_ids,
        ));
        self.library_selection_anchor = Some(anchor);
    }

    fn select_all_visible_library_entries(&mut self) {
        let visible_entries = self.visible_library_entries();
        self.selected_library_entries = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.library_selection_anchor = visible_entries.first().map(|entry| entry.id.clone());
        self.sync_details_editor_to_selection();
    }

    fn clear_library_selection(&mut self) {
        self.selected_library_entries.clear();
        self.library_selection_anchor = None;
        self.open_selection_menu = None;
        self.sync_details_editor_to_selection();
    }

    fn prune_selection_to_visible_entries(&mut self, visible_entries: &[LibraryEntry]) {
        let visible_ids = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.selected_library_entries
            .retain(|entry_id| visible_ids.contains(entry_id));
        if self
            .library_selection_anchor
            .as_ref()
            .is_some_and(|anchor| !visible_ids.contains(anchor))
        {
            self.library_selection_anchor = self.selected_library_entries.iter().next().cloned();
        }
        self.sync_details_editor_to_selection();
    }

    fn selected_entries(&self) -> Vec<LibraryEntry> {
        self.library_entries
            .iter()
            .filter(|entry| self.selected_library_entries.contains(&entry.id))
            .cloned()
            .collect()
    }

    fn primary_selected_entry(&self) -> Option<LibraryEntry> {
        if self.selected_library_entries.len() != 1 {
            return None;
        }

        let entry_id = self.selected_library_entries.iter().next()?;
        self.library_entries
            .iter()
            .find(|entry| &entry.id == entry_id)
            .cloned()
    }

    fn sync_details_editor_to_selection(&mut self) {
        let Some(entry) = self.primary_selected_entry() else {
            self.details_entry_id = None;
            self.details_title_input.clear();
            self.details_author_input.clear();
            return;
        };

        if self.details_entry_id.as_ref() == Some(&entry.id) {
            return;
        }

        self.details_title_input = entry_title(&entry);
        self.details_author_input = entry
            .display_author
            .clone()
            .or_else(|| entry.author.clone())
            .unwrap_or_default();
        self.details_entry_id = Some(entry.id);
    }

    fn visible_library_entry_window_at(
        &self,
        entries_len: usize,
        scroll_offset: f32,
    ) -> std::ops::Range<usize> {
        if entries_len == 0 {
            return 0..0;
        }

        let per_row = self.library_entries_per_row();
        let row_height = self.library_row_height();
        let first_row = (scroll_offset / row_height).floor().max(0.0) as usize;
        let visible_rows = (self.library_viewport_height / row_height).ceil().max(1.0) as usize;
        let start_row = first_row.saturating_sub(self.layout().library_overscan_rows);
        let end_row = first_row
            .saturating_add(visible_rows)
            .saturating_add(self.layout().library_overscan_rows)
            .saturating_add(1);

        let start = (start_row * per_row).min(entries_len);
        let end = (end_row * per_row).min(entries_len);
        start..end
    }

    fn visible_library_masonry_layout_items_at<'a>(
        &self,
        layout: &'a LibraryMasonryLayout,
        scroll_offset: f32,
    ) -> Vec<&'a LibraryMasonryItem> {
        let top = scroll_offset.max(0.0)
            - self.layout().library_overscan_rows as f32 * self.library_row_height();
        let bottom = scroll_offset.max(0.0)
            + self.library_viewport_height.max(1.0)
            + self.layout().library_overscan_rows as f32 * self.library_row_height();
        let mut items = layout
            .columns
            .iter()
            .flat_map(|column| column.iter())
            .filter(|item| item.top + item.height >= top && item.top <= bottom)
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.index);
        items
    }

    fn library_entries_per_row(&self) -> usize {
        if self.compact_view_mode {
            1
        } else {
            let available_width = self.library_available_grid_width();
            let column_pitch = self.library_grid_card_width() + self.layout().library_masonry_gap;
            ((available_width + self.layout().library_masonry_gap) / column_pitch)
                .floor()
                .max(1.0)
                .min(LIBRARY_GRID_ZOOM_DENSE_COLUMN_CAP as f32) as usize
        }
    }

    fn library_row_height(&self) -> f32 {
        if self.compact_view_mode {
            self.layout().library_list_row_height + LIBRARY_ROW_HOVER_LIFT
        } else {
            self.layout().library_grid_row_height * self.library_grid_zoom()
        }
    }

    fn library_masonry_layout(&self, entries: &[LibraryEntry]) -> LibraryMasonryLayout {
        let column_count = self.library_entries_per_row().max(1);
        let mut columns = vec![Vec::new(); column_count];
        let mut column_heights = vec![0.0; column_count];

        for (index, entry) in entries.iter().enumerate() {
            let column = shortest_column_index(&column_heights);
            let top = column_heights[column];
            let height = self.library_card_estimated_height(&entry.id);
            columns[column].push(LibraryMasonryItem { index, top, height });
            column_heights[column] = top + height + self.layout().library_masonry_gap;
        }

        let content_height = column_heights
            .into_iter()
            .map(|height| (height - self.layout().library_masonry_gap).max(0.0))
            .fold(0.0, f32::max);

        LibraryMasonryLayout {
            columns,
            content_height,
        }
    }

    fn library_render_item_masonry_layout(
        &self,
        items: &[LibraryRenderItem],
    ) -> LibraryMasonryLayout {
        let entries = items
            .iter()
            .map(LibraryRenderItem::entry)
            .cloned()
            .collect::<Vec<_>>();
        self.library_masonry_layout(&entries)
    }

    fn library_card_estimated_height(&self, entry_id: &EntryId) -> f32 {
        let thumbnail_height = self
            .thumbnail_for_entry(entry_id, self.thumbnail_size_for_grid_zoom())
            .map(|thumbnail| {
                let height = self.library_grid_card_width() * f32::from(thumbnail.height)
                    / f32::from(thumbnail.width.max(1));
                height.min(self.library_card_media_max_height())
            })
            .unwrap_or(self.library_card_media_max_height());

        thumbnail_height + self.library_card_info_height() + LIBRARY_CARD_HOVER_LIFT
    }

    fn library_card_hover_progress(&self, entry_id: &EntryId) -> f32 {
        self.library_card_hover_animations
            .get(entry_id)
            .map(|animation| animation.interpolate(0.0, 1.0, self.animation_now))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }

    fn set_library_card_hover(&mut self, entry_id: EntryId, hovered: bool) {
        self.animation_now = Instant::now();
        let animation = self
            .library_card_hover_animations
            .entry(entry_id)
            .or_insert_with(Self::library_card_hover_animation);
        animation.go_mut(hovered, self.animation_now);
    }

    fn tick_animations(&mut self, now: Instant) {
        self.animation_now = now;
        let visible_entry_ids = self
            .visible_library_entries()
            .into_iter()
            .map(|entry| entry.id)
            .collect::<HashSet<_>>();
        self.library_card_hover_animations
            .retain(|entry_id, animation| {
                animation.is_animating(now) || visible_entry_ids.contains(entry_id)
            });
        self.expire_folder_drop_flash(now);
        self.expire_viewer_page_fades(now);
    }

    fn start_bulk_operation_progress(&mut self, label: impl Into<String>, total: usize) {
        let label = label.into();
        self.library_status = Some(format!("{label} {total} PDFs..."));
        self.bulk_operation_progress = Some(BulkOperationProgress {
            label,
            total,
            started_at: Instant::now(),
        });
    }

    fn start_folder_drop_flash(&mut self, folder_id: FolderId, now: Instant) {
        self.folder_drop_flash = Some((folder_id, now));
        self.animation_now = now;
    }

    fn expire_folder_drop_flash(&mut self, now: Instant) {
        if self
            .folder_drop_flash
            .as_ref()
            .is_some_and(|(_, started_at)| {
                now.saturating_duration_since(*started_at)
                    >= Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
            })
        {
            self.folder_drop_flash = None;
        }
    }

    fn folder_drop_flash_active(&self, folder_id: &FolderId) -> bool {
        folder_drop_flash_active_at(
            folder_id,
            self.folder_drop_flash
                .as_ref()
                .map(|(flashed_folder_id, started_at)| (flashed_folder_id, *started_at)),
            self.animation_now,
        )
    }

    fn library_card_hover_animation() -> Animation<bool> {
        Animation::new(false)
            .duration(Duration::from_millis(LIBRARY_CARD_HOVER_DURATION_MS))
            .easing(animation::Easing::EaseOutCubic)
    }

    fn library_card_hover_animation_active(&self) -> bool {
        self.library_card_hover_animations
            .values()
            .any(|animation| animation.is_animating(self.animation_now))
    }

    fn expire_viewer_page_fades(&mut self, now: Instant) {
        self.page_fade_started.retain(|_, started_at| {
            now.saturating_duration_since(*started_at) < Duration::from_millis(VIEWER_PAGE_FADE_MS)
        });
    }

    fn viewer_page_fade_active(&self) -> bool {
        !self.page_fade_started.is_empty()
    }

    fn clear_library_transient_interactions(&mut self) {
        self.library_card_hover_animations.clear();
        self.folder_drop_flash = None;
        self.library_drag = None;
        self.folder_drag = None;
        self.resizing_library_tag_sidebar = false;
    }

    fn can_drag_reorder_library(&self) -> bool {
        can_drag_reorder_library_for_state(
            self.library_sort_mode,
            &self.search_query,
            self.search_results.is_some(),
            self.active_tag_filter.is_some(),
            self.selected_folder.is_some(),
        )
    }

    fn begin_library_drag(&mut self, entry_id: EntryId) {
        self.folder_drag = None;
        let visible_entries = self.visible_library_entries();
        let Some(source_index) = visible_entries
            .iter()
            .position(|entry| entry.id == entry_id)
        else {
            return;
        };
        let multi = self.selected_library_entries.len() > 1
            && self.selected_library_entries.contains(&entry_id);
        let entry_ids = if multi {
            visible_entries
                .iter()
                .filter(|entry| self.selected_library_entries.contains(&entry.id))
                .map(|entry| entry.id.clone())
                .collect()
        } else {
            vec![entry_id.clone()]
        };

        self.library_drag = Some(LibraryDragState::new(
            entry_id,
            entry_ids,
            source_index,
            multi,
        ));
    }

    fn begin_folder_drag(&mut self, folder_id: FolderId) {
        if !self
            .library_folders
            .iter()
            .any(|folder| folder.id == folder_id)
        {
            return;
        }

        self.library_drag = None;
        self.folder_drag = Some(FolderDragState::new(folder_id));
    }

    fn update_library_drag_target(&mut self, cursor: Point) {
        if self.library_drag.is_none() {
            return;
        }

        let can_drag_reorder = self.can_drag_reorder_library();
        if let Some(drag) = &mut self.library_drag {
            if drag.update_cursor(cursor) && !can_drag_reorder && drag.drop_target.is_none() {
                self.library_status = Some(String::from(
                    "Drop on a folder, or switch to unfiltered Manual sort to reorder PDFs.",
                ));
            }
        }

        if self.library_drag.as_ref().is_some_and(|drag| drag.active) {
            self.update_library_drag_target_from_cursor();
            if let Some(target) = self.library_folder_card_target_at_cursor(cursor) {
                self.set_library_drag_card_target(Some(target), Instant::now());
            }
        }
    }

    fn set_folder_drop_hover_target(&mut self, folder_id: Option<FolderId>, now: Instant) {
        if self.library_drag.is_none() && self.folder_drag.is_none() {
            return;
        };
        let library_drag_card_target = if folder_id.is_none() {
            self.library_drag
                .as_ref()
                .filter(|drag| drag.active)
                .and_then(|drag| {
                    drag.cursor
                        .and_then(|cursor| self.library_folder_card_target_at_cursor(cursor))
                })
        } else {
            None
        };
        let folder_drag_card_target = if folder_id.is_none() {
            self.folder_drag
                .as_ref()
                .filter(|drag| drag.active)
                .and_then(|drag| {
                    drag.cursor.and_then(|cursor| {
                        self.folder_card_target_at_cursor(cursor, &drag.folder_id)
                    })
                })
        } else {
            None
        };

        if let Some(drag) = &mut self.library_drag {
            let target = folder_id.clone().or(library_drag_card_target);
            drag.set_pending_folder_target(target, now);
        }

        if let Some(drag) = &mut self.folder_drag {
            let target = folder_id.or(folder_drag_card_target).filter(|target| {
                folder_can_move_into(&self.library_folders, &drag.folder_id, target)
            });
            drag.set_drop_target(target, now, true);
        }
    }

    fn update_folder_drop_target_dwell(&mut self, now: Instant) {
        let library_target = self
            .library_drag
            .as_ref()
            .and_then(|drag| drag.pending_target_ready(now));

        let folder_target = self
            .folder_drag
            .as_ref()
            .and_then(|drag| drag.pending_target_ready(now));

        let Some(folder_id) = library_target.or(folder_target) else {
            return;
        };

        let should_expand = self.folder_has_children(&folder_id)
            && self.collapsed_library_tree_folders.contains(&folder_id);
        if should_expand {
            self.collapsed_library_tree_folders.remove(&folder_id);
        }

        if let Some(drag) = &mut self.library_drag {
            drag.drop_target = Some(folder_id.clone());
            if should_expand {
                drag.expanded_during_drag.insert(folder_id.clone());
            }
        }
        if let Some(drag) = &mut self.folder_drag {
            drag.drop_target = Some(folder_id.clone());
            if should_expand {
                drag.expanded_during_drag.insert(folder_id);
            }
        }
    }

    fn update_folder_drag_target(&mut self, cursor: Point) {
        let Some(drag) = &mut self.folder_drag else {
            return;
        };

        if !drag.update_cursor(cursor) {
            return;
        }

        let dragged_folder_id = drag.folder_id.clone();
        if let Some(target) = self.folder_card_target_at_cursor(cursor, &dragged_folder_id) {
            self.set_folder_drag_card_target(Some(target));
        }
    }

    fn set_folder_drag_card_target(&mut self, folder_id: Option<FolderId>) {
        let Some(drag) = &mut self.folder_drag else {
            return;
        };
        let target = folder_id
            .filter(|target| folder_can_move_into(&self.library_folders, &drag.folder_id, target));
        drag.set_drop_target(target, Instant::now(), true);
    }

    fn set_library_drag_card_target(&mut self, folder_id: Option<FolderId>, now: Instant) {
        let Some(drag) = &mut self.library_drag else {
            return;
        };
        drag.set_pending_folder_target(folder_id, now);
    }

    fn active_folder_drop_target(&self) -> Option<&FolderId> {
        active_folder_drop_target(self.library_drag.as_ref(), self.folder_drag.as_ref())
    }

    fn folder_card_target_at_cursor(
        &self,
        cursor: Point,
        dragged_folder_id: &FolderId,
    ) -> Option<FolderId> {
        let child_folders = self.child_folders();
        folder_card_target_at_cursor(
            cursor,
            &child_folders,
            dragged_folder_id,
            self.library_viewport_x,
            self.library_viewport_y,
            self.library_scroll_offset,
            self.library_grid_card_width(),
            self.layout().library_folder_grid_row_height,
            self.layout().library_masonry_gap,
            Spacing::SM,
            folder_cards_per_row(self),
        )
    }

    fn library_folder_card_target_at_cursor(&self, cursor: Point) -> Option<FolderId> {
        let child_folders = self.child_folders();
        let dragged_folder_sentinel = FolderId::new("__pdf_folio_library_drag__");
        folder_card_target_at_cursor(
            cursor,
            &child_folders,
            &dragged_folder_sentinel,
            self.library_viewport_x,
            self.library_viewport_y,
            self.library_scroll_offset,
            self.library_grid_card_width(),
            self.layout().library_folder_grid_row_height,
            self.layout().library_masonry_gap,
            Spacing::SM,
            folder_cards_per_row(self),
        )
    }

    fn collapse_drag_expanded_folders(&mut self, folders: HashSet<FolderId>) {
        for folder_id in folders {
            self.collapsed_library_tree_folders.insert(folder_id);
        }
    }

    fn folder_has_children(&self, folder_id: &FolderId) -> bool {
        self.library_folders
            .iter()
            .any(|folder| folder.parent_id.as_ref() == Some(folder_id))
    }

    fn update_library_drag_target_from_cursor(&mut self) {
        let entries = self.visible_library_entries();
        let entries_len = entries.len();
        if entries_len == 0 {
            return;
        }

        let Some(cursor) = self.library_drag.as_ref().and_then(|drag| drag.cursor) else {
            return;
        };

        let dragged_ids = self
            .library_drag
            .as_ref()
            .map(|drag| drag.entry_ids.iter().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let compact_entries = entries
            .iter()
            .filter(|entry| !dragged_ids.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();
        let compact_len = compact_entries.len();
        let content_y = (cursor.y - self.library_viewport_y + self.library_scroll_offset).max(0.0);
        let index = if self.compact_view_mode {
            let row = (content_y / self.library_row_height()).round().max(0.0) as usize;
            row.saturating_mul(self.library_entries_per_row())
        } else {
            let per_row = self.library_entries_per_row().max(1);
            let column_step =
                (self.library_grid_card_width() + self.layout().library_masonry_gap).max(1.0);
            let content_x = (cursor.x - self.library_viewport_x).max(0.0);
            let column = (content_x / column_step)
                .floor()
                .clamp(0.0, per_row.saturating_sub(1) as f32) as usize;
            let layout = self.library_masonry_layout(&compact_entries);
            masonry_target_index(&layout, column, content_y).unwrap_or(compact_len)
        };

        let target_index = index.min(compact_len);
        if let Some(drag) = &mut self.library_drag {
            drag.target_index = target_index;
        }
    }

    fn library_content_height_for_len(&self, entries_len: usize) -> f32 {
        if entries_len == 0 {
            return 0.0;
        }

        if !self.compact_view_mode {
            return self
                .library_masonry_layout(&self.visible_library_entries())
                .content_height;
        }

        let rows = entries_len.div_ceil(self.library_entries_per_row());
        let row_gap = if self.compact_view_mode {
            Spacing::SM
        } else {
            Spacing::MD
        };
        rows as f32 * self.library_row_height() + rows.saturating_sub(1) as f32 * row_gap
    }

    fn max_library_scroll_offset(&self) -> f32 {
        let content_height =
            self.library_content_height_for_len(self.visible_library_entries().len());
        (content_height - self.library_viewport_height.max(1.0)).max(0.0)
    }

    fn library_drag_auto_scroll_velocity(&self) -> f32 {
        let Some(cursor) = self.library_drag.as_ref().and_then(|drag| drag.cursor) else {
            return 0.0;
        };

        if !self.library_drag.as_ref().is_some_and(|drag| drag.active) {
            return 0.0;
        }

        if self.library_viewport_height <= 1.0 {
            return 0.0;
        }

        drag_auto_scroll_velocity(
            cursor.y,
            self.library_viewport_y,
            self.library_viewport_height,
        )
    }

    fn auto_scroll_library_drag(&mut self, tick: Instant) -> Task<Message> {
        if self.library_drag.is_none() && self.folder_drag.is_none() {
            return Task::none();
        }

        self.update_folder_drop_target_dwell(tick);

        if self.library_drag.is_none() {
            return Task::none();
        }

        let last_tick = self
            .library_drag
            .as_ref()
            .and_then(|drag| drag.last_auto_scroll_tick)
            .unwrap_or(tick);
        if let Some(drag) = &mut self.library_drag {
            drag.last_auto_scroll_tick = Some(tick);
        }

        let dt = tick
            .checked_duration_since(last_tick)
            .map_or(1.0 / 60.0, |duration| {
                duration
                    .as_secs_f32()
                    .clamp(1.0 / 120.0, LIBRARY_DRAG_AUTOSCROLL_MAX_DT)
            });
        let velocity = self.library_drag_auto_scroll_velocity();
        if velocity == 0.0 {
            return Task::none();
        }

        let previous_offset = self.library_scroll_offset;
        let next_offset =
            (previous_offset + velocity * dt).clamp(0.0, self.max_library_scroll_offset());
        let delta = next_offset - previous_offset;
        if delta.abs() < 0.5 {
            return Task::none();
        }

        self.library_scroll_offset = next_offset;
        self.update_library_drag_target_from_cursor();

        Task::batch([
            scroll_library_to_offset_task(next_offset),
            self.request_visible_thumbnails(),
        ])
    }

    fn finish_library_drag(&mut self) -> Task<Message> {
        let Some(drag) = self.library_drag.take() else {
            return Task::none();
        };

        if !drag.active {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::done(Message::LibraryEntryClicked(drag.entry_id));
        }

        if let Some(folder_id) = drag.drop_target.clone() {
            if self.selected_folder.as_ref() == Some(&folder_id) {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                return scroll_library_to_offset_task(self.library_scroll_offset);
            }
            let entry_ids = drag.entry_ids.clone();
            if entry_ids.is_empty() {
                return Task::none();
            }
            self.library_status = Some(format!(
                "Adding {} to folder...",
                format_count(entry_ids.len(), "PDF")
            ));
            return Task::batch([
                move_entries_to_folder_task(Arc::clone(&self.db), entry_ids, folder_id),
                scroll_library_to_offset_task(self.library_scroll_offset),
            ]);
        }

        if !self.can_drag_reorder_library() {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return scroll_library_to_offset_task(self.library_scroll_offset);
        }

        let entries = self.visible_library_entries();
        let entry_ids: Vec<EntryId> = entries.iter().map(|entry| entry.id.clone()).collect();
        let next_order = reorder_entry_ids_for_drag(&entry_ids, &drag.entry_ids, drag.target_index);
        if next_order == entry_ids {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return scroll_library_to_offset_task(self.library_scroll_offset);
        }
        if next_order.len() != entries.len() {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        }

        let entries_by_id = entries
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        let entries = next_order
            .iter()
            .filter_map(|entry_id| entries_by_id.get(entry_id).cloned())
            .collect::<Vec<_>>();

        self.library_entries = entries;
        self.collapse_drag_expanded_folders(drag.expanded_during_drag);
        self.library_status = Some(if drag.multi {
            format!("Saving manual order for {} PDFs...", drag.entry_ids.len())
        } else {
            String::from("Saving manual PDF order...")
        });
        Task::batch([
            persist_manual_entry_order_task(Arc::clone(&self.db), next_order),
            scroll_library_to_offset_task(self.library_scroll_offset),
        ])
    }

    fn finish_folder_drag(&mut self) -> Task<Message> {
        let Some(drag) = self.folder_drag.take() else {
            return Task::none();
        };

        if !drag.active {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::done(Message::FolderSelected(Some(drag.folder_id)));
        }

        if let Some(target_id) = drag.drop_target.clone() {
            if !folder_can_move_into(&self.library_folders, &drag.folder_id, &target_id) {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                self.library_error = Some(String::from("That folder cannot be moved there."));
                return Task::none();
            }

            self.library_status = Some(String::from("Moving folder..."));
            self.start_folder_drop_flash(target_id.clone(), Instant::now());
            return Task::batch([
                move_folder_task(Arc::clone(&self.db), drag.folder_id, Some(target_id)),
                scroll_library_to_offset_task(self.library_scroll_offset),
            ]);
        }

        let Some(target_id) = drag.pending_drop_target.clone() else {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        };

        let Some((parent_id, next_order)) =
            self.folder_drag_manual_reorder(&drag.folder_id, &target_id)
        else {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        };

        self.collapse_drag_expanded_folders(drag.expanded_during_drag);
        self.library_status = Some(String::from("Saving manual folder order..."));
        Task::batch([
            persist_manual_folder_order_task(Arc::clone(&self.db), parent_id, next_order),
            scroll_library_to_offset_task(self.library_scroll_offset),
        ])
    }

    fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .library_entries
            .iter()
            .flat_map(|entry| entry.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    fn request_visible_thumbnails(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let entries = self.visible_library_entries();
        let folder_section_height = folder_cards_section_height(self, self.child_folders().len());
        let entry_scroll_offset = (self.library_scroll_offset - folder_section_height).max(0.0);
        let visible_entries = if self.compact_view_mode {
            let window = self.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
            entries[window].to_vec()
        } else {
            let layout = self.library_masonry_layout(&entries);
            self.visible_library_masonry_layout_items_at(&layout, entry_scroll_offset)
                .into_iter()
                .filter_map(|item| entries.get(item.index).cloned())
                .collect()
        };
        let thumbnail_size = if self.compact_view_mode {
            ThumbnailSize::Default
        } else {
            self.thumbnail_size_for_grid_zoom()
        };
        for entry in visible_entries {
            let key = ThumbnailCacheKey {
                entry_id: entry.id.clone(),
                size: thumbnail_size,
            };
            if self.thumbnails.contains_key(&key) || self.pending_thumbnails.contains(&key) {
                continue;
            }
            self.pending_thumbnails.insert(key);
            tasks.push(Task::perform(
                load_or_render_thumbnail(entry, thumbnail_size),
                |result| match result {
                    Ok((entry_id, size, page)) => Message::ThumbnailReady {
                        entry_id,
                        size,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                    },
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ));
        }

        Task::batch(tasks)
    }

    fn refresh_library(&mut self) -> Task<Message> {
        let db = Arc::clone(&self.db);
        let sort_mode = self.library_sort_mode;
        Task::perform(
            async move { tokio::task::spawn_blocking(move || db.get_entries_sorted(sort_mode)).await? },
            |result| match result {
                Ok(entries) => Message::LibraryLoaded(entries),
                Err(error) => Message::LibraryError(error.to_string()),
            },
        )
    }

    fn refresh_folders(&self) -> Task<Message> {
        let db = Arc::clone(&self.db);
        Task::perform(
            async move { tokio::task::spawn_blocking(move || db.get_folders()).await? },
            |result| match result {
                Ok(folders) => Message::LibraryFoldersLoaded(folders),
                Err(error) => Message::LibraryError(error.to_string()),
            },
        )
    }

    fn page_top(&self, target_page: u16) -> f32 {
        self.viewer_page_rect_for_page(target_page)
            .map_or(Spacing::PAGE_GUTTER, |rect| rect.y)
    }

    fn jump_to_page(&mut self, page: u16) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };

        let page = page.min(doc.page_count().saturating_sub(1));
        if let Some(rect) = self.viewer_page_rect_for_page(page) {
            self.last_scroll_offset = self.scroll_offset;
            if matches!(self.viewer_scroll_mode, ViewerScrollMode::Horizontal) {
                self.horizontal_offset = rect.x;
                self.scroll_offset = 0.0;
            } else {
                self.scroll_offset = rect.y;
                if matches!(self.viewer_scroll_mode, ViewerScrollMode::Wrapped) {
                    self.horizontal_offset = 0.0;
                }
            }
        }
        self.clamp_scroll_offset();
        self.clamp_horizontal_offset();
        self.jump_dialog_open = false;
        self.page_input_editing = false;
        self.jump_input.clear();
        self.request_visible_pages()
    }

    fn scroll_to_page_rect(&mut self, page: u16, x_fraction: f32, y_fraction: f32) {
        let Some(rect) = self.viewer_page_rect_for_page(page) else {
            return;
        };
        let target_x = rect.x + rect.width * x_fraction - self.viewer_viewport_width * 0.25;
        let target_y = rect.y + rect.height * y_fraction - self.viewer_viewport_height * 0.25;

        if matches!(self.viewer_scroll_mode, ViewerScrollMode::Horizontal) {
            self.horizontal_offset = target_x.max(0.0);
            self.scroll_offset = 0.0;
        } else {
            self.scroll_offset = target_y.max(0.0);
            if matches!(self.viewer_scroll_mode, ViewerScrollMode::Wrapped) {
                self.horizontal_offset = 0.0;
            }
        }
    }

    fn max_horizontal_offset(&self) -> f32 {
        (self.content_width() - self.viewer_viewport_width.max(1.0)).max(0.0)
    }

    fn max_scroll_offset(&self) -> f32 {
        (self.content_height() - self.viewer_viewport_height.max(1.0)).max(0.0)
    }

    fn clamp_horizontal_offset(&mut self) {
        self.horizontal_offset = self
            .horizontal_offset
            .clamp(0.0, self.max_horizontal_offset());
    }

    fn clamp_scroll_offset(&mut self) {
        self.scroll_offset = self.scroll_offset.clamp(0.0, self.max_scroll_offset());
    }

    fn scroll_by(&mut self, delta: f32) -> Task<Message> {
        self.last_scroll_offset = self.scroll_offset;
        self.scroll_offset = (self.scroll_offset + delta).clamp(0.0, self.max_scroll_offset());
        self.request_visible_pages()
    }

    fn scroll_page_mode_by(&mut self, direction: i16) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };
        let current = i32::from(self.current_page());
        let page_count = i32::from(doc.page_count());
        let next = (current + i32::from(direction)).clamp(0, page_count.saturating_sub(1));
        self.jump_to_page(next as u16)
    }

    fn pan_horizontally_by(&mut self, delta: f32) {
        self.horizontal_offset =
            (self.horizontal_offset + delta).clamp(0.0, self.max_horizontal_offset());
    }

    fn set_viewer_scroll_mode(&mut self, mode: ViewerScrollMode) -> Task<Message> {
        if self.viewer_scroll_mode == mode {
            return Task::none();
        }
        let current_page = self.current_page();
        self.viewer_scroll_mode = mode;
        self.horizontal_offset = 0.0;
        self.scroll_offset = 0.0;
        let zoom_task = self.apply_active_dimension_zoom();
        let page_task = self.jump_to_page(current_page);
        Task::batch([zoom_task, page_task])
    }

    fn set_viewer_spread_mode(&mut self, mode: ViewerSpreadMode) -> Task<Message> {
        if self.viewer_spread_mode == mode {
            return Task::none();
        }
        let current_page = self.current_page();
        self.viewer_spread_mode = mode;
        self.horizontal_offset = 0.0;
        self.scroll_offset = 0.0;
        let zoom_task = self.apply_active_dimension_zoom();
        let page_task = self.jump_to_page(current_page);
        Task::batch([zoom_task, page_task])
    }

    fn zoom_to_width(
        &mut self,
        width: u16,
        cursor: Option<Point>,
        render_policy: ZoomRenderPolicy,
    ) -> Task<Message> {
        let previous_width = self.zoom_width;
        let new_width = width.clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH);

        if new_width == previous_width {
            return Task::none();
        }

        if matches!(render_policy, ZoomRenderPolicy::Debounced) {
            let preview_width_px = self.render_width_px();
            self.zoom_preview_width_px.get_or_insert(preview_width_px);
        } else {
            self.zoom_preview_width_px = None;
        }

        let anchor = cursor.map(|cursor| {
            let ratio = f32::from(new_width) / f32::from(previous_width);
            let old_x = self.horizontal_offset + cursor.x;
            let old_y = self.scroll_offset + cursor.y;
            ((old_x * ratio) - cursor.x, (old_y * ratio) - cursor.y)
        });

        self.zoom_width = new_width;
        if !self.zoom_editing {
            self.zoom_input = zoom_percent_label(new_width);
        }
        self.zoom_menu_open = false;
        self.zoom_generation = self.zoom_generation.wrapping_add(1);
        let generation = self.zoom_generation;

        if let Some((x, y)) = anchor {
            self.horizontal_offset = x.clamp(0.0, self.max_horizontal_offset());
            self.scroll_offset = y.clamp(0.0, self.max_scroll_offset());
        }

        self.clamp_horizontal_offset();

        match render_policy {
            ZoomRenderPolicy::Immediate => self.request_visible_pages(),
            ZoomRenderPolicy::Debounced => schedule_zoom_render(generation),
        }
    }

    fn rendered_page_for_draw(&self, key: TileKey) -> Option<&RenderedPageView> {
        selected_render_key(
            self.rendered_pages.keys(),
            key,
            self.zoom_preview_width_px,
            true,
        )
        .and_then(|key| self.rendered_pages.get(&key))
    }

    fn fallback_rendered_page_for_draw(&self, key: TileKey) -> Option<&RenderedPageView> {
        selected_render_key(
            self.rendered_pages.keys(),
            key,
            self.zoom_preview_width_px,
            false,
        )
        .and_then(|key| self.rendered_pages.get(&key))
    }

    fn page_fade_progress(&self, key: TileKey) -> Option<f32> {
        let started = self.page_fade_started.get(&key)?;
        let elapsed = Instant::now().saturating_duration_since(*started);
        Some((elapsed.as_secs_f32() / (VIEWER_PAGE_FADE_MS as f32 / 1000.0)).clamp(0.0, 1.0))
    }

    fn all_visible_pages_rendered_at_current_zoom(&self) -> bool {
        self.visible_page_range().all(|page| {
            self.rendered_pages.contains_key(&TileKey {
                page,
                width_px: self.render_width_px(),
            })
        })
    }

    fn title(&self) -> String {
        if self.mode == AppMode::Library {
            return String::from("PDF-Folio");
        }

        self.doc
            .as_ref()
            .and_then(|doc| doc.path().file_name())
            .and_then(|name| name.to_str())
            .map(|name| format!("{name} - PDF-Folio"))
            .unwrap_or_else(|| String::from("PDF-Folio"))
    }
}

fn viewer_spread_groups(page_count: u16, spread_mode: ViewerSpreadMode) -> Vec<Vec<u16>> {
    match spread_mode {
        ViewerSpreadMode::None => (0..page_count).map(|page| vec![page]).collect(),
        ViewerSpreadMode::Odd => {
            let mut groups = Vec::new();
            let mut page = 0;
            while page < page_count {
                let mut group = vec![page];
                if page + 1 < page_count {
                    group.push(page + 1);
                }
                groups.push(group);
                page = page.saturating_add(2);
            }
            groups
        }
        ViewerSpreadMode::Even => {
            let mut groups = Vec::new();
            if page_count > 0 {
                groups.push(vec![0]);
            }
            let mut page = 1;
            while page < page_count {
                let mut group = vec![page];
                if page + 1 < page_count {
                    group.push(page + 1);
                }
                groups.push(group);
                page = page.saturating_add(2);
            }
            groups
        }
    }
}

fn prefetch_page_order_for_range(
    visible: std::ops::Range<u16>,
    page_count: u16,
    scrolling_forward: bool,
) -> Vec<u16> {
    if page_count == 0 || visible.start >= page_count {
        return Vec::new();
    }

    let start = visible.start.min(page_count);
    let end = visible
        .end
        .min(page_count)
        .max(start.saturating_add(1).min(page_count));
    let mut pages = Vec::new();

    for page in start..end {
        push_unique_page(&mut pages, page, page_count);
    }

    if start > 0 {
        push_unique_page(&mut pages, start - 1, page_count);
    }
    push_unique_page(&mut pages, end, page_count);

    if scrolling_forward {
        push_unique_page(&mut pages, end.saturating_add(1), page_count);
        push_unique_page(&mut pages, end.saturating_add(2), page_count);
    } else {
        if start > 1 {
            push_unique_page(&mut pages, start - 2, page_count);
        }
        if start > 2 {
            push_unique_page(&mut pages, start - 3, page_count);
        }
    }

    pages
}

fn push_unique_page(pages: &mut Vec<u16>, page: u16, page_count: u16) {
    if page < page_count && !pages.contains(&page) {
        pages.push(page);
    }
}

fn selected_render_key<'a>(
    keys: impl Iterator<Item = &'a TileKey>,
    target: TileKey,
    preview_width_px: Option<u16>,
    include_exact: bool,
) -> Option<TileKey> {
    let keys = keys
        .filter(|candidate| candidate.page == target.page)
        .copied()
        .collect::<Vec<_>>();

    if include_exact && keys.iter().any(|candidate| *candidate == target) {
        return Some(target);
    }

    if let Some(width_px) = preview_width_px {
        let preview = TileKey { width_px, ..target };
        if preview != target && keys.iter().any(|candidate| *candidate == preview) {
            return Some(preview);
        }
    }

    keys.into_iter()
        .filter(|candidate| include_exact || *candidate != target)
        .min_by_key(|candidate| candidate.width_px.abs_diff(target.width_px))
}

fn viewer_group_width(app: &PDFolioApp, group: &[u16]) -> f32 {
    if group.is_empty() {
        return 0.0;
    }

    f32::from(app.zoom_width) * group.len() as f32
        + Spacing::PAGE_GAP * group.len().saturating_sub(1) as f32
}

fn viewer_group_height(app: &PDFolioApp, group: &[u16]) -> f32 {
    group
        .iter()
        .map(|&page| app.page_height(page))
        .fold(0.0, f32::max)
}

fn viewer_groups_max_width(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    groups
        .iter()
        .map(|group| viewer_group_width(app, group))
        .fold(0.0, f32::max)
        + Spacing::PAGE_GUTTER * 2.0
}

fn viewer_groups_max_height(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    groups
        .iter()
        .map(|group| viewer_group_height(app, group))
        .fold(0.0, f32::max)
}

fn viewer_groups_inline_width(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    if groups.is_empty() {
        return app.viewer_viewport_width.max(1.0);
    }

    let groups_width: f32 = groups
        .iter()
        .map(|group| viewer_group_width(app, group))
        .sum();
    groups_width
        + Spacing::PAGE_GAP * groups.len().saturating_sub(1) as f32
        + Spacing::PAGE_GUTTER * 2.0
}

fn rects_intersect(a: Rectangle, b: Rectangle) -> bool {
    a.x <= b.x + b.width && a.x + a.width >= b.x && a.y <= b.y + b.height && a.y + a.height >= b.y
}

/// Launches the PDF-Folio UI.
///
/// # Errors
///
/// Returns an error when startup state cannot be created.
pub fn run(initial_file: Option<PathBuf>) -> Result<()> {
    let startup_file = initial_file.clone();
    let app = PDFolioApp::with_initial_file(initial_file)?;

    tracing::info!(
        mode = ?app.mode,
        has_document = app.doc.is_some(),
        "Initialized PDF-Folio application state"
    );

    iced::application(
        move || {
            let open_task = startup_file
                .clone()
                .map(open_document_task)
                .unwrap_or_else(Task::none);
            let load_task = Task::batch([app.clone().refresh_library(), app.refresh_folders()]);
            let attribution_task = attribute_pending_metadata_task(Arc::clone(&app.db));
            (
                app.clone(),
                Task::batch([open_task, load_task, attribution_task]),
            )
        },
        update,
        view,
    )
    .title(PDFolioApp::title)
    .theme(|app: &PDFolioApp| match app.theme {
        AppTheme::Light => Theme::Light,
        AppTheme::Dark => Theme::Dark,
    })
    .font(IBM_PLEX_SANS_REGULAR)
    .font(IBM_PLEX_SANS_MEDIUM)
    .font(IBM_PLEX_SANS_SEMIBOLD)
    .font(IBM_PLEX_SANS_BOLD)
    .default_font(iced::Font::with_name(UI_FONT_FAMILY))
    .subscription(subscription)
    .scale_factor(|app| app.scale_factor)
    .window_size(initial_window_size())
    .centered()
    .run()?;

    Ok(())
}

fn initial_window_size() -> [f32; 2] {
    StyleBook::load()
        .unwrap_or_else(|_| StyleBook::bundled())
        .layout()
        .window_size()
}

fn update(app: &mut PDFolioApp, message: Message) -> Task<Message> {
    match message {
        Message::AppMenuOpened(menu) => {
            app.open_selection_menu = None;
            app.open_view_menu_flyout = None;
            app.open_app_menu = if app.open_app_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        }
        Message::AppMenuClosed => {
            app.open_app_menu = None;
            app.open_view_menu_flyout = None;
        }
        Message::ViewMenuFlyoutOpened(flyout) => {
            if app.open_app_menu == Some(AppMenu::View) {
                app.open_view_menu_flyout = Some(flyout);
            }
        }
        Message::AppMenuActionSelected(action) => {
            app.open_app_menu = None;
            app.open_view_menu_flyout = None;
            match action {
                AppMenuAction::SetViewerScrollMode(mode) => {
                    return app.set_viewer_scroll_mode(mode)
                }
                AppMenuAction::SetViewerSpreadMode(mode) => {
                    return app.set_viewer_spread_mode(mode)
                }
                _ => {}
            }
            if let Some(message) = app_menu_action_message(app, action) {
                return Task::done(message);
            }
        }
        Message::SelectionMenuOpened(menu) => {
            app.open_app_menu = None;
            app.open_view_menu_flyout = None;
            app.open_selection_menu = if app.open_selection_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        }
        Message::SelectionMenuClosed => {
            app.open_selection_menu = None;
        }
        Message::OpenFileDialog => return open_file_dialog_task(),
        Message::FileDialogCanceled => {}
        Message::FileSelected(path) => {
            app.pending_document_open = true;
            return open_document_task(path);
        }
        Message::DocumentOpened(doc) => return app.open_document(doc),
        Message::LibraryDocumentOpened { entry_id, doc } => {
            return app.open_library_document(entry_id, doc);
        }
        Message::BackToLibrary => return app.return_to_library(),
        Message::BackToViewer => return app.return_to_viewer(),
        Message::DocumentError(error) => {
            app.pending_document_open = false;
            if !app.dismissed_document_errors.contains(&error) {
                app.document_error = Some(error);
            }
            app.pending_renders.clear();
            app.page_fade_started.clear();
        }
        Message::DismissDocumentError => {
            if let Some(error) = app.document_error.take() {
                app.dismissed_document_errors.insert(error);
            }
            app.document_error = None;
            return app.request_visible_pages();
        }
        Message::PageRendered {
            key,
            data,
            width,
            height,
            generation,
        } => {
            if app.pending_renders.get(&key) == Some(&generation) {
                app.pending_renders.remove(&key);
            }
            if generation.is_some_and(|generation| generation != app.zoom_generation) {
                return Task::none();
            }

            let had_fallback = generation.is_some()
                && key.width_px == app.render_width_px()
                && app.fallback_rendered_page_for_draw(key).is_some();
            app.cache.insert(key, data.clone());
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.rendered_pages.insert(
                key,
                RenderedPageView {
                    width,
                    height,
                    handle,
                },
            );
            if had_fallback {
                app.page_fade_started.insert(key, Instant::now());
            }

            if key.width_px == app.render_width_px()
                && app.all_visible_pages_rendered_at_current_zoom()
            {
                app.zoom_preview_width_px = None;
            }
        }
        Message::ThemeToggled => {
            app.theme = app.theme.toggled();
        }
        Message::ReloadStyles => {
            return Task::perform(async { StyleBook::load() }, Message::StylesReloaded);
        }
        Message::StylesReloaded(result) => match result {
            Ok(style_book) => {
                app.style_book = style_book;
                app.style_load_error = None;
                app.library_status = Some(String::from("Styles reloaded."));
            }
            Err(error) => {
                tracing::warn!(%error, "Failed to reload PDF-Folio styles");
                app.style_load_error = Some(error.clone());
                app.library_status = Some(format!("Style reload failed: {error}"));
            }
        },
        Message::ToggleSidebar | Message::ToggleTocPanel => {
            app.toc_open = !app.toc_open;
            app.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer_viewport_height = app.estimated_viewer_viewport_height();
            return app.apply_active_dimension_zoom();
        }
        Message::ViewerSidebarTabSelected(tab) => {
            app.viewer_sidebar_tab = tab;
            return app.request_viewer_thumbnail_pages();
        }
        Message::ToggleViewMode => {
            app.compact_view_mode = !app.compact_view_mode;
            return save_library_preferences_task(app);
        }
        Message::LibrarySortChanged(sort_mode) => {
            app.library_sort_mode = sort_mode;
            app.library_scroll_offset = 0.0;
            app.library_drag = None;
            return Task::batch([save_library_preferences_task(app), app.refresh_library()]);
        }
        Message::LibraryGridZoomChanged(zoom) => {
            app.library_grid_zoom = zoom.clamp(LIBRARY_GRID_ZOOM_MIN, LIBRARY_GRID_ZOOM_MAX);
            app.library_scroll_offset = app
                .library_scroll_offset
                .min(app.max_library_scroll_offset());
            app.update_library_drag_target_from_cursor();
            return Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
            ]);
        }
        Message::LibraryMetadataDensityChanged(density) => {
            app.library_metadata_density = density;
            return save_library_preferences_task(app);
        }
        Message::LibraryLoaded(entries) => {
            app.library_entries = entries;
            app.library_error = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            app.sync_details_editor_to_selection();
            app.library_status = Some(format!("{} PDFs in library", app.library_entries.len()));
            if !app.search_query.trim().is_empty() {
                return Task::done(Message::SearchDebounced(app.search_query.clone()));
            }
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(app.library_scroll_offset),
            ]);
        }
        Message::LibraryFoldersLoaded(folders) => {
            app.library_folders = folders;
            if app.selected_folder.as_ref().is_some_and(|selected| {
                !app.library_folders
                    .iter()
                    .any(|folder| &folder.id == selected)
            }) {
                app.selected_folder = None;
                app.sync_folder_rename_input();
                return save_library_preferences_task(app);
            }
            app.sync_folder_rename_input();
        }
        Message::LibraryRefresh => return app.refresh_library(),
        Message::LibraryError(error) => {
            app.library_status = Some(String::from("Library operation failed."));
            if !app.dismissed_library_errors.contains(&error) {
                app.library_error = Some(error);
            }
            app.bulk_operation_progress = None;
            app.pending_thumbnails.clear();
        }
        Message::DismissLibraryError => {
            if let Some(error) = app.library_error.take() {
                app.dismissed_library_errors.insert(error);
            }
            return scroll_library_to_offset_task(app.library_scroll_offset);
        }
        Message::LibraryStatus(status) => {
            app.library_status = Some(status);
            app.library_error = None;
        }
        Message::ImportFolderDialog => return import_folder_dialog_task(),
        Message::ImportFolderSelected(path) => {
            app.library_status = Some(format!("Importing {}...", path.display()));
            let db = Arc::clone(&app.db);
            app.settings.watch_directories.push(path.clone());
            app.settings.watch_directories.sort();
            app.settings.watch_directories.dedup();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || import_folder_with_index(&db, &path))
                        .await?
                },
                |result| match result {
                    Ok(summary) => Message::ImportFinished(summary),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::ImportFinished(summary) => {
            app.library_status = Some(format!(
                "Imported {} PDFs{}",
                summary.entries.len(),
                if summary.errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({} skipped)", summary.errors.len())
                }
            ));
            return app.refresh_library();
        }
        Message::AuthorAttributionFinished => return app.refresh_library(),
        Message::OpenLibraryEntry(entry_id) => {
            if let Some(entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            {
                app.pending_document_open = true;
                return open_library_document_task(entry.id, entry.path);
            }
        }
        Message::LibraryEntryClicked(entry_id) => {
            if app.library_drag.is_some() {
                return Task::none();
            }
            app.select_library_entry(entry_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.last_library_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == &entry_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });

            app.last_library_click = Some((entry_id.clone(), now));

            if is_double_click {
                return Task::done(Message::OpenLibraryEntry(entry_id));
            }
        }
        Message::EntryCheckboxToggled(entry_id) => {
            app.toggle_library_entry_selection(entry_id);
        }
        Message::MasterCheckboxClicked => match app.master_checkbox_state() {
            MasterCheckboxState::All => app.clear_library_selection(),
            MasterCheckboxState::None | MasterCheckboxState::Partial => {
                app.select_all_visible_library_entries();
            }
        },
        Message::LibraryEntryHoverChanged(entry_id, hovered) => {
            app.set_library_card_hover(entry_id, hovered);
        }
        Message::AnimationFrame(now) => {
            app.tick_animations(now);
        }
        Message::BeginLibraryEntryDrag(entry_id) => {
            app.begin_library_drag(entry_id);
            return scroll_library_to_offset_task(app.library_scroll_offset);
        }
        Message::BeginFolderDrag(folder_id) => {
            app.begin_folder_drag(folder_id);
            return scroll_library_to_offset_task(app.library_scroll_offset);
        }
        Message::ClearLibrarySelection => {
            app.clear_library_selection();
        }
        Message::SelectAllVisibleLibraryEntries => {
            app.select_all_visible_library_entries();
        }
        Message::LibraryEntryDragMoved(position) => {
            app.update_library_drag_target(position);
        }
        Message::FolderDragMoved(position) => {
            app.update_folder_drag_target(position);
        }
        Message::FolderDropTargetChanged(folder_id) => {
            app.set_folder_drop_hover_target(folder_id, Instant::now());
        }
        Message::LibraryAutoScrollTick(tick) => {
            return app.auto_scroll_library_drag(tick);
        }
        Message::EndLibraryEntryDrag => {
            return app.finish_library_drag();
        }
        Message::EndFolderDrag => {
            return app.finish_folder_drag();
        }
        Message::ManualEntryOrderSaved => {
            app.library_status = Some(String::from("Manual PDF order saved."));
            return Task::batch([
                app.refresh_library(),
                scroll_library_to_offset_task(app.library_scroll_offset),
            ]);
        }
        Message::SearchQueryChanged(query) => {
            app.search_query = query;
            app.library_drag = None;
            app.search_generation = app.search_generation.wrapping_add(1);
            let query = app.search_query.clone();
            if query.trim().is_empty() {
                app.search_results = None;
                app.search_hit_pages.clear();
                return app.request_visible_thumbnails();
            }
            return schedule_search(query);
        }
        Message::SearchDebounced(query) => {
            if query == app.search_query {
                let db = Arc::clone(&app.db);
                let sort_mode = app.library_sort_mode;
                return Task::perform(search_library_task(db, query, sort_mode), |result| {
                    match result {
                        Ok((entries, hit_pages)) => Message::SearchResults { entries, hit_pages },
                        Err(error) => Message::LibraryError(error.to_string()),
                    }
                });
            }
        }
        Message::SearchResults { entries, hit_pages } => {
            app.search_results = Some(entries);
            app.search_hit_pages = hit_pages;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return app.request_visible_thumbnails();
        }
        Message::LibraryScrolled {
            offset_y,
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
        } => {
            app.library_scroll_offset = offset_y.max(0.0);
            app.library_viewport_x = viewport_x;
            app.library_viewport_y = viewport_y;
            app.library_viewport_width = viewport_width.max(1.0);
            app.library_viewport_height = viewport_height.max(1.0);
            app.update_library_drag_target_from_cursor();
            return app.request_visible_thumbnails();
        }
        Message::CollapseLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library_tag_sidebar_open = false;
            app.resizing_library_tag_sidebar = false;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            return app.request_visible_thumbnails();
        }
        Message::ExpandLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library_tag_sidebar_open = true;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            return app.request_visible_thumbnails();
        }
        Message::BeginTagSidebarResize => {
            app.resizing_library_tag_sidebar = true;
        }
        Message::TagSidebarResizeDragged(width) => {
            if app.resizing_library_tag_sidebar {
                app.library_tag_sidebar_width = width.clamp(
                    app.layout().library_sidebar_min_width,
                    app.layout().library_sidebar_max_width,
                );
                app.recalculate_library_viewport_width();
            }
        }
        Message::EndTagSidebarResize => {
            app.resizing_library_tag_sidebar = false;
            return save_library_preferences_task(app);
        }
        Message::LibrarySidebarTabChanged(tab) => {
            app.library_sidebar_tab = tab;
        }
        Message::ToggleLibraryTreeRoot => {
            app.library_tree_root_expanded = !app.library_tree_root_expanded;
            return save_library_preferences_task(app);
        }
        Message::ToggleLibraryTreeFolder(folder_id) => {
            if !app.collapsed_library_tree_folders.insert(folder_id.clone()) {
                app.collapsed_library_tree_folders.remove(&folder_id);
            }
            return save_library_preferences_task(app);
        }
        Message::LibraryWatchEvent(event) => {
            let db = Arc::clone(&app.db);
            app.library_status = Some(match &event {
                LibraryWatchEvent::PdfCreated(path) => format!("Importing {}...", path.display()),
                LibraryWatchEvent::PdfRemoved(path) => {
                    format!("Marking missing: {}", path.display())
                }
            });
            return Task::perform(
                async move { tokio::task::spawn_blocking(move || apply_watch_event(&db, event)).await? },
                |result| match result {
                    Ok(()) => Message::LibraryRefresh,
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::TagFilterChanged(tag) => {
            app.active_tag_filter = tag;
            app.library_drag = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return app.request_visible_thumbnails();
        }
        Message::ReadingFilterChanged(filter) => {
            app.active_reading_filter = filter;
            app.library_drag = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return app.request_visible_thumbnails();
        }
        Message::MissingFilterChanged(active) => {
            app.missing_filter_active = active;
            app.library_drag = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return app.request_visible_thumbnails();
        }
        Message::FolderSelected(folder_id) => {
            app.selected_folder = folder_id;
            app.sync_folder_rename_input();
            app.library_drag = None;
            app.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::ClearLibraryFilters => {
            app.search_query.clear();
            app.search_results = None;
            app.search_hit_pages.clear();
            app.active_tag_filter = None;
            app.active_reading_filter = None;
            app.missing_filter_active = false;
            app.selected_folder = None;
            app.library_drag = None;
            app.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::NewFolderNameChanged(value) => {
            app.new_folder_name = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::FolderRenameInputChanged(value) => {
            app.folder_rename_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::OpenCreateFolderDialog => {
            app.create_folder_dialog_open = true;
        }
        Message::CreateFolder => {
            let name = app.new_folder_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            let db = Arc::clone(&app.db);
            let parent_id = app.selected_folder.clone();
            app.library_status = Some(format!("Creating folder {name}..."));
            app.new_folder_name.clear();
            app.create_folder_dialog_open = false;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || db.create_folder(&name, parent_id.as_ref()))
                        .await?
                },
                |result| match result {
                    Ok(folder_id) => Message::FolderCreated(folder_id),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::RenameSelectedFolder => {
            let Some(folder_id) = app.selected_folder.clone() else {
                return Task::none();
            };
            let name = app.folder_rename_input.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            app.library_status = Some(format!("Renaming folder to {name}..."));
            return rename_folder_task(Arc::clone(&app.db), folder_id, name);
        }
        Message::MoveSelectedFolderToRoot => {
            let Some(folder_id) = app.selected_folder.clone() else {
                return Task::none();
            };
            app.library_status = Some(String::from("Moving folder to library root..."));
            return move_folder_task(Arc::clone(&app.db), folder_id, None);
        }
        Message::MoveSelectedFolderUp => {
            let Some(folder) = app.selected_folder().cloned() else {
                return Task::none();
            };
            let Some(parent_id) = folder.parent_id.as_ref() else {
                return Task::none();
            };
            let grandparent_id = app
                .library_folders
                .iter()
                .find(|candidate| &candidate.id == parent_id)
                .and_then(|parent| parent.parent_id.clone());
            app.library_status = Some(String::from("Moving folder up one level..."));
            return move_folder_task(Arc::clone(&app.db), folder.id, grandparent_id);
        }
        Message::MoveSelectedFolderEarlier => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(-1) else {
                return Task::none();
            };
            app.library_status = Some(String::from("Moving folder earlier..."));
            return persist_manual_folder_order_task(Arc::clone(&app.db), parent_id, folder_ids);
        }
        Message::MoveSelectedFolderLater => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(1) else {
                return Task::none();
            };
            app.library_status = Some(String::from("Moving folder later..."));
            return persist_manual_folder_order_task(Arc::clone(&app.db), parent_id, folder_ids);
        }
        Message::RequestDeleteSelectedFolder => {
            if let Some(folder_id) = app.selected_folder.clone() {
                app.pending_confirmation = Some(ConfirmationAction::DeleteFolder(folder_id));
            }
        }
        Message::DeleteFolder(folder_id) => {
            app.library_status = Some(String::from("Deleting folder..."));
            return delete_folder_task(Arc::clone(&app.db), folder_id);
        }
        Message::FolderUpdated => {
            app.library_status = Some(String::from("Folder updated."));
            return Task::batch([app.refresh_folders(), app.refresh_library()]);
        }
        Message::FolderCreated(folder_id) => {
            app.library_status = Some(String::from("Folder created."));
            app.selected_folder = Some(folder_id);
            app.sync_folder_rename_input();
            app.library_scroll_offset = 0.0;
            return Task::batch([
                save_library_preferences_task(app),
                app.refresh_folders(),
                app.refresh_library(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::StartTagEntry(entry_id) => {
            app.tag_entry_id = Some(entry_id);
            app.tag_input.clear();
        }
        Message::TagInputChanged(value) => {
            app.tag_input = value;
        }
        Message::SubmitTag => {
            if let Some(entry_id) = app.tag_entry_id.clone() {
                let tag = app.tag_input.trim().to_owned();
                app.tag_entry_id = None;
                app.tag_input.clear();
                if !tag.is_empty() {
                    let db = Arc::clone(&app.db);
                    return Task::perform(
                        async move {
                            let saved_entry_id = entry_id.clone();
                            let saved_tag = tag.clone();
                            tokio::task::spawn_blocking(move || {
                                db.add_tag(&saved_entry_id, &saved_tag)
                            })
                            .await??;
                            Ok::<_, anyhow::Error>((entry_id, tag))
                        },
                        |result| match result {
                            Ok((id, tag)) => Message::EntryTagged { id, tag },
                            Err(error) => Message::LibraryError(error.to_string()),
                        },
                    );
                }
            }
        }
        Message::EntryTagged { .. } | Message::EntryUntagged { .. } | Message::EntryDeleted(_) => {
            return app.refresh_library();
        }
        Message::RequestConfirmation(action) => {
            app.pending_confirmation = Some(action);
        }
        Message::CancelConfirmation => {
            app.pending_confirmation = None;
        }
        Message::ConfirmPendingAction => {
            let Some(action) = app.pending_confirmation.take() else {
                return Task::none();
            };
            return Task::done(match action {
                ConfirmationAction::BulkResetDisplayMetadata => Message::BulkResetDisplayMetadata,
                ConfirmationAction::BulkDeleteFromLibrary => Message::BulkDeleteFromLibrary,
                ConfirmationAction::ResetDetailsMetadata(entry_id) => {
                    Message::ResetDetailsMetadata(entry_id)
                }
                ConfirmationAction::DeleteFolder(folder_id) => Message::DeleteFolder(folder_id),
            });
        }
        Message::SelectionToolbarActionSelected(action) => {
            app.open_selection_menu = None;
            return Task::done(match action {
                SelectionToolbarAction::AddTag => Message::BulkAddTag,
                SelectionToolbarAction::RemoveTag => Message::BulkRemoveTag,
                SelectionToolbarAction::AddToFolder => Message::BulkAddToCurrentFolder,
                SelectionToolbarAction::RemoveFromFolder => Message::BulkRemoveFromCurrentFolder,
                SelectionToolbarAction::SaveDetails => Message::SaveDetailsMetadata,
                SelectionToolbarAction::ResetDetails => {
                    let Some(entry_id) = app.details_entry_id.clone() else {
                        return Task::none();
                    };
                    Message::RequestConfirmation(ConfirmationAction::ResetDetailsMetadata(entry_id))
                }
                SelectionToolbarAction::SortTitles => Message::BulkApplyTitleSortCleanup,
                SelectionToolbarAction::RefreshMetadata => Message::BulkRefreshPdfMetadata,
                SelectionToolbarAction::ResetMetadata => {
                    Message::RequestConfirmation(ConfirmationAction::BulkResetDisplayMetadata)
                }
                SelectionToolbarAction::RebuildThumbnails => Message::BulkRebuildThumbnails,
                SelectionToolbarAction::Reindex => Message::BulkReindex,
                SelectionToolbarAction::DeleteMetadata => {
                    Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)
                }
            });
        }
        Message::DetailsTitleChanged(value) => {
            app.details_title_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
        }
        Message::DetailsAuthorChanged(value) => {
            app.details_author_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
        }
        Message::SaveDetailsMetadata => {
            let Some(entry_id) = app.details_entry_id.clone() else {
                return Task::none();
            };
            let Some(mut entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            entry.display_title = clean_metadata_input(&app.details_title_input);
            entry.display_author = clean_metadata_input(&app.details_author_input);
            entry.metadata_locked = true;
            app.library_status = Some(format!("Saving metadata for {}...", entry_title(&entry)));
            return edit_metadata_task(
                Arc::clone(&app.db),
                entry,
                app.details_title_input.clone(),
                app.details_author_input.clone(),
            );
        }
        Message::ResetDetailsMetadata(entry_id) => {
            let Some(mut entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            entry.display_title = None;
            entry.display_author = None;
            entry.metadata_locked = false;
            app.library_status = Some(format!("Resetting metadata for {}...", entry_title(&entry)));
            return reset_metadata_task(Arc::clone(&app.db), entry);
        }
        Message::RevealEntryInFileManager(entry_id) => {
            let Some(entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library_status = Some(format!("Revealing {}...", entry_title(&entry)));
            return open_file_manager_task(entry.path, true);
        }
        Message::OpenEntryContainingFolder(entry_id) => {
            let Some(entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library_status = Some(format!("Opening folder for {}...", entry_title(&entry)));
            return open_file_manager_task(entry.path, false);
        }
        Message::RelinkMissingEntry(entry_id) => {
            return relink_file_dialog_task(entry_id);
        }
        Message::RelinkFileSelected { entry_id, path } => {
            let Some(entry) = app
                .library_entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library_status = Some(format!("Relinking {}...", entry_title(&entry)));
            return relink_entry_task(Arc::clone(&app.db), entry_id, path);
        }
        Message::RelinkFinished { entry_id: _, path } => {
            app.library_status = Some(format!("Relinked PDF to {}.", path.display()));
            app.library_error = None;
            app.pending_thumbnails.clear();
            return Task::batch([app.refresh_library(), app.request_visible_thumbnails()]);
        }
        Message::MetadataEditFinished {
            entry_id: _,
            label,
            errors,
        } => {
            app.library_status = Some(if errors.is_empty() {
                label
            } else {
                format!("{label}; {} indexing errors.", errors.len())
            });
            app.details_entry_id = None;
            return app.refresh_library();
        }
        Message::BulkTagInputChanged(value) => {
            app.bulk_tag_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::BulkAddTag => {
            let tag = app.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.selected_library_entries.is_empty() {
                return Task::none();
            }
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tag to", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                move |db, entry_id| db.add_tag(entry_id, &tag),
            );
        }
        Message::BulkRemoveTag => {
            let tag = app.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.selected_library_entries.is_empty() {
                return Task::none();
            }
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Removing tag from", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Untagged"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            );
        }
        Message::BulkAddToCurrentFolder => {
            let Some(folder_id) = app.selected_folder.clone() else {
                app.library_status = Some(String::from("Open a folder before adding PDFs to it."));
                return Task::none();
            };
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Adding to folder", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Added to folder"),
                move |db, entry_id| db.add_entry_to_folder(entry_id, &folder_id),
            );
        }
        Message::BulkRemoveFromCurrentFolder => {
            let Some(folder_id) = app.selected_folder.clone() else {
                app.library_status =
                    Some(String::from("Open a folder before removing PDFs from it."));
                return Task::none();
            };
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Removing from folder", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Removed from folder"),
                move |db, entry_id| db.remove_entry_from_folder(entry_id, &folder_id),
            );
        }
        Message::BulkResetDisplayMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Resetting metadata for", entries.len());
            return bulk_reset_metadata_task(Arc::clone(&app.db), entries);
        }
        Message::BulkApplyTitleSortCleanup => {
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Cleaning title sort keys for", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Cleaned title sort for"),
                |db, entry_id| db.apply_title_sort_cleanup(entry_id),
            );
        }
        Message::BulkRefreshPdfMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Refreshing metadata for", entries.len());
            return bulk_refresh_metadata_task(Arc::clone(&app.db), entries);
        }
        Message::BulkRebuildThumbnails => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            for entry in &entries {
                app.thumbnails.retain(|key, _| key.entry_id != entry.id);
                app.pending_thumbnails
                    .retain(|key| key.entry_id != entry.id);
            }
            app.start_bulk_operation_progress("Rebuilding thumbnails for", entries.len());
            return bulk_thumbnail_task(entries);
        }
        Message::BulkReindex => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Reindexing", entries.len());
            return bulk_reindex_task(entries);
        }
        Message::BulkDeleteFromLibrary => {
            let entry_ids = app
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Deleting from library metadata", entry_ids.len());
            return bulk_delete_metadata_task(Arc::clone(&app.db), entry_ids);
        }
        Message::BulkOperationFinished {
            label,
            updated,
            errors,
        } => {
            app.bulk_operation_progress = None;
            app.library_status = Some(if errors.is_empty() {
                app.library_error = None;
                format!("{label} {updated} PDFs.")
            } else {
                app.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.pending_thumbnails.clear();
            return Task::batch([app.refresh_library(), app.request_visible_thumbnails()]);
        }
        Message::FolderAssignmentFinished {
            folder_id,
            label,
            updated,
            errors,
        } => {
            app.library_status = Some(if errors.is_empty() {
                app.library_error = None;
                if updated > 0 {
                    app.start_folder_drop_flash(folder_id, Instant::now());
                }
                format!("{label} {updated} PDFs.")
            } else {
                app.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.pending_thumbnails.clear();
            return Task::batch([app.refresh_library(), app.request_visible_thumbnails()]);
        }
        Message::ThumbnailReady {
            entry_id,
            size,
            data,
            width,
            height,
        } => {
            let key = ThumbnailCacheKey {
                entry_id: entry_id.clone(),
                size,
            };
            app.pending_thumbnails.remove(&key);
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.thumbnails.insert(
                key,
                ThumbnailView {
                    width,
                    height,
                    handle,
                },
            );
        }
        Message::ProgressUpdated { entry_id, page } => {
            let db = Arc::clone(&app.db);
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || db.update_last_page(&entry_id, page))
                        .await??;
                    Ok::<_, anyhow::Error>(())
                },
                |result| match result {
                    Ok(()) => Message::ProgressSaved,
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::ProgressSaved | Message::LibraryPreferencesSaved => {}
        Message::OpenJumpDialog => {
            app.page_input_editing = false;
            app.jump_dialog_open = true;
            app.jump_input = app
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
        }
        Message::OpenViewerFind => {
            return app.open_viewer_find();
        }
        Message::CloseViewerFind => {
            app.viewer_find.open = false;
        }
        Message::ViewerFindQueryChanged(query) => {
            return app.set_viewer_find_query(query);
        }
        Message::ViewerFindPrevious => {
            app.viewer_find.select_previous();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::ViewerFindNext => {
            app.viewer_find.select_next();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::ViewerFindHighlightAllToggled(value) => {
            app.viewer_find.highlight_all = value;
        }
        Message::ViewerFindMatchCaseToggled(value) => {
            app.viewer_find.match_case = value;
            app.refresh_viewer_find_matches();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::ViewerFindMatchDiacriticsToggled(value) => {
            app.viewer_find.match_diacritics = value;
            app.refresh_viewer_find_matches();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::CloseOverlay => {
            if app.jump_dialog_open {
                app.jump_dialog_open = false;
                app.jump_input.clear();
            } else if app.page_input_editing {
                app.page_input_editing = false;
                app.jump_input.clear();
            } else if app.viewer_find.open {
                app.viewer_find.open = false;
            } else if app.create_folder_dialog_open {
                app.create_folder_dialog_open = false;
            } else if app.pending_confirmation.is_some() {
                app.pending_confirmation = None;
            } else if app.open_app_menu.is_some() {
                app.open_app_menu = None;
                app.open_view_menu_flyout = None;
            } else if app.open_selection_menu.is_some() {
                app.open_selection_menu = None;
            } else {
                app.toc_open = false;
            }
        }
        Message::JumpInputChanged(value) => {
            app.jump_input = value.chars().filter(char::is_ascii_digit).take(5).collect();
        }
        Message::StartPageInputEdit => {
            app.jump_dialog_open = false;
            app.page_input_editing = true;
            app.jump_input = app
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
            return operation::focus(Id::new(PAGE_INPUT_ID));
        }
        Message::SubmitJump => {
            if let Ok(page) = app.jump_input.parse::<u16>() {
                return app.jump_to_page(page.saturating_sub(1));
            }
            app.page_input_editing = false;
            app.jump_input.clear();
        }
        Message::JumpToPage(page) => return app.jump_to_page(page),
        Message::PreviousPage => {
            let page = app.current_page().saturating_sub(1);
            return app.jump_to_page(page);
        }
        Message::NextPage => {
            if let Some(doc) = &app.doc {
                let page = app
                    .current_page()
                    .saturating_add(1)
                    .min(doc.page_count().saturating_sub(1));
                return app.jump_to_page(page);
            }
        }
        Message::ToggleOutlineNode(path) => {
            if !app.expanded_outline_paths.insert(path.clone()) {
                app.expanded_outline_paths.remove(&path);
            }
        }
        Message::ViewerTextLayerLoaded { page, layer } => {
            app.pending_text_layers.remove(&page);
            app.viewer_text_layers.insert(page, layer);
            let mut tasks = Vec::new();
            if app.viewer_find.open {
                let previous_match = app.viewer_find.selected_match();
                app.refresh_viewer_find_matches();
                if !app.viewer_find.query.is_empty()
                    && previous_match != app.viewer_find.selected_match()
                    && app.viewer_find.selected_match().is_some()
                {
                    tasks.push(app.scroll_to_selected_viewer_find_match());
                }
            }
            if app.viewer_copy_pending && app.selected_text_layers_ready() {
                tasks.push(app.copy_selected_viewer_text());
            }
            if !tasks.is_empty() {
                return Task::batch(tasks);
            }
        }
        Message::ViewerTextLayerError { page, error } => {
            app.pending_text_layers.remove(&page);
            app.document_error = Some(error);
        }
        Message::ViewerTextSelectionStarted { page, char_index } => {
            app.start_viewer_text_selection(page, char_index);
        }
        Message::ViewerTextSelectionChanged { page, char_index } => {
            app.update_viewer_text_selection(page, char_index);
        }
        Message::ViewerTextSelectionEnded => {
            app.finish_viewer_text_selection();
        }
        Message::ViewerCanvasClicked => {
            app.clear_viewer_text_selection();
        }
        Message::ClearViewerTextSelection => {
            app.clear_viewer_text_selection();
        }
        Message::CopyViewerTextSelection => {
            return app.copy_selected_viewer_text();
        }
        Message::ScrollChanged(offset) => {
            app.last_scroll_offset = app.scroll_offset;
            app.scroll_offset = offset;
            app.clamp_scroll_offset();
            let render_task = app.request_visible_pages();
            let progress_task = app
                .current_entry_id
                .clone()
                .map_or_else(Task::none, |entry_id| {
                    Task::done(Message::ProgressUpdated {
                        entry_id,
                        page: app.current_page(),
                    })
                });
            return Task::batch([render_task, progress_task]);
        }
        Message::ViewportChanged {
            scroll_offset,
            width,
            height,
        } => {
            app.last_scroll_offset = app.scroll_offset;
            app.scroll_offset = scroll_offset;
            app.viewer_viewport_width = width.max(1.0);
            app.viewer_viewport_height = height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();
            return Task::batch([
                app.apply_active_dimension_zoom(),
                app.request_visible_pages(),
            ]);
        }
        Message::WindowResized { width, height } => {
            app.viewport_width = width.max(1.0);
            app.viewport_height = height.max(1.0);
            app.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer_viewport_height = app.estimated_viewer_viewport_height();
            if app.mode == AppMode::Library {
                app.recalculate_library_viewport_width();
                app.library_viewport_height =
                    (app.viewport_height - app_menu_bar_height(app) - Spacing::LG * 2.0).max(1.0);
                return app.request_visible_thumbnails();
            }
            return app.apply_active_dimension_zoom();
        }
        Message::ViewportWheelScrolled {
            delta_x,
            delta_y,
            cursor,
            viewport_width,
            viewport_height,
        } => {
            app.viewer_viewport_width = viewport_width.max(1.0);
            app.viewer_viewport_height = viewport_height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();

            if app.modifiers.control() {
                app.active_zoom_preset = None;
                let direction = if delta_y.abs() >= delta_x.abs() {
                    delta_y
                } else {
                    -delta_x
                };
                let step = if direction > 0.0 { 100 } else { -100 };
                let width = (i32::from(app.zoom_width) + step)
                    .clamp(i32::from(MIN_ZOOM_WIDTH), i32::from(MAX_ZOOM_WIDTH))
                    as u16;
                return app.zoom_to_width(width, Some(cursor), ZoomRenderPolicy::Debounced);
            }

            if app.viewer_scroll_mode == ViewerScrollMode::Page {
                let direction = if delta_y < 0.0 || delta_x > 0.0 {
                    1
                } else {
                    -1
                };
                return app.scroll_page_mode_by(direction);
            }

            if app.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                let delta = if delta_x != 0.0 { delta_x } else { delta_y };
                app.horizontal_offset =
                    (app.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
                return app.request_visible_pages();
            }

            if app.modifiers.shift() || delta_x != 0.0 {
                let delta = if delta_x != 0.0 { delta_x } else { delta_y };
                app.horizontal_offset =
                    (app.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
            } else {
                app.last_scroll_offset = app.scroll_offset;
                app.scroll_offset =
                    (app.scroll_offset - delta_y).clamp(0.0, app.max_scroll_offset());
                return app.request_visible_pages();
            }
        }
        Message::ModifiersChanged(modifiers) => {
            app.modifiers = modifiers;
        }
        Message::ZoomRenderSettled(generation) => {
            if generation == app.zoom_generation {
                return app.request_visible_pages();
            }
        }
        Message::ZoomIn => {
            app.active_zoom_preset = None;
            return app.zoom_to_width(
                app.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ZoomOut => {
            app.active_zoom_preset = None;
            return app.zoom_to_width(
                app.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::In) => {
            app.active_zoom_preset = None;
            return app.zoom_to_width(
                app.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::Out) => {
            app.active_zoom_preset = None;
            return app.zoom_to_width(
                app.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::Reset) => {
            app.active_zoom_preset = Some(ZoomPreset::Automatic);
            let width = ZoomPreset::Automatic.width_for(app);
            return app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        }
        Message::ShortcutPressed(Shortcut::ToggleTheme) => {
            app.theme = app.theme.toggled();
        }
        Message::ShortcutPressed(Shortcut::ReloadStyles) => {
            return Task::done(Message::ReloadStyles);
        }
        Message::ShortcutPressed(Shortcut::PageDown) => {
            if app.viewer_scroll_mode == ViewerScrollMode::Page {
                return app.scroll_page_mode_by(1);
            }
            return app.scroll_by(app.viewer_viewport_height * 0.86);
        }
        Message::ShortcutPressed(Shortcut::PageUp) => {
            if app.viewer_scroll_mode == ViewerScrollMode::Page {
                return app.scroll_page_mode_by(-1);
            }
            return app.scroll_by(-(app.viewer_viewport_height * 0.86));
        }
        Message::ShortcutPressed(Shortcut::FineScroll(delta)) => {
            if app.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                app.pan_horizontally_by(f32::from(delta));
                return Task::none();
            }
            return app.scroll_by(f32::from(delta));
        }
        Message::ShortcutPressed(Shortcut::HorizontalPan(delta)) => {
            app.pan_horizontally_by(f32::from(delta));
        }
        Message::ShortcutPressed(Shortcut::SelectAll) => {
            if app.mode == AppMode::Library {
                app.select_all_visible_library_entries();
            }
        }
        Message::ShortcutPressed(Shortcut::OpenSelected) => {
            if app.mode == AppMode::Library && app.selected_library_entries.len() == 1 {
                if let Some(entry_id) = app.selected_library_entries.iter().next().cloned() {
                    return Task::done(Message::OpenLibraryEntry(entry_id));
                }
            }
        }
        Message::ShortcutPressed(Shortcut::FocusSearch) => {
            if app.mode == AppMode::Library {
                return operation::focus(Id::new(LIBRARY_SEARCH_INPUT_ID));
            }
            if app.mode == AppMode::Viewer {
                return app.open_viewer_find();
            }
        }
        Message::ShortcutPressed(Shortcut::RenameSelected) => {
            if app.mode == AppMode::Library && app.selected_library_entries.len() == 1 {
                return operation::focus(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID));
            }
            if app.mode == AppMode::Library && app.selected_folder.is_some() {
                return operation::focus(Id::new(LIBRARY_FOLDER_RENAME_INPUT_ID));
            }
        }
        Message::ShortcutPressed(Shortcut::DeleteSelected) => {
            if app.mode == AppMode::Library && !app.selected_library_entries.is_empty() {
                return Task::done(Message::RequestConfirmation(
                    ConfirmationAction::BulkDeleteFromLibrary,
                ));
            }
            if app.mode == AppMode::Library {
                if let Some(folder_id) = app.selected_folder.clone() {
                    return Task::done(Message::RequestConfirmation(
                        ConfirmationAction::DeleteFolder(folder_id),
                    ));
                }
            }
        }
        Message::ShortcutPressed(Shortcut::Jump) => {
            app.page_input_editing = false;
            app.jump_dialog_open = true;
            app.jump_input = (u32::from(app.current_page()) + 1).to_string();
        }
        Message::ShortcutPressed(Shortcut::Copy) => {
            if app.mode == AppMode::Viewer {
                return app.copy_selected_viewer_text();
            }
        }
        Message::ShortcutPressed(Shortcut::Escape) => {
            if app.pending_confirmation.is_some() {
                app.pending_confirmation = None;
            } else if app.open_app_menu.is_some() {
                app.open_app_menu = None;
                app.open_view_menu_flyout = None;
            } else if app.open_selection_menu.is_some() {
                app.open_selection_menu = None;
            } else if app.zoom_menu_open {
                app.zoom_menu_open = false;
            } else if app.zoom_editing {
                app.zoom_editing = false;
                app.zoom_input = zoom_percent_label(app.zoom_width);
            } else if app.page_input_editing {
                app.page_input_editing = false;
                app.jump_input.clear();
            } else if app.mode == AppMode::Viewer && app.viewer_find.open {
                app.viewer_find.open = false;
            } else if app.mode == AppMode::Viewer && app.viewer_text_selection.is_some() {
                app.clear_viewer_text_selection();
            } else if app.mode == AppMode::Library && !app.selected_library_entries.is_empty() {
                app.clear_library_selection();
            } else if app.jump_dialog_open {
                app.jump_dialog_open = false;
                app.jump_input.clear();
            } else if app.create_folder_dialog_open {
                app.create_folder_dialog_open = false;
            } else {
                app.toc_open = false;
            }
        }
        Message::ZoomSet(width) => {
            app.active_zoom_preset = None;
            return app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        }
        Message::StartZoomInputEdit => {
            app.zoom_editing = true;
            app.zoom_menu_open = false;
            app.zoom_input = zoom_percent_label(app.zoom_width);
            return operation::focus(Id::new(ZOOM_INPUT_ID));
        }
        Message::ZoomInputChanged(value) => {
            app.zoom_input = value;
        }
        Message::SubmitZoomInput => {
            let width = width_from_percent_input(&app.zoom_input);
            app.zoom_editing = false;
            if let Some(width) = width {
                app.active_zoom_preset = None;
                return app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
            }
            app.zoom_input = zoom_percent_label(app.zoom_width);
        }
        Message::ToggleZoomMenu => {
            app.zoom_menu_open = !app.zoom_menu_open;
            app.zoom_editing = false;
            app.zoom_input = zoom_percent_label(app.zoom_width);
        }
        Message::CloseZoomMenu => {
            app.zoom_menu_open = false;
        }
        Message::ZoomPresetSelected(preset) => {
            app.zoom_menu_open = false;
            app.zoom_editing = false;
            app.active_zoom_preset = Some(preset);
            let width = preset.width_for(app);
            app.zoom_input = zoom_percent_label(width);
            let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
            if matches!(preset, ZoomPreset::PageWidth) {
                app.horizontal_offset = 0.0;
            }
            return task;
        }
        _ => {}
    }

    Task::none()
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
        sort_mode: app.library_sort_mode,
        layout_mode: if app.compact_view_mode {
            LibraryLayoutMode::List
        } else {
            LibraryLayoutMode::Grid
        },
        selected_folder: app.selected_folder.clone(),
        sidebar_width: app.library_tag_sidebar_width,
        grid_zoom: app.library_grid_zoom(),
        visible_metadata_fields: app.library_metadata_density.visible_fields(),
        library_tree_root_expanded: app.library_tree_root_expanded,
        collapsed_folder_ids: app.collapsed_library_tree_folders.iter().cloned().collect(),
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
    let query = app.search_query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }

    app.search_hit_pages
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
        .font(display_font(FontWeight::MEDIUM))
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

fn file_tree_label(label: &str, width: f32) -> String {
    truncate_for_width_with_font(label, width, 0.0, FILE_TREE_LABEL_SIZE)
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
