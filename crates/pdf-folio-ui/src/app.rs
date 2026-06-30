//! Top-level application state and launch entrypoint.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use iced::futures::SinkExt;
use iced::mouse;
use iced::stream;
use iced::time;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, canvas, column, container, image, mouse_area, pick_list, pin, row, scrollable, slider,
    stack, text, text_input, tooltip, Svg,
};
use iced::widget::{operation, Id};
use iced::{
    animation, event, font, keyboard, Animation, Color, ContentFit, Element, Event, Font, Length,
    Point, Rectangle, Renderer, Size,
};
use iced::{Subscription, Task, Theme};
use notify::{EventKind, RecursiveMode, Watcher};
use pdf_folio_core::{Annotation, OutlineNode, PdfDoc, RenderedPage, TileCache, TileKey};
use pdf_folio_library::{
    hash_file, scan_pdf_files, thumbnail_path, Db, EntryId, Folder, FolderId, ImportSummary,
    ImportedEntry, IndexDocument, LibraryEntry, LibraryLayoutMode, LibraryPreferences,
    LibrarySortMode, LibraryWatchEvent, LibraryWatcher, NewLibraryEntry, SearchIndex,
};

use crate::messages::{
    AppMenu, AppMenuAction, ConfirmationAction, LibrarySidebarTab, Message, SelectionMenu,
    SelectionToolbarAction, Shortcut,
};
use crate::style::{
    container_style, display_font, empty_state, error_banner, master_checkbox,
    menu_style_for_class, mix_color, pick_list_style, progress_bar, scrollable_style,
    search_input_with_class, section_heading, selection_checkbox, side_border,
    side_border_for_class, slider_style, tag_pill, text_input_style, toc_entry, toolbar_button,
    ui_font, viewer_primitives, Class, ComponentState, FontSize, FontWeight, LabelSection,
    MasterCheckboxState, Spacing, StyleBook, ThemeTokens, UI_FONT_FAMILY,
};
use crate::theme::AppTheme;

const CHEVRON_LEFT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"##;
const CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"##;
const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;
const GRID_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/></svg>"##;
const LIST_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>"##;
const GEIST_MONO_PROPO_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/GeistMonoNerdFontPropo-Regular.otf");
const GEIST_MONO_PROPO_MEDIUM: &[u8] =
    include_bytes!("../assets/fonts/GeistMonoNerdFontPropo-Medium.otf");
const GEIST_MONO_PROPO_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/GeistMonoNerdFontPropo-SemiBold.otf");
const GEIST_MONO_PROPO_BOLD: &[u8] =
    include_bytes!("../assets/fonts/GeistMonoNerdFontPropo-Bold.otf");
const FILE_TREE_FONT_FAMILY: &str = "GeistMono Nerd Font Propo";
const FILE_TREE_LABEL_SIZE: u32 = FontSize::MD;
const FILE_TREE_ROW_HEIGHT: f32 = 26.0;
const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
const LIBRARY_SEARCH_INPUT_ID: &str = "library-search-input";
const LIBRARY_FOLDER_RENAME_INPUT_ID: &str = "library-folder-rename-input";
const LIBRARY_DETAILS_TITLE_INPUT_ID: &str = "library-details-title-input";
const LIBRARY_DRAG_AUTOSCROLL_TICK_MS: u64 = 16;
const LIBRARY_DRAG_AUTOSCROLL_EDGE_BAND: f32 = 96.0;
const LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED: f32 = 980.0;
const LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED: f32 = 80.0;
const LIBRARY_DRAG_AUTOSCROLL_MAX_DT: f32 = 1.0 / 20.0;
const LIBRARY_DRAG_ACTIVATION_DISTANCE: f32 = 6.0;
const LIBRARY_FOLDER_DROP_DWELL_MS: u64 = 500;
const LIBRARY_FOLDER_DROP_FLASH_MS: u64 = 900;
const LIBRARY_CARD_HOVER_TICK_MS: u64 = 16;
const LIBRARY_CARD_HOVER_DURATION_MS: u64 = 180;
const LIBRARY_CARD_HOVER_LIFT: f32 = 2.0;
const LIBRARY_ROW_HOVER_LIFT: f32 = 1.0;
const LIBRARY_GRID_ZOOM_MIN: f32 = 0.25;
const LIBRARY_GRID_ZOOM_MAX: f32 = 12.0;
const LIBRARY_GRID_ZOOM_STEP: f32 = 0.05;
const LIBRARY_GRID_ZOOM_DENSE_COLUMN_CAP: usize = 28;
const APP_MENU_LABELS: [AppMenu; 7] = [
    AppMenu::File,
    AppMenu::Edit,
    AppMenu::View,
    AppMenu::Document,
    AppMenu::Library,
    AppMenu::Tools,
    AppMenu::Help,
];
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
const LIBRARY_READING_FILTER_OPTIONS: [LibraryReadingFilter; 3] = [
    LibraryReadingFilter::Unread,
    LibraryReadingFilter::Reading,
    LibraryReadingFilter::Finished,
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

/// Density of metadata shown in library cards and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMetadataDensity {
    /// Show title and author with minimal supporting metadata.
    Minimal,
    /// Show common reading metadata.
    Standard,
    /// Show reading metadata plus file details.
    Detailed,
}

impl LibraryMetadataDensity {
    fn from_visible_fields(fields: &[String]) -> Self {
        let has_file_size = fields.iter().any(|field| field == "file_size");
        let has_page_count = fields.iter().any(|field| field == "page_count");
        if has_file_size {
            Self::Detailed
        } else if has_page_count {
            Self::Standard
        } else {
            Self::Minimal
        }
    }

    fn visible_fields(self) -> Vec<String> {
        match self {
            Self::Minimal => vec![String::from("author")],
            Self::Standard => vec![String::from("author"), String::from("page_count")],
            Self::Detailed => vec![
                String::from("author"),
                String::from("page_count"),
                String::from("file_size"),
            ],
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Standard => "Standard",
            Self::Detailed => "Detailed",
        }
    }
}

impl std::fmt::Display for LibraryMetadataDensity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Reading-progress filter applied to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryReadingFilter {
    /// Entries with no saved progress.
    Unread,
    /// Entries with saved progress before the final known page.
    Reading,
    /// Entries whose saved progress reaches the final known page.
    Finished,
}

impl LibraryReadingFilter {
    fn label(self) -> &'static str {
        match self {
            Self::Unread => "Unread",
            Self::Reading => "Reading",
            Self::Finished => "Finished",
        }
    }
}

impl std::fmt::Display for LibraryReadingFilter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
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

/// A rendered page prepared for display by iced.
#[derive(Debug, Clone)]
pub struct RenderedPageView {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

impl From<RenderedPage> for RenderedPageView {
    fn from(page: RenderedPage) -> Self {
        Self {
            width: page.width,
            height: page.height,
            handle: image::Handle::from_rgba(
                u32::from(page.width),
                u32::from(page.height),
                page.rgba,
            ),
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
    /// Last document error shown in the viewer.
    pub document_error: Option<String>,
    /// Rendered tile cache.
    pub cache: TileCache,
    /// Current vertical scroll offset.
    pub scroll_offset: f32,
    /// Current horizontal pan offset for wide/zoomed pages.
    pub horizontal_offset: f32,
    /// Current rendered page width.
    pub zoom_width: u16,
    /// Render width used as the stable preview source during an active zoom gesture.
    pub zoom_preview_width_px: Option<u16>,
    /// Monotonic token used to debounce wheel zoom rendering.
    pub zoom_generation: u64,
    /// UI scale factor used to render pages at physical-pixel resolution.
    pub scale_factor: f32,
    /// Last known keyboard modifiers.
    pub modifiers: keyboard::Modifiers,
    /// Tile render jobs currently in flight.
    pub pending_renders: HashSet<TileKey>,
    /// Whether the table-of-contents panel is open.
    pub toc_open: bool,
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
    /// Open selected-PDF contextual menu.
    pub open_selection_menu: Option<SelectionMenu>,
    /// User settings.
    pub settings: Settings,
    /// Library database handle.
    pub db: Arc<Db>,
}

/// A rendered cover thumbnail prepared for display by iced.
#[derive(Debug, Clone)]
pub struct ThumbnailView {
    /// Thumbnail width in pixels.
    pub width: u16,
    /// Thumbnail height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

/// Rendered cover thumbnail cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailCacheKey {
    /// Library entry id.
    pub entry_id: EntryId,
    /// Thumbnail resolution tier.
    pub size: ThumbnailSize,
}

/// Thumbnail resolution tiers stored on disk and selected by grid zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    /// Dense grid thumbnail.
    Small,
    /// Normal grid and list thumbnail.
    Default,
    /// Large single/few-column grid thumbnail.
    Large,
}

impl ThumbnailSize {
    fn width_px(self) -> u16 {
        match self {
            Self::Small => 96,
            Self::Default => 200,
            Self::Large => 640,
        }
    }

    fn cache_suffix(self) -> Option<&'static str> {
        match self {
            Self::Small => Some("small"),
            Self::Default => None,
            Self::Large => Some("large"),
        }
    }
}

/// Current manual-reorder drag state for the library view.
#[derive(Debug, Clone)]
pub struct LibraryDragState {
    /// Entry being dragged.
    pub entry_id: EntryId,
    /// Entries moved by this drag, in their original visible order.
    pub entry_ids: Vec<EntryId>,
    /// Original zero-based index in the visible manual-order list.
    pub source_index: usize,
    /// Current insertion index after removing the dragged entry from the visible list.
    pub target_index: usize,
    /// Whether this drag moves multiple selected entries as a group.
    pub multi: bool,
    /// Active folder target for additive assignment.
    pub drop_target: Option<FolderId>,
    /// Folder currently hovered while waiting for dwell activation.
    pub pending_drop_target: Option<FolderId>,
    /// Time when the current folder hover began.
    pub pending_drop_started_at: Option<Instant>,
    /// Sidebar folders expanded by drag dwell and eligible to collapse on leave.
    pub expanded_during_drag: HashSet<FolderId>,
    /// Whether pointer movement has crossed the drag threshold.
    pub active: bool,
    /// Cursor position recorded when the press began.
    pub press_cursor: Option<Point>,
    /// Latest cursor position in window coordinates.
    pub cursor: Option<Point>,
    /// Last auto-scroll tick used for frame-rate independent scrolling.
    pub last_auto_scroll_tick: Option<Instant>,
}

/// Current drag state for moving a folder into another folder.
#[derive(Debug, Clone)]
pub struct FolderDragState {
    /// Folder being dragged.
    pub folder_id: FolderId,
    /// Active folder target for nesting.
    pub drop_target: Option<FolderId>,
    /// Folder currently hovered while waiting for dwell activation.
    pub pending_drop_target: Option<FolderId>,
    /// Time when the current folder hover began.
    pub pending_drop_started_at: Option<Instant>,
    /// Sidebar folders expanded by drag dwell and eligible to collapse on cancel.
    pub expanded_during_drag: HashSet<FolderId>,
    /// Whether pointer movement has crossed the drag threshold.
    pub active: bool,
    /// Cursor position recorded when the press began.
    pub press_cursor: Option<Point>,
    /// Latest cursor position in window coordinates.
    pub cursor: Option<Point>,
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
            document_error: None,
            cache: TileCache::with_default_capacity(),
            scroll_offset: 0.0,
            horizontal_offset: 0.0,
            zoom_width: settings.default_zoom_width,
            zoom_preview_width_px: None,
            zoom_generation: 0,
            scale_factor: 1.0,
            modifiers: keyboard::Modifiers::default(),
            pending_renders: HashSet::new(),
            toc_open: true,
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

        Ok(app)
    }

    fn open_document(&mut self, doc: Arc<PdfDoc>) -> Task<Message> {
        self.mode = AppMode::Viewer;
        self.doc = Some(Arc::clone(&doc));
        self.cache.clear();
        self.rendered_pages.clear();
        self.page_aspect_ratios = (0..doc.page_count())
            .map(|page| doc.page_aspect_ratio(page).unwrap_or(11.0 / 8.5))
            .collect();
        self.outline = doc.outline().unwrap_or_default();
        self.expanded_outline_paths.clear();
        self.pending_renders.clear();
        self.scroll_offset = 0.0;
        self.horizontal_offset = 0.0;
        self.zoom_preview_width_px = None;
        self.zoom_generation = self.zoom_generation.wrapping_add(1);
        self.document_error = None;
        self.jump_dialog_open = false;
        self.jump_input.clear();

        self.request_visible_pages()
    }

    fn return_to_library(&mut self) -> Task<Message> {
        self.mode = AppMode::Library;
        self.doc = None;
        self.current_entry_id = None;
        self.rendered_pages.clear();
        self.pending_renders.clear();
        self.page_aspect_ratios.clear();
        self.outline.clear();
        self.expanded_outline_paths.clear();
        self.document_error = None;
        self.jump_dialog_open = false;
        self.jump_input.clear();
        self.zoom_preview_width_px = None;
        self.scroll_offset = 0.0;
        self.horizontal_offset = 0.0;
        Task::batch([
            self.refresh_library(),
            self.refresh_folders(),
            self.request_visible_thumbnails(),
        ])
    }

    fn open_library_document(&mut self, entry_id: EntryId, doc: Arc<PdfDoc>) -> Task<Message> {
        self.current_entry_id = Some(entry_id.clone());
        let last_page = self
            .library_entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .map_or(0, |entry| entry.last_page);
        let task = self.open_document(doc);
        self.scroll_offset = self.page_top(last_page);
        self.clamp_scroll_offset();
        Task::batch([task, self.request_visible_pages()])
    }

    fn request_visible_pages(&mut self) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        for page in self.visible_page_range() {
            let key = TileKey {
                page,
                width_px: self.render_width_px(),
            };

            if self.rendered_pages.contains_key(&key) || self.pending_renders.contains(&key) {
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

            self.pending_renders.insert(key);
            let doc = Arc::clone(doc);
            tasks.push(Task::perform(
                render_page(doc, key),
                |result| match result {
                    Ok((key, page)) => Message::PageRendered {
                        key,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                    },
                    Err(error) => Message::DocumentError(error.to_string()),
                },
            ));
        }

        Task::batch(tasks)
    }

    fn visible_page_range(&self) -> std::ops::Range<u16> {
        let Some(doc) = &self.doc else {
            return 0..0;
        };

        let page_count = doc.page_count();
        let top = self.scroll_offset.max(0.0);
        let bottom = top + self.viewport_height.max(1.0) + Spacing::PAGE_GAP;
        let mut y = Spacing::PAGE_GUTTER;
        let mut first = None;
        let mut end = 0;

        for page in 0..page_count {
            let height = self.page_height(page);
            let page_top = y;
            let page_bottom = y + height;

            if page_bottom >= top && page_top <= bottom {
                first.get_or_insert(page);
                end = page.saturating_add(1);
            } else if page_top > bottom && first.is_some() {
                break;
            }

            y = page_bottom + Spacing::PAGE_GAP;
        }

        first.unwrap_or(0)..end.max(first.unwrap_or(0).saturating_add(1).min(page_count))
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
        let pages: f32 = (0..self.doc.as_ref().map_or(0, |doc| doc.page_count()))
            .map(|page| self.page_height(page) + Spacing::PAGE_GAP)
            .sum();
        pages + Spacing::PAGE_GUTTER * 2.0
    }

    fn content_width(&self) -> f32 {
        f32::from(self.zoom_width) + Spacing::PAGE_GUTTER * 2.0
    }

    fn current_page(&self) -> u16 {
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
            .filter(|entry| {
                self.selected_folder.as_ref().is_none_or(|folder_id| {
                    entry.folders.iter().any(|folder| &folder.id == folder_id)
                })
            })
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

    fn can_drag_reorder_library(&self) -> bool {
        self.library_sort_mode == LibrarySortMode::Manual
            && self.search_query.trim().is_empty()
            && self.search_results.is_none()
            && self.active_tag_filter.is_none()
            && self.selected_folder.is_none()
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

        self.library_drag = Some(LibraryDragState {
            entry_id,
            entry_ids,
            source_index,
            target_index: source_index,
            multi,
            drop_target: None,
            pending_drop_target: None,
            pending_drop_started_at: None,
            expanded_during_drag: HashSet::new(),
            active: false,
            press_cursor: None,
            cursor: None,
            last_auto_scroll_tick: None,
        });
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
        self.folder_drag = Some(FolderDragState {
            folder_id,
            drop_target: None,
            pending_drop_target: None,
            pending_drop_started_at: None,
            expanded_during_drag: HashSet::new(),
            active: false,
            press_cursor: None,
            cursor: None,
        });
    }

    fn update_library_drag_target(&mut self, cursor: Point) {
        if self.library_drag.is_none() {
            return;
        }

        let can_drag_reorder = self.can_drag_reorder_library();
        if let Some(drag) = &mut self.library_drag {
            let press_cursor = *drag.press_cursor.get_or_insert(cursor);
            drag.cursor = Some(cursor);
            if distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE {
                if !drag.active {
                    drag.active = true;
                }
                if !can_drag_reorder && drag.drop_target.is_none() {
                    self.library_status = Some(String::from(
                        "Drop on a folder, or switch to unfiltered Manual sort to reorder PDFs.",
                    ));
                }
            }
        }

        if self.library_drag.as_ref().is_some_and(|drag| drag.active) {
            self.update_library_drag_target_from_cursor();
        }
    }

    fn set_folder_drop_hover_target(&mut self, folder_id: Option<FolderId>, now: Instant) {
        if self.library_drag.is_none() && self.folder_drag.is_none() {
            return;
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
            if drag.pending_drop_target == folder_id {
                return;
            }

            drag.pending_drop_target = folder_id.clone();
            drag.pending_drop_started_at = drag.pending_drop_target.as_ref().map(|_| now);
            drag.drop_target = None;
        }

        if let Some(drag) = &mut self.folder_drag {
            let target = folder_id.or(folder_drag_card_target).filter(|target| {
                folder_can_move_into(&self.library_folders, &drag.folder_id, target)
            });
            if drag.pending_drop_target == target {
                return;
            }

            drag.pending_drop_target = target.clone();
            drag.pending_drop_started_at = drag.pending_drop_target.as_ref().map(|_| now);
            drag.drop_target = if drag.active { target } else { None };
        }
    }

    fn update_folder_drop_target_dwell(&mut self, now: Instant) {
        let library_target = self.library_drag.as_ref().and_then(|drag| {
            if !drag.active || drag.drop_target.is_some() {
                return None;
            }
            let folder_id = drag.pending_drop_target.clone()?;
            let started_at = drag.pending_drop_started_at?;
            folder_drop_target_ready(started_at, now).then_some(folder_id)
        });

        let folder_target = self.folder_drag.as_ref().and_then(|drag| {
            if !drag.active || drag.drop_target.is_some() {
                return None;
            }
            let folder_id = drag.pending_drop_target.clone()?;
            let started_at = drag.pending_drop_started_at?;
            folder_drop_target_ready(started_at, now).then_some(folder_id)
        });

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

        let press_cursor = *drag.press_cursor.get_or_insert(cursor);
        drag.cursor = Some(cursor);
        if distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE {
            drag.active = true;
        }
        if !drag.active {
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
        if drag.drop_target == target {
            return;
        }
        drag.pending_drop_target = target.clone();
        drag.pending_drop_started_at = target.as_ref().map(|_| Instant::now());
        drag.drop_target = target;
    }

    fn active_folder_drop_target(&self) -> Option<&FolderId> {
        self.library_drag
            .as_ref()
            .and_then(|drag| drag.drop_target.as_ref())
            .or_else(|| {
                self.folder_drag
                    .as_ref()
                    .and_then(|drag| drag.drop_target.as_ref())
            })
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
                assign_entries_to_folder_task(Arc::clone(&self.db), entry_ids, folder_id),
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
        let mut y = Spacing::PAGE_GUTTER;
        for page in 0..target_page {
            y += self.page_height(page) + Spacing::PAGE_GAP;
        }
        y
    }

    fn jump_to_page(&mut self, page: u16) -> Task<Message> {
        let Some(doc) = &self.doc else {
            return Task::none();
        };

        let page = page.min(doc.page_count().saturating_sub(1));
        self.scroll_offset = self.page_top(page);
        self.clamp_scroll_offset();
        self.jump_dialog_open = false;
        self.jump_input.clear();
        self.request_visible_pages()
    }

    fn max_horizontal_offset(&self) -> f32 {
        (self.content_width() - self.viewport_width.max(1.0)).max(0.0)
    }

    fn max_scroll_offset(&self) -> f32 {
        (self.content_height() - self.viewport_height.max(1.0)).max(0.0)
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
        self.scroll_offset = (self.scroll_offset + delta).clamp(0.0, self.max_scroll_offset());
        self.request_visible_pages()
    }

    fn pan_horizontally_by(&mut self, delta: f32) {
        self.horizontal_offset =
            (self.horizontal_offset + delta).clamp(0.0, self.max_horizontal_offset());
    }

    fn zoom_to_width(
        &mut self,
        width: u16,
        cursor: Option<Point>,
        render_policy: ZoomRenderPolicy,
    ) -> Task<Message> {
        let previous_width = self.zoom_width;
        let new_width = width.clamp(240, 2400);

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
        self.pending_renders.clear();
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
        self.rendered_pages
            .get(&key)
            .or_else(|| {
                self.zoom_preview_width_px
                    .and_then(|width_px| self.rendered_pages.get(&TileKey { width_px, ..key }))
            })
            .or_else(|| {
                self.rendered_pages
                    .iter()
                    .filter(|(candidate, _)| candidate.page == key.page)
                    .min_by_key(|(candidate, _)| candidate.width_px.abs_diff(key.width_px))
                    .map(|(_, rendered)| rendered)
            })
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
        self.doc
            .as_ref()
            .and_then(|doc| doc.path().file_name())
            .and_then(|name| name.to_str())
            .map(|name| format!("{name} - PDF-Folio"))
            .unwrap_or_else(|| String::from("PDF-Folio"))
    }
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
    .font(GEIST_MONO_PROPO_REGULAR)
    .font(GEIST_MONO_PROPO_MEDIUM)
    .font(GEIST_MONO_PROPO_SEMIBOLD)
    .font(GEIST_MONO_PROPO_BOLD)
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
            app.open_app_menu = if app.open_app_menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
        }
        Message::AppMenuClosed => {
            app.open_app_menu = None;
        }
        Message::AppMenuActionSelected(action) => {
            app.open_app_menu = None;
            if let Some(message) = app_menu_action_message(app, action) {
                return Task::done(message);
            }
        }
        Message::SelectionMenuOpened(menu) => {
            app.open_app_menu = None;
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
        Message::FileSelected(path) => return open_document_task(path),
        Message::DocumentOpened(doc) => return app.open_document(doc),
        Message::LibraryDocumentOpened { entry_id, doc } => {
            return app.open_library_document(entry_id, doc);
        }
        Message::BackToLibrary => return app.return_to_library(),
        Message::DocumentError(error) => {
            app.document_error = Some(error);
            app.pending_renders.clear();
        }
        Message::PageRendered {
            key,
            data,
            width,
            height,
        } => {
            app.pending_renders.remove(&key);
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
            app.library_error = Some(error);
            app.bulk_operation_progress = None;
            app.pending_thumbnails.clear();
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
        }
        Message::BeginFolderDrag(folder_id) => {
            app.begin_folder_drag(folder_id);
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
            app.jump_dialog_open = true;
            app.jump_input = app
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
        }
        Message::CloseOverlay => {
            if app.jump_dialog_open {
                app.jump_dialog_open = false;
                app.jump_input.clear();
            } else if app.create_folder_dialog_open {
                app.create_folder_dialog_open = false;
            } else if app.pending_confirmation.is_some() {
                app.pending_confirmation = None;
            } else if app.open_app_menu.is_some() {
                app.open_app_menu = None;
            } else if app.open_selection_menu.is_some() {
                app.open_selection_menu = None;
            } else {
                app.toc_open = false;
            }
        }
        Message::JumpInputChanged(value) => {
            app.jump_input = value.chars().filter(char::is_ascii_digit).take(5).collect();
        }
        Message::SubmitJump => {
            if let Ok(page) = app.jump_input.parse::<u16>() {
                return app.jump_to_page(page.saturating_sub(1));
            }
        }
        Message::JumpToPage(page) => return app.jump_to_page(page),
        Message::ToggleOutlineNode(path) => {
            if !app.expanded_outline_paths.insert(path.clone()) {
                app.expanded_outline_paths.remove(&path);
            }
        }
        Message::ScrollChanged(offset) => {
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
            app.scroll_offset = scroll_offset;
            app.viewport_width = width;
            app.viewport_height = height;
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();
            return app.request_visible_pages();
        }
        Message::WindowResized { width, height } => {
            app.viewport_width = width.max(1.0);
            app.viewport_height = height.max(1.0);
            if app.mode == AppMode::Library {
                app.recalculate_library_viewport_width();
                app.library_viewport_height =
                    (app.viewport_height - app_menu_bar_height(app) - Spacing::LG * 2.0).max(1.0);
                return app.request_visible_thumbnails();
            }
        }
        Message::ViewportWheelScrolled {
            delta_x,
            delta_y,
            cursor,
            viewport_width,
            viewport_height,
        } => {
            app.viewport_width = viewport_width;
            app.viewport_height = viewport_height;
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();

            if app.modifiers.control() {
                let direction = if delta_y.abs() >= delta_x.abs() {
                    delta_y
                } else {
                    -delta_x
                };
                let step = if direction > 0.0 { 100 } else { -100 };
                let width = (i32::from(app.zoom_width) + step).clamp(240, 2400) as u16;
                return app.zoom_to_width(width, Some(cursor), ZoomRenderPolicy::Debounced);
            }

            if app.modifiers.shift() || delta_x != 0.0 {
                let delta = if delta_x != 0.0 { delta_x } else { delta_y };
                app.horizontal_offset =
                    (app.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
            } else {
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
            return app.zoom_to_width(
                app.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ZoomOut => {
            return app.zoom_to_width(
                app.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::In) => {
            return app.zoom_to_width(
                app.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::Out) => {
            return app.zoom_to_width(
                app.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::Reset) => {
            return app.zoom_to_width(
                app.settings.default_zoom_width,
                None,
                ZoomRenderPolicy::Immediate,
            );
        }
        Message::ShortcutPressed(Shortcut::ToggleTheme) => {
            app.theme = app.theme.toggled();
        }
        Message::ShortcutPressed(Shortcut::ReloadStyles) => {
            return Task::done(Message::ReloadStyles);
        }
        Message::ShortcutPressed(Shortcut::PageDown) => {
            return app.scroll_by(app.viewport_height * 0.86);
        }
        Message::ShortcutPressed(Shortcut::PageUp) => {
            return app.scroll_by(-(app.viewport_height * 0.86));
        }
        Message::ShortcutPressed(Shortcut::FineScroll(delta)) => {
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
            app.jump_dialog_open = true;
            app.jump_input = (u32::from(app.current_page()) + 1).to_string();
        }
        Message::ShortcutPressed(Shortcut::Escape) => {
            if app.pending_confirmation.is_some() {
                app.pending_confirmation = None;
            } else if app.open_app_menu.is_some() {
                app.open_app_menu = None;
            } else if app.open_selection_menu.is_some() {
                app.open_selection_menu = None;
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
            return app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        }
        _ => {}
    }

    Task::none()
}

fn app_menu_action_message(app: &PDFolioApp, action: AppMenuAction) -> Option<Message> {
    Some(match action {
        AppMenuAction::OpenFile => Message::OpenFileDialog,
        AppMenuAction::ImportFolder => Message::ImportFolderDialog,
        AppMenuAction::BackToLibrary => Message::BackToLibrary,
        AppMenuAction::RefreshLibrary => Message::LibraryRefresh,
        AppMenuAction::SelectAllVisible => Message::SelectAllVisibleLibraryEntries,
        AppMenuAction::ClearSelection => Message::ClearLibrarySelection,
        AppMenuAction::SaveDetails => Message::SaveDetailsMetadata,
        AppMenuAction::ResetDetails => {
            let entry_id = app.details_entry_id.clone()?;
            Message::RequestConfirmation(ConfirmationAction::ResetDetailsMetadata(entry_id))
        }
        AppMenuAction::AddTag => Message::BulkAddTag,
        AppMenuAction::RemoveTag => Message::BulkRemoveTag,
        AppMenuAction::AddToFolder => Message::BulkAddToCurrentFolder,
        AppMenuAction::RemoveFromFolder => Message::BulkRemoveFromCurrentFolder,
        AppMenuAction::DeleteFromLibrary => {
            Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)
        }
        AppMenuAction::ToggleLayout => Message::ToggleViewMode,
        AppMenuAction::ToggleTheme => Message::ThemeToggled,
        AppMenuAction::ReloadStyles => Message::ReloadStyles,
        AppMenuAction::ToggleToc => Message::ToggleSidebar,
        AppMenuAction::JumpToPage => Message::OpenJumpDialog,
        AppMenuAction::ZoomIn => Message::ZoomIn,
        AppMenuAction::ZoomOut => Message::ZoomOut,
        AppMenuAction::ResetZoom => Message::ZoomSet(app.settings.default_zoom_width),
        AppMenuAction::SortLibrary(sort_mode) => Message::LibrarySortChanged(sort_mode),
        AppMenuAction::CreateFolder => Message::OpenCreateFolderDialog,
        AppMenuAction::ResetMetadata => {
            Message::RequestConfirmation(ConfirmationAction::BulkResetDisplayMetadata)
        }
        AppMenuAction::SortTitles => Message::BulkApplyTitleSortCleanup,
        AppMenuAction::RefreshMetadata => Message::BulkRefreshPdfMetadata,
        AppMenuAction::RebuildThumbnails => Message::BulkRebuildThumbnails,
        AppMenuAction::Reindex => Message::BulkReindex,
    })
}

fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let base_content: Element<'_, Message> = if app.doc.is_some() {
        let sidebar: Element<'_, Message> = if app.toc_open {
            view_sidebar(app).into()
        } else {
            container("").width(Length::Shrink).into()
        };

        let viewer = canvas(ViewerCanvas { app })
            .width(Length::Fill)
            .height(Length::Fill);
        let main = if app.jump_dialog_open {
            column![view_jump_dialog(app), viewer].spacing(0)
        } else {
            column![viewer]
        };

        column![
            view_app_menu_bar(app),
            row![sidebar, main.width(Length::Fill)].height(Length::Fill)
        ]
        .into()
    } else {
        column![view_app_menu_bar(app), view_library(app)].into()
    };

    let menu_content = if app.open_app_menu.is_some() {
        stack![
            base_content,
            app_menu_capture_layer(app),
            view_app_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.open_selection_menu.is_some() {
        stack![
            base_content,
            selection_menu_capture_layer(app),
            view_selection_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        base_content
    };

    let content = if app.pending_confirmation.is_some() {
        stack![menu_content, view_confirmation_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.create_folder_dialog_open {
        stack![menu_content, view_create_folder_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_folder_drag_preview(app, tokens) {
        stack![menu_content, floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_library_drag_preview(app, tokens) {
        stack![menu_content, library_drag_capture_layer(), floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        menu_content
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into()
}

fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let entries = app.visible_library_entries();
    let child_folders = app.child_folders();
    let render_items = library_render_items(app, &entries);
    let folder_section_height = folder_cards_section_height(app, child_folders.len());
    let entry_scroll_offset = (app.library_scroll_offset - folder_section_height).max(0.0);
    let window = app.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
    let mut header = row![];
    if !app.library_tag_sidebar_open {
        header = header.push(sidebar_chevron_button(
            CHEVRON_RIGHT_SVG,
            "Expand Sidebar",
            Message::ExpandLibrarySidebar,
            tokens,
        ));
    }
    let header = header
        .push(
            search_input_with_class(
                "Search library",
                &app.search_query,
                tokens,
                Class::LibrarySearchInput,
                Message::SearchQueryChanged,
            )
            .id(Id::new(LIBRARY_SEARCH_INPUT_ID))
            .width(Length::Fill),
        )
        .push(
            pick_list(
                LIBRARY_SORT_OPTIONS,
                Some(app.library_sort_mode),
                Message::LibrarySortChanged,
            )
            .placeholder("Sort")
            .width(190.0)
            .menu_height(360.0)
            .padding([Spacing::SM, Spacing::MD])
            .text_size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
            .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown)),
        )
        .push(library_layout_toggle_button(app, tokens))
        .push(library_metadata_density_picker(app, tokens))
        .push(library_new_folder_button(tokens).on_press(Message::OpenCreateFolderDialog))
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center);
    let header = container(header)
        .width(Length::Fill)
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::LibraryControlBar));

    let reorder_hint = if app.can_drag_reorder_library() {
        "Manual reorder enabled"
    } else {
        "Reordering requires unfiltered Manual sort"
    };
    let context_row = if app.selected_library_entries.is_empty() {
        view_library_breadcrumb_row(app, tokens, reorder_hint)
    } else {
        view_library_selection_status_row(app, tokens, reorder_hint)
    };
    let mut content = column![header, context_row,]
        .spacing(Spacing::MD)
        .padding(Spacing::LG);
    if let Some(progress) = app.bulk_operation_progress.as_ref() {
        content = content.push(bulk_operation_progress_banner(app, progress, tokens));
    }
    if let Some(error) = app.library_error.as_deref() {
        content = content.push(error_banner(error, tokens));
    }

    if entries.is_empty() && child_folders.is_empty() {
        content = content.push(empty_state(
            if app.selected_folder.is_some() {
                "This folder is empty."
            } else {
                "Import a folder of PDFs to build your library."
            },
            tokens,
        ));
    } else if app.compact_view_mode {
        let mut rows = column![].spacing(Spacing::SM);
        let top_spacer = window.start as f32 * app.library_row_height();
        let bottom_spacer =
            entries.len().saturating_sub(window.end) as f32 * app.library_row_height();
        if top_spacer > 0.0 {
            rows = rows.push(container("").height(top_spacer));
        }
        for item in render_items[window.clone()].iter().cloned() {
            rows = rows.push(match item {
                LibraryRenderItem::Entry(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Normal)
                }
                LibraryRenderItem::Ghost(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Placeholder)
                }
                LibraryRenderItem::DropZone(_) => library_drop_zone_row(app, tokens),
            });
        }
        if bottom_spacer > 0.0 {
            rows = rows.push(container("").height(bottom_spacer));
        }
        let scroll_content = if child_folders.is_empty() {
            rows
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), rows]
                .spacing(Spacing::MD)
        };
        content = content.push(library_scrollable(scroll_content, tokens));
    } else {
        let layout = app.library_render_item_masonry_layout(&render_items);
        let mut grid = row![]
            .spacing(app.layout().library_masonry_gap)
            .height(layout.content_height);
        for column_items in &layout.columns {
            let mut stack = column![]
                .width(app.library_grid_card_width())
                .height(layout.content_height);
            let mut cursor_y = 0.0;
            for item_layout in column_items {
                let bottom = item_layout.top + item_layout.height;
                let visible_top = entry_scroll_offset
                    - app.layout().library_overscan_rows as f32 * app.library_row_height();
                let visible_bottom = entry_scroll_offset
                    + app.library_viewport_height.max(1.0)
                    + app.layout().library_overscan_rows as f32 * app.library_row_height();
                if bottom < visible_top || item_layout.top > visible_bottom {
                    continue;
                }

                let spacer = item_layout.top - cursor_y;
                if spacer > 0.0 {
                    stack = stack.push(container("").height(spacer));
                }
                if let Some(item) = render_items.get(item_layout.index).cloned() {
                    stack = stack.push(match item {
                        LibraryRenderItem::Entry(entry) => {
                            library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Normal)
                        }
                        LibraryRenderItem::Ghost(entry) => library_entry_card(
                            app,
                            entry,
                            tokens,
                            LibraryEntryRenderMode::Placeholder,
                        ),
                        LibraryRenderItem::DropZone(_) => library_drop_zone_card(app, tokens),
                    });
                    cursor_y = bottom;
                }
            }
            let trailing = layout.content_height - cursor_y;
            if trailing > 0.0 {
                stack = stack.push(container("").height(trailing));
            }
            grid = grid.push(stack);
        }
        grid = grid.push(container("").width(app.layout().library_scrollbar_gutter));
        let scroll_content = if child_folders.is_empty() {
            column![grid]
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), grid]
                .spacing(Spacing::MD)
        };
        content = content.push(library_scrollable(scroll_content, tokens));
    }

    let main_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell));

    let mut layout = row![].height(Length::Fill);
    if app.library_tag_sidebar_open {
        layout = layout.push(view_library_tag_sidebar(app));
    }
    layout = layout.push(main_content);
    layout.height(Length::Fill).into()
}

fn view_library_breadcrumb_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let breadcrumbs = app.folder_breadcrumbs();
    let active_index = breadcrumbs.len().saturating_sub(1);
    let mut trail = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    for (index, (label, folder_id)) in breadcrumbs.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(
                text(">")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            );
        }

        trail = trail.push(breadcrumb_button(
            label,
            folder_id,
            index == active_index,
            tokens,
        ));
    }

    row![
        row![
            trail.width(Length::Shrink),
            library_quick_filter_chips(app, tokens),
            library_filter_summary(app, tokens),
            library_grid_zoom_control(app, tokens),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
        text(reorder_hint)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

fn library_quick_filter_chips<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut chips = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    for filter in LIBRARY_READING_FILTER_OPTIONS {
        let active = app.active_reading_filter == Some(filter);
        let label = filter.label();
        let next = if active { None } else { Some(filter) };
        let chip = tag_pill(label, tokens)
            .on_press(Message::ReadingFilterChanged(next))
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_, status| {
                if active {
                    crate::style::button_style(tokens, Class::LibraryImportButton, status)
                } else {
                    crate::style::button_style(tokens, Class::TagPill, status)
                }
            });
        chips = chips.push(chip);
    }

    let missing_active = app.missing_filter_active;
    chips = chips.push(
        tag_pill("Missing", tokens)
            .on_press(Message::MissingFilterChanged(!missing_active))
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_, status| {
                if missing_active {
                    crate::style::button_style(tokens, Class::LibraryImportButton, status)
                } else {
                    crate::style::button_style(tokens, Class::TagPill, status)
                }
            }),
    );

    chips.into()
}

fn library_filter_summary<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let mut labels = Vec::new();
    if let Some(folder_name) = app.selected_folder_name() {
        labels.push(format!("Folder: {folder_name}"));
    }
    if let Some(tag) = app.active_tag_filter.as_ref() {
        labels.push(format!("Tag: {tag}"));
    }
    if let Some(filter) = app.active_reading_filter {
        labels.push(format!("Reading: {}", filter.label()));
    }
    if app.missing_filter_active {
        labels.push(String::from("Missing files"));
    }
    let query = app.search_query.trim();
    if !query.is_empty() {
        labels.push(format!("Search: {query}"));
    }

    if labels.is_empty() {
        return container("").width(Length::Shrink).into();
    }

    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for label in labels {
        row = row.push(
            container(
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_primary)
                    .wrapping(Wrapping::None),
            )
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| container_style(tokens, Class::TagPill)),
        );
    }

    row = row.push(
        tag_pill("Clear filters", tokens)
            .on_press(Message::ClearLibraryFilters)
            .padding([Spacing::XS, Spacing::MD]),
    );
    row.into()
}

fn view_library_selection_status_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let selected_count = app.selected_library_entries.len();
    let mut details = row![
        master_checkbox(
            app.master_checkbox_state(),
            tokens,
            Message::MasterCheckboxClicked
        ),
        text(format!("{} selected", format_count(selected_count, "PDF")))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.accent),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    if let Some(status) = app.library_status.as_deref() {
        details = details.push(
            text(status)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary),
        );
    }

    row![
        details,
        text(reorder_hint)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

fn breadcrumb_button<'a>(
    label: String,
    folder_id: Option<FolderId>,
    active: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.accent
            })
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_, status| {
        let hovered = matches!(
            status,
            iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
        );
        let mut style = crate::style::button_style(tokens, Class::SidebarRow, status);
        style.background = Some(iced::Background::Color(if hovered && !active {
            mix_color(tokens.background, tokens.accent, 0.12)
        } else {
            tokens.background
        }));
        style.border.width = 0.0;
        style
    })
    .on_press(Message::FolderSelected(folder_id))
    .into()
}

fn library_scrollable<'a>(
    content: iced::widget::Column<'a, Message>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    scrollable(content)
        .id(Id::new(LIBRARY_SCROLLABLE_ID))
        .height(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::LibraryRow, status))
        .on_scroll(|viewport| {
            let offset = viewport.absolute_offset();
            let bounds = viewport.bounds();
            Message::LibraryScrolled {
                offset_y: offset.y,
                viewport_x: bounds.x,
                viewport_y: bounds.y,
                viewport_width: bounds.width,
                viewport_height: bounds.height,
            }
        })
        .into()
}

fn view_confirmation_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let Some(action) = app.pending_confirmation.as_ref() else {
        return container("").into();
    };
    let (title, body, confirm_label) = confirmation_copy(action, app);
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
            .color(tokens.text_primary),
        text(body).size(FontSize::MD).color(tokens.text_secondary),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelConfirmation),
            toolbar_button(confirm_label, tokens).on_press(Message::ConfirmPendingAction),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| {
        let mut style = container_style(tokens, Class::PresentationOverlay);
        style.background = Some(iced::Background::Color(with_alpha(tokens.canvas, 0.72)));
        style
    })
    .into()
}

fn bulk_operation_progress_banner<'a>(
    app: &'a PDFolioApp,
    progress: &'a BulkOperationProgress,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let elapsed = app
        .animation_now
        .saturating_duration_since(progress.started_at)
        .as_secs_f32();
    let value = indeterminate_progress_value(elapsed);
    let label = format!("{} {} PDFs...", progress.label, progress.total);

    container(
        column![
            row![
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text("Working in background")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            ]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center),
            progress_bar(value, tokens),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

fn indeterminate_progress_value(elapsed_secs: f32) -> f32 {
    let sweep = (elapsed_secs * 0.72).fract();
    (0.18 + 0.64 * (0.5 - (sweep - 0.5).abs()) * 2.0).clamp(0.0, 1.0)
}

fn folder_drop_flash_active_at(
    folder_id: &FolderId,
    flash: Option<(&FolderId, Instant)>,
    now: Instant,
) -> bool {
    flash.is_some_and(|(flashed_folder_id, started_at)| {
        flashed_folder_id == folder_id
            && now.saturating_duration_since(started_at)
                < Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
    })
}

fn view_create_folder_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let parent = app
        .selected_folder_name()
        .unwrap_or_else(|| String::from("Library"));
    let dialog = column![
        text("New Folder")
            .size(FontSize::HEADING)
            .color(tokens.text_primary),
        text(format!("Create a folder in {parent}."))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        text_input("Folder name", &app.new_folder_name)
            .on_input(Message::NewFolderNameChanged)
            .on_submit(Message::CreateFolder)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
            toolbar_button("Create", tokens).on_press(Message::CreateFolder),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| {
        let mut style = container_style(tokens, Class::PresentationOverlay);
        style.background = Some(iced::Background::Color(with_alpha(tokens.canvas, 0.72)));
        style
    })
    .into()
}

fn confirmation_copy<'a>(
    action: &'a ConfirmationAction,
    app: &'a PDFolioApp,
) -> (&'a str, String, &'a str) {
    match action {
        ConfirmationAction::BulkResetDisplayMetadata => (
            "Reset metadata?",
            format!(
                "This will clear display title and author edits for {} selected PDFs.",
                app.selected_library_entries.len()
            ),
            "Reset",
        ),
        ConfirmationAction::BulkDeleteFromLibrary => (
            "Delete from library?",
            format!(
                "This removes library metadata for {} selected PDFs. The PDF files remain on disk.",
                app.selected_library_entries.len()
            ),
            "Delete",
        ),
        ConfirmationAction::ResetDetailsMetadata(_) => (
            "Reset PDF details?",
            String::from("This clears the edited display title and author for this PDF."),
            "Reset",
        ),
        ConfirmationAction::DeleteFolder(folder_id) => (
            "Delete folder?",
            format!(
                "This removes the folder \"{}\" and any nested folders. PDFs remain in the library and on disk.",
                app.library_folders
                    .iter()
                    .find(|folder| &folder.id == folder_id)
                    .map_or("Selected folder", |folder| folder.name.as_str())
            ),
            "Delete",
        ),
    }
}

fn view_folder_cards<'a>(
    app: &'a PDFolioApp,
    folders: Vec<Folder>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let active_folder_drag = app.folder_drag.as_ref().filter(|drag| drag.active);
    let mut rows = column![].spacing(Spacing::SM);
    for chunk in folders.chunks(folder_cards_per_row(app)) {
        let mut card_row = row![].spacing(app.layout().library_masonry_gap);
        for folder in chunk {
            let mode = if active_folder_drag.is_some_and(|drag| drag.folder_id == folder.id) {
                if active_folder_drag
                    .and_then(|drag| drag.drop_target.as_ref())
                    .is_some()
                {
                    FolderCardRenderMode::NestingTarget
                } else {
                    FolderCardRenderMode::Placeholder
                }
            } else {
                FolderCardRenderMode::Normal
            };
            card_row = card_row.push(folder_grid_card(app, folder.clone(), tokens, mode));
        }
        rows = rows.push(card_row);
    }
    rows.into()
}

fn folder_cards_per_row(app: &PDFolioApp) -> usize {
    let available_width =
        (app.library_viewport_width - app.layout().library_scrollbar_gutter).max(1.0);
    let card_pitch = app.library_grid_card_width() + app.layout().library_masonry_gap;
    ((available_width + app.layout().library_masonry_gap) / card_pitch)
        .floor()
        .max(1.0) as usize
}

fn folder_cards_section_height(app: &PDFolioApp, folder_count: usize) -> f32 {
    if folder_count == 0 {
        return 0.0;
    }

    let rows = folder_count.div_ceil(folder_cards_per_row(app)).max(1);
    rows as f32 * app.layout().library_folder_grid_row_height
        + rows.saturating_sub(1) as f32 * Spacing::SM
        + Spacing::MD
}

fn folder_grid_card<'a>(
    app: &'a PDFolioApp,
    folder: Folder,
    tokens: ThemeTokens,
    mode: FolderCardRenderMode,
) -> Element<'a, Message> {
    let folder_id = folder.id.clone();
    let drop_active = app.active_folder_drop_target() == Some(&folder.id);
    let flash_active = app.folder_drop_flash_active(&folder.id);
    let smart_counts = app.folder_smart_counts(Some(&folder.id));
    let child_count = app
        .library_folders
        .iter()
        .filter(|child| child.parent_id.as_ref() == Some(&folder.id))
        .count();
    let meta = folder_meta_label(smart_counts, child_count);
    let folder_title_size = app.library_card_font_size(FontSize::CONTROL);
    let folder_meta_size = app.library_card_font_size(FontSize::SM);
    let folder_text_width = (app.library_grid_card_width() - 72.0).max(16.0);
    let content_alpha = folder_card_content_alpha(app, mode);
    let title =
        truncate_for_width_with_font(&folder.name, folder_text_width, 0.0, folder_title_size);
    let meta = truncate_for_width_with_font(&meta, folder_text_width, 0.0, folder_meta_size);
    let content = row![
        folder_icon(tokens, content_alpha),
        column![
            text(title)
                .size(folder_title_size)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(with_alpha(tokens.text_primary, content_alpha))
                .wrapping(Wrapping::None),
            text(meta)
                .size(folder_meta_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(with_alpha(tokens.text_secondary, content_alpha))
                .wrapping(Wrapping::None),
        ]
        .spacing(app.library_card_spacing().min(Spacing::XS))
        .width(Length::Fill),
    ]
    .spacing(app.library_card_spacing().max(Spacing::XS))
    .padding(app.library_card_padding().min(Spacing::MD))
    .height(app.layout().library_folder_grid_row_height)
    .align_y(iced::Alignment::Center);

    let card = container(content)
        .width(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::LibraryFolderCard);
            if matches!(mode, FolderCardRenderMode::Placeholder) {
                let mut background = mix_color(tokens.surface, tokens.background, 0.55);
                background.a = 0.28;
                style.background = Some(iced::Background::Color(background));
                style.border.color = mix_color(tokens.border, tokens.background, 0.62);
            }
            if drop_active || flash_active || matches!(mode, FolderCardRenderMode::NestingTarget) {
                let drop_style = container_style(tokens, Class::FolderDropTarget);
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .width(app.library_grid_card_width());

    if mode == FolderCardRenderMode::Floating {
        return card.into();
    }

    let area = mouse_area(card)
        .on_enter(Message::FolderDropTargetChanged(Some(folder_id)))
        .on_exit(Message::FolderDropTargetChanged(None));
    if mode == FolderCardRenderMode::Normal {
        area.on_press(Message::BeginFolderDrag(folder.id.clone()))
            .on_release(Message::EndFolderDrag)
            .interaction(mouse::Interaction::Grab)
            .into()
    } else {
        area.into()
    }
}

fn folder_icon<'a>(tokens: ThemeTokens, alpha: f32) -> Element<'a, Message> {
    container(
        text("DIR")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(with_alpha(tokens.accent, alpha)),
    )
    .center(38.0)
    .height(28.0)
    .style(move |_| {
        let mut style = container_style(tokens, Class::TagPill);
        style.background = Some(iced::Background::Color(mix_color(
            tokens.surface,
            tokens.accent,
            0.18,
        )));
        style
    })
    .into()
}

fn folder_card_content_alpha(app: &PDFolioApp, mode: FolderCardRenderMode) -> f32 {
    if mode == FolderCardRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

fn folder_meta_label(counts: FolderSmartCounts, child_count: usize) -> String {
    let mut parts = Vec::new();
    if counts.total > 0 {
        parts.push(format_count(counts.total, "PDF"));
    }
    if child_count > 0 {
        parts.push(format_count(child_count, "Folder"));
    }
    if counts.in_progress > 0 {
        parts.push(format!("{} reading", counts.in_progress));
    }
    if counts.missing > 0 {
        parts.push(format!("{} missing", counts.missing));
    }

    if parts.is_empty() {
        String::from("Empty")
    } else {
        parts.join(" . ")
    }
}

fn folder_sidebar_count_label(counts: FolderSmartCounts) -> String {
    if counts.in_progress > 0 || counts.missing > 0 {
        let mut parts = vec![format_count(counts.total, "PDF")];
        if counts.in_progress > 0 {
            parts.push(format!("{} reading", counts.in_progress));
        }
        if counts.missing > 0 {
            parts.push(format!("{} missing", counts.missing));
        }
        parts.join(" . ")
    } else {
        format_count(counts.total, "PDF")
    }
}

fn format_count(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

fn scroll_library_to_offset_task(offset_y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(LIBRARY_SCROLLABLE_ID),
        operation::AbsoluteOffset {
            x: Some(0.0),
            y: Some(offset_y.max(0.0)),
        },
    )
}

impl LibraryRenderItem {
    fn entry(&self) -> &LibraryEntry {
        match self {
            Self::Entry(entry) | Self::Ghost(entry) | Self::DropZone(entry) => entry,
        }
    }
}

fn library_render_items(app: &PDFolioApp, entries: &[LibraryEntry]) -> Vec<LibraryRenderItem> {
    let Some(drag) = app.library_drag.as_ref().filter(|drag| drag.active) else {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    };
    if !drag.multi {
        let Some(ghost_entry) = entries
            .iter()
            .find(|entry| entry.id == drag.entry_id)
            .cloned()
        else {
            return entries
                .iter()
                .cloned()
                .map(LibraryRenderItem::Entry)
                .collect();
        };

        let compact_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.id != drag.entry_id)
            .cloned()
            .collect();
        let target_index = drag.target_index.min(compact_entries.len());

        let mut items = Vec::with_capacity(entries.len());
        for index in 0..=compact_entries.len() {
            if target_index == index {
                items.push(LibraryRenderItem::Ghost(ghost_entry.clone()));
            }

            if let Some(entry) = compact_entries.get(index) {
                items.push(LibraryRenderItem::Entry(entry.clone()));
            }
        }

        return items;
    }

    let dragged_ids = drag.entry_ids.iter().cloned().collect::<HashSet<_>>();
    let placeholder_entries = entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .cloned()
        .collect::<Vec<_>>();
    if placeholder_entries.is_empty() {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    }

    let drop_zone_entry = placeholder_entries[0].clone();
    let target_index = drag
        .target_index
        .min(entries.len().saturating_sub(placeholder_entries.len()));
    let mut compact_index = 0;
    let mut drop_zone_inserted = false;
    let mut items = Vec::with_capacity(entries.len() + 1);
    for entry in entries {
        if dragged_ids.contains(&entry.id) {
            items.push(LibraryRenderItem::Ghost(entry.clone()));
        } else {
            if !drop_zone_inserted && drag.drop_target.is_none() && compact_index == target_index {
                items.push(LibraryRenderItem::DropZone(drop_zone_entry.clone()));
                drop_zone_inserted = true;
            }
            items.push(LibraryRenderItem::Entry(entry.clone()));
            compact_index += 1;
        }
    }
    if !drop_zone_inserted && drag.drop_target.is_none() {
        items.push(LibraryRenderItem::DropZone(drop_zone_entry));
    }

    items
}

fn shortest_column_index(column_heights: &[f32]) -> usize {
    column_heights
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn masonry_target_index(
    layout: &LibraryMasonryLayout,
    column_index: usize,
    content_y: f32,
) -> Option<usize> {
    let column = layout.columns.get(column_index)?;
    if column.is_empty() {
        return Some(layout.columns.iter().flatten().count());
    }

    column
        .iter()
        .find(|item| content_y < item.top + item.height / 2.0)
        .map(|item| item.index)
        .or_else(|| column.last().map(|item| item.index + 1))
}

fn floating_library_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app.library_drag.as_ref().filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let visible_entries = app.visible_library_entries();
    let entry = visible_entries
        .iter()
        .find(|entry| entry.id == drag.entry_id)?
        .clone();

    let preview = if drag.multi {
        multi_drag_stack_preview(app, drag, &visible_entries, tokens)?
    } else if app.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    let x_offset = if app.compact_view_mode {
        app.layout().library_drag_preview_list_x_offset
    } else {
        app.layout().library_drag_preview_grid_x_offset
    };
    let y_offset = if app.compact_view_mode {
        app.layout().library_drag_preview_list_y_offset
    } else {
        app.layout().library_drag_preview_grid_y_offset
    };

    Some(
        pin(preview)
            .x((cursor.x - x_offset).max(0.0))
            .y((cursor.y - y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

fn floating_folder_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app.folder_drag.as_ref().filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let folder = app
        .library_folders
        .iter()
        .find(|folder| folder.id == drag.folder_id)?
        .clone();
    let preview = container(folder_grid_card(
        app,
        folder,
        tokens,
        FolderCardRenderMode::Floating,
    ))
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        pin(preview)
            .x((cursor.x - app.layout().library_drag_preview_grid_x_offset).max(0.0))
            .y((cursor.y - app.layout().library_drag_preview_grid_y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

fn multi_drag_stack_preview<'a>(
    app: &'a PDFolioApp,
    drag: &LibraryDragState,
    visible_entries: &[LibraryEntry],
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let dragged_ids = drag.entry_ids.iter().collect::<HashSet<_>>();
    let mut entries = visible_entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }

    while entries.len() < 3 {
        let entry = entries.last().cloned()?;
        entries.push(entry);
    }

    let rear = drag_stack_card(app, entries[2].clone(), tokens);
    let middle = drag_stack_card(app, entries[1].clone(), tokens);
    let front = drag_stack_card(app, entries[0].clone(), tokens);
    let badge = container(
        text(format_count(drag.entry_ids.len(), "PDF"))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_primary),
    )
    .padding([Spacing::XS, Spacing::MD])
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        stack![
            pin(rear).x(Spacing::LG).y(Spacing::LG),
            pin(middle).x(Spacing::SM).y(Spacing::SM),
            pin(front).x(0.0).y(0.0),
            pin(badge).x(Spacing::MD).y(Spacing::MD),
        ]
        .into(),
    )
}

fn drag_stack_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let card = if app.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    container(card)
        .style(move |_| container_style(tokens, Class::DragStackGhost))
        .into()
}

fn library_drag_capture_layer<'a>() -> Element<'a, Message> {
    mouse_area(container("").width(Length::Fill).height(Length::Fill))
        .on_move(Message::LibraryEntryDragMoved)
        .on_release(Message::EndLibraryEntryDrag)
        .interaction(mouse::Interaction::Grabbing)
        .into()
}

fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let sidebar_width = app.library_tag_sidebar_width;
    let sidebar_body = if let Some(entry) = app.primary_selected_entry() {
        view_selected_pdf_sidebar(app, entry, sidebar_width, tokens)
    } else if !app.selected_library_entries.is_empty() {
        view_multi_selection_sidebar(app, sidebar_width, tokens)
    } else {
        view_library_navigation_sidebar(app, sidebar_width, tokens)
    };

    let sidebar = container(sidebar_body)
        .width(sidebar_width)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::Sidebar));

    let handle_color = if app.resizing_library_tag_sidebar {
        tokens.focus
    } else {
        tokens.border
    };
    let handle_visual_width = if app.resizing_library_tag_sidebar {
        app.layout().sidebar_resize_handle_width
    } else {
        app.layout().sidebar_resize_handle_visual_width
    };
    let resize_handle = mouse_area(
        container(
            container("")
                .width(handle_visual_width)
                .height(Length::Fill)
                .style(move |_| {
                    let mut style = container_style(tokens, Class::Sidebar);
                    style.background = Some(iced::Background::Color(handle_color));
                    style
                }),
        )
        .width(app.layout().sidebar_resize_handle_width)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(Message::BeginTagSidebarResize)
    .on_release(Message::EndTagSidebarResize)
    .interaction(mouse::Interaction::ResizingHorizontally);

    row![sidebar, resize_handle].height(Length::Fill).into()
}

fn view_library_navigation_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let heading = container(
        row![
            section_heading("Explorer", tokens).width(Length::Fill),
            sidebar_chevron_button(
                CHEVRON_LEFT_SVG,
                "Collapse Sidebar",
                Message::CollapseLibrarySidebar,
                tokens,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding(Spacing::MD);

    let sidebar_tab_component = tokens.class_styles[Class::SidebarTab.index()];
    let sidebar_tab_layout = sidebar_tab_component.layout;
    let sidebar_tab_style = sidebar_tab_component.resolve(ComponentState::Normal);
    let tab_area_background = sidebar_tab_style
        .background
        .unwrap_or_else(|| sidebar_tab_area_background(tokens));
    let file_tree_component = tokens.class_styles[Class::FileTree.index()];
    let file_tree_layout = file_tree_component.layout;
    let file_tree_style = file_tree_component.resolve(ComponentState::Normal);
    let content_background = file_tree_style
        .background
        .or_else(|| {
            sidebar_tab_component
                .resolve(ComponentState::Active)
                .background
        })
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let tabs = container(
        row![
            sidebar_tab_button(
                LibrarySidebarTab::Files,
                app.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
            sidebar_tab_button(
                LibrarySidebarTab::Tags,
                app.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
        ]
        .spacing(sidebar_tab_layout.spacing.unwrap_or(Spacing::XS))
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(iced::Padding {
        top: sidebar_tab_layout.margin_top(Spacing::XS),
        right: sidebar_tab_layout.margin_right(Spacing::SM),
        bottom: sidebar_tab_layout.margin_bottom(Spacing::XS),
        left: sidebar_tab_layout.margin_left(Spacing::SM),
    })
    .style(move |_| {
        let mut style = container_style(tokens, Class::Sidebar);
        style.background = Some(iced::Background::Color(tab_area_background));
        style.border.width = 0.0;
        style
    });

    let body = match app.library_sidebar_tab {
        LibrarySidebarTab::Files => view_file_tree_sidebar(app, sidebar_width, tokens),
        LibrarySidebarTab::Tags => view_tag_tree_sidebar(app, sidebar_width, tokens),
    };

    let body_scroll = scrollable(body)
        .height(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::Sidebar, status));

    let padded_body = container(body_scroll)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: file_tree_layout.padding_top(0.0),
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

    let tabbed_body = container(column![tabs, padded_body].spacing(0).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::FileTree);
            if file_tree_style.background.is_none() {
                style.background = Some(iced::Background::Color(content_background));
            }
            style
        });

    let content = column![heading, tabbed_body]
        .spacing(Spacing::SM)
        .height(Length::Fill);

    container(content).height(Length::Fill).into()
}

fn sidebar_tab_button<'a>(
    tab: LibrarySidebarTab,
    active_tab: LibrarySidebarTab,
    tokens: ThemeTokens,
    labels: &'a crate::style::AppLabelTokens,
) -> iced::widget::Button<'a, Message> {
    let active = tab == active_tab;
    let component = tokens.class_styles[Class::SidebarTab.index()];
    let layout = component.layout;
    let text_style = component.text;
    let normal_style = component.resolve(ComponentState::Normal);
    let active_style = component.resolve(ComponentState::Active);
    button(
        text(library_sidebar_tab_label(labels, tab))
            .size(text_style.size.unwrap_or(FontSize::MD))
            .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(if active {
                active_style.text_color.unwrap_or(tokens.text_primary)
            } else {
                normal_style.text_color.unwrap_or(tokens.text_secondary)
            }),
    )
    .height(layout.height.unwrap_or(30.0))
    .width(Length::FillPortion(layout.width_portion.unwrap_or(1)))
    .padding(iced::Padding {
        top: layout.padding_top(Spacing::XS),
        right: layout.padding_right(Spacing::MD),
        bottom: layout.padding_bottom(Spacing::XS),
        left: layout.padding_left(Spacing::MD),
    })
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::SidebarTab, status);
        let state = if active {
            ComponentState::Active
        } else {
            match status {
                iced::widget::button::Status::Active => ComponentState::Normal,
                iced::widget::button::Status::Hovered => ComponentState::Hovered,
                iced::widget::button::Status::Pressed => ComponentState::Pressed,
                iced::widget::button::Status::Disabled => ComponentState::Disabled,
            }
        };
        let state_style = component.resolve(state);
        if let Some(background) = state_style.background {
            style.background = Some(iced::Background::Color(background));
        }
        if let Some(text_color) = state_style.text_color {
            style.text_color = text_color;
        }
        if let Some(border_color) = state_style.border_color {
            style.border.color = border_color;
        }
        if let Some(border_width) = state_style.border_width {
            style.border.width = border_width;
        }
        if let Some(radius) = state_style.radius {
            style.border.radius = radius.into();
        }
        style
    })
    .on_press(Message::LibrarySidebarTabChanged(tab))
}

fn sidebar_tab_area_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, Color::BLACK, 0.34)
    } else {
        mix_color(tokens.surface_raised, Color::BLACK, 0.09)
    }
}

fn sidebar_tab_content_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, tokens.surface_raised, 0.62)
    } else {
        tokens.surface
    }
}

fn is_dark_surface(color: Color) -> bool {
    color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722 < 0.5
}

fn view_file_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let library_counts = app.folder_smart_counts(None);
    let mut tree = column![file_tree_row(
        "Library",
        Some(folder_sidebar_count_label(library_counts)),
        0,
        app.selected_folder.is_none(),
        true,
        app.library_tree_root_expanded,
        Message::ToggleLibraryTreeRoot,
        Message::FolderSelected(None),
        sidebar_width,
        tokens,
        false,
    ),]
    .spacing(0);

    if app.library_tree_root_expanded {
        tree = tree.push(folder_sidebar_rows(app, None, 1, sidebar_width, tokens));
    }

    let mut content = column![tree].spacing(Spacing::SM);
    if let Some(panel) = selected_folder_actions_panel(app, sidebar_width, tokens) {
        content = content.push(panel);
    }

    content.into()
}

fn selected_folder_actions_panel<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let folder = app.selected_folder()?;
    let parent_id = folder.parent_id.clone();
    let has_parent = parent_id.is_some();
    let has_grandparent = parent_id.as_ref().is_some_and(|parent_id| {
        app.library_folders
            .iter()
            .find(|candidate| &candidate.id == parent_id)
            .and_then(|parent| parent.parent_id.as_ref())
            .is_some()
    });
    let can_move_earlier = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, _, index)| index > 0);
    let can_move_later = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, folder_ids, index)| index + 1 < folder_ids.len());
    let input_width = (sidebar_width - Spacing::LG * 2.0).max(80.0);
    let mut actions = column![
        text("Folder")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_secondary),
        text_input("Folder name", &app.folder_rename_input)
            .on_input(Message::FolderRenameInputChanged)
            .on_submit(Message::RenameSelectedFolder)
            .id(Id::new(LIBRARY_FOLDER_RENAME_INPUT_ID))
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(input_width),
        row![
            sidebar_action_button("Rename", tokens).on_press(Message::RenameSelectedFolder),
            sidebar_action_button("Delete", tokens).on_press(Message::RequestDeleteSelectedFolder),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
        row![
            maybe_sidebar_action_button(
                "Earlier",
                tokens,
                can_move_earlier,
                Message::MoveSelectedFolderEarlier,
            ),
            maybe_sidebar_action_button(
                "Later",
                tokens,
                can_move_later,
                Message::MoveSelectedFolderLater,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::SM);

    if has_parent {
        actions = actions.push(
            sidebar_action_button("Move to root", tokens)
                .on_press(Message::MoveSelectedFolderToRoot)
                .width(Length::Fill),
        );
    }
    if has_grandparent {
        actions = actions.push(
            sidebar_action_button("Move up", tokens)
                .on_press(Message::MoveSelectedFolderUp)
                .width(Length::Fill),
        );
    }

    Some(
        container(actions)
            .width(Length::Fill)
            .padding([Spacing::SM, Spacing::MD])
            .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
            .into(),
    )
}

fn view_tag_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let all_tags = app.all_tags();
    let mut tags = column![
        file_tree_row(
            "All tags",
            Some(format_count(app.library_entries.len(), "PDF")),
            0,
            app.active_tag_filter.is_none(),
            !all_tags.is_empty(),
            true,
            Message::TagFilterChanged(None),
            Message::TagFilterChanged(None),
            sidebar_width,
            tokens,
            false,
        ),
        section_heading("Tags", tokens),
    ]
    .spacing(Spacing::SM);

    for tag in all_tags {
        let count = app
            .library_entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == &tag))
            .count();
        let active = app.active_tag_filter.as_ref() == Some(&tag);
        tags = tags.push(file_tree_row(
            tag.clone(),
            Some(format_count(count, "PDF")),
            1,
            active,
            false,
            false,
            Message::TagFilterChanged(Some(tag.clone())),
            Message::TagFilterChanged(Some(tag)),
            sidebar_width,
            tokens,
            false,
        ));
    }

    tags.into()
}

fn view_selected_pdf_sidebar<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let title = entry_title(&entry);
    let author = entry_author(&entry);
    let path_label = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown file");
    let folder_label = if entry.folders.is_empty() {
        String::from("No folders")
    } else {
        entry
            .folders
            .iter()
            .map(|folder| folder.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let tags_label = if entry.tags.is_empty() {
        String::from("No tags")
    } else {
        entry.tags.join(", ")
    };
    let progress_label = selected_pdf_progress_label(&entry);
    let status_label = if entry.missing {
        "Missing file"
    } else {
        "Available"
    };
    let duplicate_label = duplicate_status_label(app, &entry);
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("PDF Details", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let mut content = column![
        heading,
        thumbnail_element(app, &entry.id, tokens, details_width.min(160.0), 1.0),
        text(truncate_for_width(&title, details_width, 0.0))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
        text(truncate_for_width(&author, details_width, 0.0))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
        sidebar_detail_row("Status", status_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Pages", page_count_label(&entry), details_width, tokens),
        sidebar_detail_row("Progress", progress_label, details_width, tokens),
        sidebar_detail_row("Size", file_size_label(&entry), details_width, tokens),
        sidebar_detail_row("Duplicates", duplicate_label, details_width, tokens),
        sidebar_detail_row("Opened", last_opened_label(&entry), details_width, tokens),
        sidebar_detail_row(
            "Added",
            format!("Added {}", entry.added_at.format("%b %-d, %Y")),
            details_width,
            tokens
        ),
        sidebar_detail_row("File", path_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Folders", folder_label, details_width, tokens),
        sidebar_detail_row("Tags", tags_label, details_width, tokens),
        sidebar_action_button("Open PDF", tokens)
            .on_press(Message::OpenLibraryEntry(entry.id.clone())),
        sidebar_action_button("Reveal in file manager", tokens)
            .on_press(Message::RevealEntryInFileManager(entry.id.clone())),
        sidebar_action_button("Open containing folder", tokens)
            .on_press(Message::OpenEntryContainingFolder(entry.id.clone())),
    ];
    if entry.missing {
        content = content.push(
            sidebar_action_button("Relink missing file", tokens)
                .on_press(Message::RelinkMissingEntry(entry.id.clone())),
        );
    }
    let content = content
        .push(
            sidebar_action_button("Clear selection", tokens)
                .on_press(Message::ClearLibrarySelection),
        )
        .spacing(Spacing::SM)
        .padding(Spacing::MD);

    container(
        scrollable(content)
            .height(Length::Fill)
            .style(move |_, status| scrollable_style(tokens, Class::Sidebar, status)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

fn view_multi_selection_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let selected_entries = app.selected_entries();
    let selected_count = selected_entries.len();
    let total_pages: u32 = selected_entries
        .iter()
        .filter_map(|entry| entry.page_count.map(u32::from))
        .sum();
    let missing_count = selected_entries
        .iter()
        .filter(|entry| entry.missing)
        .count();
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("Selection", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let content = column![
        heading,
        text(format_count(selected_count, "PDF"))
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary),
        sidebar_detail_row(
            "Known pages",
            if total_pages == 0 {
                String::from("Unknown")
            } else {
                total_pages.to_string()
            },
            details_width,
            tokens,
        ),
        sidebar_detail_row(
            "Missing files",
            missing_count.to_string(),
            details_width,
            tokens,
        ),
        sidebar_action_button("Clear selection", tokens).on_press(Message::ClearLibrarySelection),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(content)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
        .into()
}

fn sidebar_detail_row<'a>(
    label: &'a str,
    value: String,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary),
            text(truncate_for_width(&value, width, 0.0))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

fn selected_pdf_progress_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || format!("Page {}", u32::from(entry.last_page) + 1),
        |page_count| {
            let current_page = entry.last_page.saturating_add(1).min(page_count.max(1));
            format!(
                "{} of {} ({:.0}%)",
                current_page,
                page_count,
                f32::from(current_page) / f32::from(page_count.max(1)) * 100.0
            )
        },
    )
}

fn duplicate_status_label(app: &PDFolioApp, entry: &LibraryEntry) -> String {
    let duplicate_count = app
        .library_entries
        .iter()
        .filter(|candidate| candidate.id == entry.id)
        .count()
        .saturating_sub(1);
    duplicate_status_label_for_count(duplicate_count)
}

fn duplicate_status_label_for_count(duplicate_count: usize) -> String {
    if duplicate_count == 0 {
        String::from("Unique content hash")
    } else {
        format_count(duplicate_count, "matching duplicate")
    }
}

fn folder_sidebar_rows<'a>(
    app: &'a PDFolioApp,
    parent_id: Option<&'a FolderId>,
    depth: usize,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(0);
    let mut children: Vec<&Folder> = app
        .library_folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == parent_id)
        .collect();
    children.sort_by_key(|folder| (folder.manual_order, folder.name.to_lowercase()));

    for folder in children {
        let has_children = app
            .library_folders
            .iter()
            .any(|child| child.parent_id.as_ref() == Some(&folder.id));
        let expanded = !app.collapsed_library_tree_folders.contains(&folder.id);
        let active = app.selected_folder.as_ref() == Some(&folder.id);
        let drop_active = app.active_folder_drop_target() == Some(&folder.id);
        let flash_active = app.folder_drop_flash_active(&folder.id);
        let counts = app.folder_smart_counts(Some(&folder.id));
        let row = file_tree_row(
            &folder.name,
            Some(folder_sidebar_count_label(counts)),
            depth,
            active,
            has_children,
            expanded,
            Message::ToggleLibraryTreeFolder(folder.id.clone()),
            Message::FolderSelected(Some(folder.id.clone())),
            sidebar_width,
            tokens,
            drop_active || flash_active,
        );
        rows = rows.push(
            mouse_area(row)
                .on_enter(Message::FolderDropTargetChanged(Some(folder.id.clone())))
                .on_exit(Message::FolderDropTargetChanged(None))
                .on_press(Message::BeginFolderDrag(folder.id.clone()))
                .on_release(Message::EndFolderDrag),
        );
        if expanded {
            rows = rows.push(folder_sidebar_rows(
                app,
                Some(&folder.id),
                depth.saturating_add(1),
                sidebar_width,
                tokens,
            ));
        }
    }

    rows.into()
}

fn file_tree_row<'a>(
    label: impl Into<String>,
    meta: Option<String>,
    depth: usize,
    active: bool,
    has_children: bool,
    expanded: bool,
    toggle_message: Message,
    message: Message,
    sidebar_width: f32,
    tokens: ThemeTokens,
    drop_active: bool,
) -> Element<'a, Message> {
    let label = label.into();
    let file_tree_style = tokens.class_styles[Class::FileTree.index()];
    let fold_button_component = tokens.class_styles[Class::FileTreeFoldButton.index()];
    let fold_button_layout = fold_button_component.layout;
    let fold_button_normal_style = fold_button_component.resolve(ComponentState::Normal);
    let fold_button_hovered_style = fold_button_component.resolve(ComponentState::Hovered);
    let normal_style = file_tree_style.resolve(ComponentState::Normal);
    let active_style = file_tree_style.resolve(ComponentState::Active);
    let content_background = normal_style
        .background
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let indent = (depth as f32 * 12.0).min(72.0);
    let meta_width = meta
        .as_ref()
        .map_or(0.0, |value| (value.len() as f32 * 6.0).clamp(52.0, 128.0));
    let label_width = (sidebar_width - indent - meta_width - 34.0).max(52.0);
    let text_color = if active || drop_active {
        active_style.text_color.unwrap_or(tokens.text_primary)
    } else {
        normal_style.text_color.unwrap_or(tokens.text_secondary)
    };

    let chevron: Element<'_, Message> = if has_children {
        let icon = Svg::new(iced::widget::svg::Handle::from_memory(if expanded {
            CHEVRON_DOWN_SVG
        } else {
            CHEVRON_RIGHT_SVG
        }))
        .width(13.0)
        .height(13.0)
        .style(move |_, status| iced::widget::svg::Style {
            color: Some(match status {
                iced::widget::svg::Status::Hovered => fold_button_hovered_style
                    .text_color
                    .unwrap_or(tokens.text_primary),
                iced::widget::svg::Status::Idle => fold_button_normal_style
                    .text_color
                    .unwrap_or(tokens.text_secondary),
            }),
        });

        button(
            container(icon)
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill),
        )
        .width(fold_button_layout.width.unwrap_or(16.0))
        .height(fold_button_layout.height.unwrap_or(20.0))
        .padding(fold_button_layout.padding_top(0.0))
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::FileTreeFoldButton, status)
        })
        .on_press(toggle_message)
        .into()
    } else {
        container("").width(16.0).height(20.0).into()
    };

    let mut content = row![
        container("").width(indent),
        chevron,
        text(file_tree_label(&label, label_width))
            .size(FILE_TREE_LABEL_SIZE)
            .line_height(1.12)
            .font(file_tree_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(text_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    if let Some(meta) = meta {
        content = content.push(
            text(meta)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
        );
    }

    let row_button = button(content)
        .height(FILE_TREE_ROW_HEIGHT)
        .width(Length::Fill)
        .padding([3.0, Spacing::SM])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            let state = if active || drop_active {
                ComponentState::Active
            } else if hovered {
                ComponentState::Hovered
            } else {
                ComponentState::Normal
            };
            let mut style = crate::style::button_style(tokens, Class::FileTree, status);
            apply_file_tree_state_style(&mut style, tokens, state, content_background);
            if drop_active {
                let drop_style = crate::style::button_style(
                    tokens,
                    Class::FolderDropTarget,
                    button::Status::Active,
                );
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .on_press(message);

    if active || drop_active {
        if let Some(border) = side_border_for_class(tokens, Class::FileTree, ComponentState::Active)
        {
            side_border(row_button, border)
        } else {
            row_button.into()
        }
    } else {
        row_button.into()
    }
}

fn apply_file_tree_state_style(
    style: &mut button::Style,
    tokens: ThemeTokens,
    state: ComponentState,
    fallback_background: Color,
) {
    let state_style = tokens.class_styles[Class::FileTree.index()].resolve(state);
    style.background = Some(iced::Background::Color(
        state_style.background.unwrap_or(fallback_background),
    ));
    if let Some(text_color) = state_style.text_color {
        style.text_color = text_color;
    }
    if let Some(border_color) = state_style.border_color {
        style.border.color = border_color;
    }
    if let Some(border_width) = state_style.border_width {
        style.border.width = border_width;
    }
    if state_style.border.is_some() {
        style.border.width = 0.0;
    }
    if let Some(radius) = state_style.radius {
        style.border.radius = radius.into();
    }
}

fn sidebar_chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(18.0)
        .height(18.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_primary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(28.0)
    .height(28.0)
    .padding(0)
    .style(move |_, status| crate::style::button_style(tokens, Class::SidebarToggleButton, status))
    .on_press(message);

    tooltip(
        button,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn sidebar_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::SidebarActionButton, status))
}

fn maybe_sidebar_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    enabled: bool,
    message: Message,
) -> iced::widget::Button<'a, Message> {
    let button = button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(if enabled {
                tokens.text_primary
            } else {
                with_alpha(tokens.text_secondary, 0.55)
            }),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::SidebarActionButton, status));

    if enabled {
        button.on_press(message)
    } else {
        button
    }
}

fn library_layout_toggle_button(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let (icon, tooltip_label) = if app.compact_view_mode {
        (GRID_LAYOUT_SVG, "Switch to grid")
    } else {
        (LIST_LAYOUT_SVG, "Switch to list")
    };
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(18.0)
        .height(18.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_primary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(34.0)
    .height(34.0)
    .padding(0)
    .style(move |_, status| crate::style::button_style(tokens, Class::LibraryViewToggle, status))
    .on_press(Message::ToggleViewMode);

    tooltip(
        button,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn library_grid_zoom_control<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let control = row![
        text("Grid")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
        slider(
            LIBRARY_GRID_ZOOM_MIN..=app.library_grid_zoom_max(),
            app.library_grid_zoom(),
            Message::LibraryGridZoomChanged,
        )
        .step(LIBRARY_GRID_ZOOM_STEP)
        .width(150.0)
        .style(move |_, status| slider_style(tokens, Class::LibraryGridZoomSlider, status)),
        text(app.library_grid_zoom_label())
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .width(44.0),
    ]
    .spacing(Spacing::SM)
    .align_y(iced::Alignment::Center);

    tooltip(
        control,
        container(
            text("Grid zoom")
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn library_new_folder_button<'a>(tokens: ThemeTokens) -> iced::widget::Button<'a, Message> {
    button(
        text("New folder")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::LibraryImportButton, status))
}

fn library_drop_zone_card<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(app.library_card_font_size(FontSize::SM))
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
    )
    .width(app.library_grid_card_width())
    .height(app.library_card_estimated_height(&EntryId::new("__drop_zone__")))
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

fn library_drop_zone_row<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
    )
    .width(Length::Fill)
    .height(app.layout().library_list_row_height)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

fn library_metadata_density_picker<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    pick_list(
        LIBRARY_METADATA_DENSITY_OPTIONS,
        Some(app.library_metadata_density),
        Message::LibraryMetadataDensityChanged,
    )
    .placeholder("Metadata")
    .width(130.0)
    .padding([Spacing::SM, Spacing::MD])
    .text_size(FontSize::MD)
    .font(ui_font(FontWeight::MEDIUM))
    .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
    .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown))
    .into()
}

fn library_entry_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let author = entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"));
    let opened = last_opened_label(&entry);
    let metadata_label = library_card_metadata_label(app.library_metadata_density, &entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let activity_is_match = search_match.is_some();
    let activity_label = search_match.unwrap_or(opened);
    let activity_color = if activity_is_match {
        accent
    } else {
        text_secondary
    };
    let progress_value = progress_fraction(&entry);
    let media = card_thumbnail_media(app, &entry_id, tokens, content_alpha);
    let title_font_size = app.library_card_title_font_size();
    let metadata_font_size = app.library_card_font_size(FontSize::SM);
    let text_width = app.library_card_title_width();
    let author = truncate_for_width_with_font(&author, text_width, 0.0, metadata_font_size);
    let metadata_label = metadata_label
        .map(|label| truncate_for_width_with_font(&label, text_width, 0.0, metadata_font_size));
    let activity_label =
        truncate_for_width_with_font(&activity_label, text_width, 0.0, metadata_font_size);
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = LIBRARY_CARD_HOVER_LIFT * (1.0 - hover_progress);
    let bottom_lift_space = LIBRARY_CARD_HOVER_LIFT * hover_progress;
    let mut info = column![
        truncated_title(title, text_width, tokens, content_alpha, title_font_size),
        text(author)
            .size(metadata_font_size)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary)
            .wrapping(Wrapping::None),
    ]
    .spacing(app.library_card_spacing())
    .padding(app.library_card_padding())
    .height(app.library_card_info_height())
    .width(Length::Fill);
    if let Some(metadata_label) = metadata_label {
        info = info.push(
            text(metadata_label)
                .size(metadata_font_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(text_secondary)
                .wrapping(Wrapping::None),
        );
    }
    info = info
        .push(
            text(activity_label)
                .size(metadata_font_size)
                .font(ui_font(if activity_is_match {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::REGULAR
                }))
                .color(activity_color)
                .wrapping(Wrapping::None),
        )
        .push(progress_bar(progress_value, tokens));

    if mode == LibraryEntryRenderMode::Normal && app.tag_entry_id.as_ref() == Some(&entry_id) {
        info = info.push(
            text_input("Tag", &app.tag_input)
                .on_input(Message::TagInputChanged)
                .on_submit(Message::SubmitTag),
        );
    }
    let checkbox_visible = selected
        || !app.selected_library_entries.is_empty()
        || app.library_card_hover_progress(&entry_id) > 0.01;
    let media = if mode == LibraryEntryRenderMode::Normal && checkbox_visible {
        stack![
            media,
            container(selection_checkbox(
                selected,
                tokens,
                Message::EntryCheckboxToggled(entry_id.clone())
            ))
            .padding(Spacing::SM)
            .width(Length::Shrink)
            .height(Length::Shrink),
        ]
        .into()
    } else {
        media
    };
    let body = column![media, info].spacing(0).width(Length::Fill);
    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(app.library_grid_card_width())
    } else {
        Length::Fixed(app.library_grid_card_width())
    };
    let surface = container(body).width(width).clip(true).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryCard, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app.library_drag.as_ref().is_some_and(|drag| drag.active) {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

fn library_entry_row<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let details = library_row_metadata_label(app.library_metadata_density, &entry);
    let tags = entry.tags.clone();
    let progress_value = progress_fraction(&entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = LIBRARY_ROW_HOVER_LIFT * (1.0 - hover_progress);
    let bottom_lift_space = LIBRARY_ROW_HOVER_LIFT * hover_progress;
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let mut detail_column = column![
        truncated_title(
            title,
            app.layout().library_row_title_width,
            tokens,
            content_alpha,
            16
        ),
        text(details)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary),
    ]
    .spacing(Spacing::XS)
    .width(Length::Fill);
    if let Some(match_label) = search_match {
        detail_column = detail_column.push(
            text(match_label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(accent),
        );
    }
    detail_column = detail_column.push(if mode != LibraryEntryRenderMode::Normal {
        ghost_tags_row(tags, tokens, content_alpha)
    } else {
        tags_row(entry_id.clone(), tags, tokens)
    });
    let checkbox_lane: Element<'a, Message> = if mode == LibraryEntryRenderMode::Normal
        && (selected
            || !app.selected_library_entries.is_empty()
            || app.library_card_hover_progress(&entry_id) > 0.01)
    {
        selection_checkbox(
            selected,
            tokens,
            Message::EntryCheckboxToggled(entry_id.clone()),
        )
        .into()
    } else {
        container("").width(Length::Fixed(24.0)).into()
    };
    let row_content = row![
        checkbox_lane,
        thumbnail_element(
            app,
            &entry_id,
            tokens,
            app.layout().library_row_thumbnail_width,
            content_alpha
        ),
        detail_column,
        column![progress_bar(progress_value, tokens),]
            .spacing(Spacing::XS)
            .width(app.layout().library_row_progress_width),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(720.0)
    } else {
        Length::Fill
    };
    let surface = container(row_content).width(width).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryRow, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app.library_drag.as_ref().is_some_and(|drag| drag.active) {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

fn library_entry_container_style(
    tokens: ThemeTokens,
    class: Class,
    mode: LibraryEntryRenderMode,
    selected: bool,
    hover_progress: f32,
) -> iced::widget::container::Style {
    let mut style = container_style(tokens, class);
    match mode {
        LibraryEntryRenderMode::Normal => {
            let hover_progress = hover_progress.clamp(0.0, 1.0);
            let normal_style = tokens.class_styles[class.index()].resolve(ComponentState::Normal);
            let hovered_style = tokens.class_styles[class.index()].resolve(ComponentState::Hovered);
            let normal_background = normal_style
                .background
                .or_else(|| {
                    style.background.and_then(|background| match background {
                        iced::Background::Color(color) => Some(color),
                        _ => None,
                    })
                })
                .unwrap_or(tokens.surface_raised);
            let hovered_background = hovered_style
                .background
                .unwrap_or_else(|| mix_color(normal_background, tokens.accent, 0.14));
            let normal_border = normal_style.border_color.unwrap_or(style.border.color);
            let hovered_border = hovered_style
                .border_color
                .unwrap_or_else(|| mix_color(normal_border, tokens.accent, 0.42));

            if !selected && hover_progress > 0.0 {
                style.background = Some(iced::Background::Color(mix_color(
                    normal_background,
                    hovered_background,
                    hover_progress,
                )));
                style.border.color = mix_color(normal_border, hovered_border, hover_progress);
            }

            style.shadow = iced::Shadow {
                color: with_alpha(tokens.shadow, 0.20 + 0.10 * hover_progress),
                offset: iced::Vector::new(0.0, 1.0 + 4.0 * hover_progress),
                blur_radius: 7.0 + 7.0 * hover_progress,
            };
            if selected {
                let selected_style =
                    tokens.class_styles[class.index()].resolve(ComponentState::Selected);
                if let Some(background) = selected_style.background {
                    style.background = Some(iced::Background::Color(background));
                }
                if let Some(border_color) = selected_style.border_color {
                    style.border.color = border_color;
                }
                if let Some(border_width) = selected_style.border_width {
                    style.border.width = border_width;
                }
                style.shadow = iced::Shadow {
                    color: with_alpha(tokens.shadow, 0.24 + 0.10 * hover_progress),
                    offset: iced::Vector::new(0.0, 2.0 + 4.0 * hover_progress),
                    blur_radius: 9.0 + 7.0 * hover_progress,
                };
            }
        }
        LibraryEntryRenderMode::Placeholder => {
            let mut background = mix_color(tokens.surface, tokens.background, 0.55);
            background.a = 0.28;
            style.background = Some(iced::Background::Color(background));
            style.border.color = mix_color(tokens.border, tokens.background, 0.62);
        }
        LibraryEntryRenderMode::Floating => {
            style.background = Some(iced::Background::Color(tokens.surface_raised));
            style.border.color = tokens.focus;
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 10.0),
                blur_radius: 18.0,
            };
        }
    }
    style
}

fn library_entry_content_alpha(app: &PDFolioApp, mode: LibraryEntryRenderMode) -> f32 {
    if mode == LibraryEntryRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

fn card_thumbnail_media<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let width = app.library_grid_card_width();
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, app.thumbnail_size_for_grid_zoom()) {
        let height = (width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1)))
            .min(app.library_card_media_max_height());
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover)
                .border_radius(iced::border::bottom(crate::style::Radius::MD))
                .opacity(alpha),
        )
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    } else {
        container(document_preview_lines(
            width,
            app.library_card_media_max_height(),
            tokens,
            alpha,
        ))
        .center(width)
        .height(app.library_card_media_max_height())
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    }
}

fn document_preview_lines<'a>(
    width: f32,
    height: f32,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let line_widths = [0.68, 0.98, 0.78, 0.92, 0.54, 0.74, 0.98, 0.62];
    let mut lines = column![].spacing(7.0);
    for (index, fraction) in line_widths.into_iter().enumerate() {
        let color = if index == 0 {
            with_alpha(tokens.accent, alpha * 0.78)
        } else {
            with_alpha(tokens.text_secondary, alpha * 0.68)
        };
        lines = lines.push(
            container("")
                .width((width * fraction).max(12.0))
                .height(if index == 0 { 4.0 } else { 2.0 })
                .style(move |_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        radius: 1.0.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::container::Style::default()
                }),
        );
    }

    container(lines)
        .padding([14.0, 14.0])
        .width(width)
        .height(height)
        .into()
}

fn flush_media_style(tokens: ThemeTokens, alpha: f32) -> iced::widget::container::Style {
    let mut background = mix_color(tokens.background, tokens.surface_raised, 0.42);
    background.a *= alpha.clamp(0.0, 1.0);

    iced::widget::container::Style {
        background: Some(iced::Background::Color(background)),
        text_color: Some(with_alpha(tokens.text_secondary, alpha)),
        border: iced::Border {
            width: 0.0,
            color: with_alpha(tokens.border, alpha),
            radius: iced::border::top(crate::style::Radius::MD),
        },
        ..iced::widget::container::Style::default()
    }
}

fn thumbnail_element<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    width: f32,
    alpha: f32,
) -> Element<'a, Message> {
    let max_height = width * 1.32;
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, ThumbnailSize::Default) {
        let height = width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1));
        let display_height = height.min(max_height);
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .opacity(alpha),
        )
        .width(width)
        .height(display_height)
        .clip(true)
        .style(move |_| {
            let mut style = container_style(tokens, Class::PagePlaceholder);
            style.background = Some(iced::Background::Color(mix_color(
                tokens.background,
                tokens.surface_raised,
                0.42,
            )));
            style.border.color = mix_color(tokens.border, tokens.background, 0.28);
            if alpha < 1.0 {
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
            }
            style
        })
        .into()
    } else {
        container(
            text("PDF")
                .size(FontSize::SM)
                .color(with_alpha(tokens.text_secondary, alpha)),
        )
        .center(width)
        .height(max_height)
        .style(move |_| {
            let mut style = container_style(tokens, Class::PagePlaceholder);
            if alpha < 1.0 {
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
            }
            style
        })
        .into()
    }
}

fn tags_row<'a>(entry_id: EntryId, tags: Vec<String>, tokens: ThemeTokens) -> Element<'a, Message> {
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(
            tag_pill(tag.clone(), tokens).on_press(Message::TagFilterChanged(Some(tag.clone()))),
        );
    }
    row.push(tag_pill("+ tag", tokens).on_press(Message::StartTagEntry(entry_id)))
        .into()
}

fn ghost_tags_row<'a>(tags: Vec<String>, tokens: ThemeTokens, alpha: f32) -> Element<'a, Message> {
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(
            container(
                text(tag)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(with_alpha(tokens.text_secondary, alpha)),
            )
            .padding([Spacing::XS, Spacing::SM])
            .style(move |_| {
                let mut style = container_style(tokens, Class::TagPill);
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
                style
            }),
        );
    }
    row.into()
}

fn view_app_menu_bar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let labels = app.labels();
    let mut menus = row![]
        .spacing(2.0)
        .padding([0.0, Spacing::MD])
        .height(app.layout().app_menu_bar_height)
        .align_y(iced::Alignment::Center);

    for menu in APP_MENU_LABELS {
        let active = app.open_app_menu == Some(menu);
        menus = menus.push(app_menu_button(menu, active, tokens, labels));
    }

    let content: Element<'_, Message> =
        if app.mode == AppMode::Library && !app.selected_library_entries.is_empty() {
            column![menus, view_selection_context_row(app, tokens)]
                .spacing(0)
                .into()
        } else {
            menus.into()
        };

    container(content)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::MenuBar))
        .into()
}

fn app_menu_bar_height(app: &PDFolioApp) -> f32 {
    if app.mode == AppMode::Library && !app.selected_library_entries.is_empty() {
        app.layout().app_menu_bar_height + app.layout().selection_context_row_height
    } else {
        app.layout().app_menu_bar_height
    }
}

fn app_menu_button<'a>(
    menu: AppMenu,
    active: bool,
    tokens: ThemeTokens,
    labels: &'a crate::style::AppLabelTokens,
) -> Element<'a, Message> {
    button(
        container(
            text(app_menu_label(labels, menu))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(if active {
                    tokens.accent
                } else {
                    tokens.text_secondary
                })
                .wrapping(Wrapping::None),
        )
        .height(Length::Shrink)
        .center_y(Length::Shrink),
    )
    .padding([0.0, Spacing::MD])
    .height(24.0)
    .on_press(Message::AppMenuOpened(menu))
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::MenuButton, status);
        if active {
            style.background = Some(iced::Background::Color(tokens.surface));
            style.border.width = 0.0;
            style.text_color = tokens.accent;
        } else {
            style.border.width = 0.0;
        }
        style
    })
    .into()
}

fn app_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::AppMenuClosed),
    )
    .y(app_menu_bar_height(app))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn selection_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::SelectionMenuClosed),
    )
    .y(app_menu_bar_height(app))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_app_menu_dropdown(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(menu) = app.open_app_menu else {
        return container("").into();
    };
    let menu_index = APP_MENU_LABELS
        .iter()
        .position(|candidate| *candidate == menu)
        .unwrap_or(0);
    let x = 10.0 + menu_index as f32 * 76.0;

    pin(app_menu_panel(app, menu, tokens))
        .x(x)
        .y(app_menu_bar_height(app))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn app_menu_panel<'a>(
    app: &'a PDFolioApp,
    menu: AppMenu,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let labels = app.labels();
    let mut panel = column![].spacing(2.0).padding(Spacing::XS);
    match menu {
        AppMenu::File => {
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "OpenFile", "Open PDF..."),
                    "Ctrl+O",
                    true,
                    AppMenuAction::OpenFile,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportFolder", "Import Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshLibrary", "Refresh Library"),
                    "F5",
                    app.mode == AppMode::Library,
                    AppMenuAction::RefreshLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "BackToLibrary", "Back to Library"),
                    "Esc",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::BackToLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Edit => {
            let has_selection = !app.selected_library_entries.is_empty();
            let single_selection = app.selected_library_entries.len() == 1;
            let has_bulk_tag = has_selection && !app.bulk_tag_input.trim().is_empty();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "SelectAllVisible", "Select All Visible PDFs"),
                    "Ctrl+A",
                    app.mode == AppMode::Library,
                    AppMenuAction::SelectAllVisible,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ClearSelection", "Clear Selection"),
                    "Esc",
                    has_selection,
                    AppMenuAction::ClearSelection,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "SaveDetails", "Save Details"),
                    "Enter",
                    single_selection,
                    AppMenuAction::SaveDetails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetDetails", "Reset Details..."),
                    "",
                    single_selection,
                    AppMenuAction::ResetDetails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "AddTag", "Add Typed Tag"),
                    "",
                    has_bulk_tag,
                    AppMenuAction::AddTag,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RemoveTag", "Remove Typed Tag"),
                    "",
                    has_bulk_tag,
                    AppMenuAction::RemoveTag,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "DeleteFromLibrary", "Delete From Library..."),
                    "Delete",
                    has_selection,
                    AppMenuAction::DeleteFromLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::View => {
            panel = panel
                .push(app_menu_item(
                    if app.compact_view_mode {
                        app_menu_action_label(labels, "ToggleLayoutGrid", "Switch to Grid")
                    } else {
                        app_menu_action_label(labels, "ToggleLayoutList", "Switch to List")
                    },
                    "",
                    true,
                    AppMenuAction::ToggleLayout,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    match app.theme {
                        AppTheme::Light => {
                            app_menu_action_label(labels, "ToggleThemeDark", "Switch to Dark Theme")
                        }
                        AppTheme::Dark => app_menu_action_label(
                            labels,
                            "ToggleThemeLight",
                            "Switch to Light Theme",
                        ),
                    },
                    "",
                    true,
                    AppMenuAction::ToggleTheme,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ReloadStyles", "Reload Styles"),
                    "",
                    true,
                    AppMenuAction::ReloadStyles,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    if app.toc_open {
                        app_menu_action_label(labels, "ToggleTocHide", "Hide Table of Contents")
                    } else {
                        app_menu_action_label(labels, "ToggleTocShow", "Show Table of Contents")
                    },
                    "",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ToggleToc,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "JumpToPage", "Jump to Page..."),
                    "Ctrl+G",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::JumpToPage,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomIn", "Zoom In"),
                    "Ctrl++",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomIn,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomOut", "Zoom Out"),
                    "Ctrl+-",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomOut,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetZoom", "Reset Zoom"),
                    "Ctrl+0",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ResetZoom,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Document => {
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "JumpToPage", "Jump to Page..."),
                    "Ctrl+G",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::JumpToPage,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    if app.toc_open {
                        app_menu_action_label(labels, "ToggleTocHide", "Hide Table of Contents")
                    } else {
                        app_menu_action_label(labels, "ToggleTocShow", "Show Table of Contents")
                    },
                    "",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ToggleToc,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomIn", "Zoom In"),
                    "Ctrl++",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomIn,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomOut", "Zoom Out"),
                    "Ctrl+-",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomOut,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetZoom", "Reset Zoom"),
                    "Ctrl+0",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ResetZoom,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Library => {
            let has_selection = !app.selected_library_entries.is_empty();
            let has_active_folder = app.selected_folder.is_some();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportFolder", "Import Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshLibrary", "Refresh Library"),
                    "F5",
                    app.mode == AppMode::Library,
                    AppMenuAction::RefreshLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "CreateFolder", "New Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::CreateFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "AddToFolder", "Add Selection to Current Folder"),
                    "",
                    has_selection && has_active_folder,
                    AppMenuAction::AddToFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(
                        labels,
                        "RemoveFromFolder",
                        "Remove Selection from Current Folder",
                    ),
                    "",
                    has_selection && has_active_folder,
                    AppMenuAction::RemoveFromFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens));
            for sort_mode in LIBRARY_SORT_OPTIONS {
                panel = panel.push(app_menu_item(
                    sort_mode.label(),
                    if app.library_sort_mode == sort_mode {
                        label_text(labels, "sort_selected", "Selected")
                    } else {
                        ""
                    },
                    app.mode == AppMode::Library,
                    AppMenuAction::SortLibrary(sort_mode),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
            }
        }
        AppMenu::Tools => {
            let has_selection = !app.selected_library_entries.is_empty();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "SortTitles", "Apply Title Sort Cleanup"),
                    "",
                    has_selection,
                    AppMenuAction::SortTitles,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshMetadata", "Refresh PDF Metadata"),
                    "",
                    has_selection,
                    AppMenuAction::RefreshMetadata,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetMetadata", "Reset Display Metadata..."),
                    "",
                    has_selection,
                    AppMenuAction::ResetMetadata,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RebuildThumbnails", "Rebuild Thumbnails"),
                    "",
                    has_selection,
                    AppMenuAction::RebuildThumbnails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "Reindex", "Reindex Full Text"),
                    "",
                    has_selection,
                    AppMenuAction::Reindex,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Help => {
            panel = panel
                .push(app_menu_static_item(
                    label_text(labels, "help_product_name", "PDF-Folio"),
                    label_text(
                        labels,
                        "help_product_detail",
                        "Local PDF library and reader",
                    ),
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_static_item(
                    label_text(labels, "help_status_label", "Status"),
                    label_text(
                        labels,
                        "help_status_detail",
                        "No help actions available yet",
                    ),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
    }

    container(panel)
        .width(app.layout().app_menu_panel_width)
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuPanel);
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 18.0,
            };
            style
        })
        .into()
}

fn app_menu_item<'a>(
    label: &'a str,
    shortcut: &'a str,
    enabled: bool,
    action: AppMenuAction,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let label_color = if enabled {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let shortcut_color = if enabled {
        tokens.text_secondary
    } else {
        with_alpha(tokens.text_secondary, 0.58)
    };
    let content = row![
        text(label)
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(label_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
        text(shortcut)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(shortcut_color)
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center);

    if enabled {
        button(content)
            .height(item_height)
            .width(Length::Fill)
            .padding([Spacing::XS, Spacing::MD])
            .on_press(Message::AppMenuActionSelected(action))
            .style(move |_, status| crate::style::button_style(tokens, Class::MenuItem, status))
            .into()
    } else {
        container(content)
            .height(item_height)
            .width(Length::Fill)
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| {
                let mut style = container_style(tokens, Class::MenuItem);
                style.background = Some(iced::Background::Color(tokens.surface_raised));
                style
            })
            .into()
    }
}

fn app_menu_static_item<'a>(
    label: &'a str,
    detail: &'a str,
    tokens: ThemeTokens,
    _item_height: f32,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary),
            text(detail)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| {
        let mut style = container_style(tokens, Class::MenuItem);
        style.background = Some(iced::Background::Color(tokens.surface_raised));
        style
    })
    .into()
}

fn view_selection_context_row(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let selected_count = app.selected_library_entries.len();
    let title_input_width = selection_title_input_width(app);
    let author_input_width = selection_author_input_width(app);
    let tag_input_width = selection_tag_input_width(app);
    let selected_label = text(format!("{selected_count} selected"))
        .size(FontSize::CONTROL)
        .font(ui_font(FontWeight::SEMIBOLD))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    let mut controls = row![]
        .spacing(Spacing::SM)
        .padding([Spacing::SM, Spacing::MD])
        .height(app.layout().selection_context_row_height)
        .align_y(iced::Alignment::Center)
        .push(selected_label)
        .push(toolbar_button("Clear", tokens).on_press(Message::ClearLibrarySelection));

    if selected_count == 1 {
        controls = controls
            .push(
                text_input("Title", &app.details_title_input)
                    .on_input(Message::DetailsTitleChanged)
                    .on_submit(Message::SaveDetailsMetadata)
                    .id(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID))
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(title_input_width),
            )
            .push(
                text_input("Author", &app.details_author_input)
                    .on_input(Message::DetailsAuthorChanged)
                    .on_submit(Message::SaveDetailsMetadata)
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(author_input_width),
            )
            .push(toolbar_button("Save", tokens).on_press(Message::SaveDetailsMetadata))
            .push(selection_menu_button(
                "More",
                SelectionMenu::More,
                app.open_selection_menu == Some(SelectionMenu::More),
                tokens,
            ));
    } else {
        controls = controls
            .push(
                text_input("Tag", &app.bulk_tag_input)
                    .on_input(Message::BulkTagInputChanged)
                    .on_submit(Message::BulkAddTag)
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(tag_input_width),
            )
            .push(selection_menu_button(
                "Tags",
                SelectionMenu::Tags,
                app.open_selection_menu == Some(SelectionMenu::Tags),
                tokens,
            ))
            .push(selection_menu_button(
                "Folders",
                SelectionMenu::Folders,
                app.open_selection_menu == Some(SelectionMenu::Folders),
                tokens,
            ))
            .push(selection_menu_button(
                "Metadata",
                SelectionMenu::Metadata,
                app.open_selection_menu == Some(SelectionMenu::Metadata),
                tokens,
            ))
            .push(selection_menu_button(
                "Maintenance",
                SelectionMenu::Maintenance,
                app.open_selection_menu == Some(SelectionMenu::Maintenance),
                tokens,
            ));
    }

    controls = controls.push(
        text("PDF-Folio")
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_secondary)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .wrapping(Wrapping::None),
    );

    container(controls)
        .width(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuBar);
            style.background = Some(iced::Background::Color(mix_color(
                tokens.surface,
                tokens.surface_raised,
                0.48,
            )));
            style
        })
        .into()
}

fn selection_menu_button<'a>(
    label: &'a str,
    menu: SelectionMenu,
    active: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    button(
        row![
            text(label)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
            text("v")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding([Spacing::SM, Spacing::MD])
    .height(30.0)
    .on_press(Message::SelectionMenuOpened(menu))
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::MenuButton, status);
        if active {
            style.background = Some(iced::Background::Color(mix_color(
                tokens.surface_raised,
                tokens.accent,
                0.26,
            )));
            style.border.color = tokens.focus;
        }
        style
    })
    .into()
}

fn view_selection_menu_dropdown(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(menu) = app.open_selection_menu else {
        return container("").into();
    };
    pin(selection_menu_panel(app, menu, tokens))
        .x(selection_menu_x(app, menu))
        .y(app_menu_bar_height(app))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn selection_menu_x(app: &PDFolioApp, menu: SelectionMenu) -> f32 {
    let base = Spacing::MD + 128.0;
    if app.selected_library_entries.len() == 1 {
        return base + selection_title_input_width(app) + selection_author_input_width(app) + 88.0;
    }

    match menu {
        SelectionMenu::Tags => base + selection_tag_input_width(app),
        SelectionMenu::Folders => base + selection_tag_input_width(app) + 92.0,
        SelectionMenu::Metadata => base + selection_tag_input_width(app) + 202.0,
        SelectionMenu::Maintenance => base + selection_tag_input_width(app) + 330.0,
        SelectionMenu::More => base,
    }
}

fn selection_menu_panel<'a>(
    app: &'a PDFolioApp,
    menu: SelectionMenu,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let labels = app.labels();
    let actions: &'static [SelectionToolbarAction] = match menu {
        SelectionMenu::More => &SINGLE_MORE_ACTIONS,
        SelectionMenu::Tags => &BULK_TAG_ACTIONS,
        SelectionMenu::Folders => &BULK_FOLDER_ACTIONS,
        SelectionMenu::Metadata => &BULK_METADATA_ACTIONS,
        SelectionMenu::Maintenance => &BULK_MAINTENANCE_ACTIONS,
    };
    let mut panel = column![].spacing(2.0).padding(Spacing::XS);
    for action in actions {
        panel = panel.push(selection_menu_item(
            *action,
            tokens,
            labels,
            app.layout().app_menu_item_height,
        ));
    }

    container(panel)
        .width(app.layout().app_menu_panel_width)
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuPanel);
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 18.0,
            };
            style
        })
        .into()
}

fn app_menu_label<'a>(labels: &'a crate::style::AppLabelTokens, menu: AppMenu) -> &'a str {
    labels.get(LabelSection::AppMenu, app_menu_key(menu), menu.label())
}

fn app_menu_action_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    labels.get(LabelSection::AppMenuAction, key, fallback)
}

fn selection_toolbar_action_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    action: SelectionToolbarAction,
) -> &'a str {
    labels.get(
        LabelSection::SelectionToolbarAction,
        selection_toolbar_action_key(action),
        action.label(),
    )
}

fn library_sidebar_tab_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    tab: LibrarySidebarTab,
) -> &'a str {
    labels.get(
        LabelSection::LibrarySidebarTab,
        library_sidebar_tab_key(tab),
        tab.label(),
    )
}

fn label_text<'a>(
    labels: &'a crate::style::AppLabelTokens,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    labels.get(LabelSection::Text, key, fallback)
}

fn app_menu_key(menu: AppMenu) -> &'static str {
    match menu {
        AppMenu::File => "File",
        AppMenu::Edit => "Edit",
        AppMenu::View => "View",
        AppMenu::Document => "Document",
        AppMenu::Library => "Library",
        AppMenu::Tools => "Tools",
        AppMenu::Help => "Help",
    }
}

fn library_sidebar_tab_key(tab: LibrarySidebarTab) -> &'static str {
    match tab {
        LibrarySidebarTab::Files => "Files",
        LibrarySidebarTab::Tags => "Tags",
    }
}

fn selection_toolbar_action_key(action: SelectionToolbarAction) -> &'static str {
    match action {
        SelectionToolbarAction::AddTag => "AddTag",
        SelectionToolbarAction::RemoveTag => "RemoveTag",
        SelectionToolbarAction::AddToFolder => "AddToFolder",
        SelectionToolbarAction::RemoveFromFolder => "RemoveFromFolder",
        SelectionToolbarAction::SaveDetails => "SaveDetails",
        SelectionToolbarAction::ResetDetails => "ResetDetails",
        SelectionToolbarAction::SortTitles => "SortTitles",
        SelectionToolbarAction::RefreshMetadata => "RefreshMetadata",
        SelectionToolbarAction::ResetMetadata => "ResetMetadata",
        SelectionToolbarAction::RebuildThumbnails => "RebuildThumbnails",
        SelectionToolbarAction::Reindex => "Reindex",
        SelectionToolbarAction::DeleteMetadata => "DeleteMetadata",
    }
}

fn selection_menu_item(
    action: SelectionToolbarAction,
    tokens: ThemeTokens,
    labels: &crate::style::AppLabelTokens,
    item_height: f32,
) -> Element<'_, Message> {
    button(
        text(selection_toolbar_action_label(labels, action))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
    )
    .height(item_height)
    .width(Length::Fill)
    .padding([Spacing::XS, Spacing::MD])
    .on_press(Message::SelectionToolbarActionSelected(action))
    .style(move |_, status| crate::style::button_style(tokens, Class::MenuItem, status))
    .into()
}

fn app_menu_separator<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    container("")
        .height(1.0)
        .width(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuItem);
            style.background = Some(iced::Background::Color(mix_color(
                tokens.surface_raised,
                tokens.border,
                0.62,
            )));
            style
        })
        .into()
}

fn selection_title_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().selection_title_input_min_width,
        app.layout().selection_title_input_width,
        0.34,
    )
}

fn selection_author_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().selection_author_input_min_width,
        app.layout().selection_author_input_width,
        0.24,
    )
}

fn selection_tag_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().bulk_tag_input_min_width,
        app.layout().bulk_tag_input_width,
        0.2,
    )
}

fn responsive_selection_input_width(
    app: &PDFolioApp,
    min_width: f32,
    max_width: f32,
    viewport_fraction: f32,
) -> f32 {
    (app.library_viewport_width * viewport_fraction).clamp(min_width, max_width)
}

fn view_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let body: Element<'_, Message> = if app.outline.is_empty() {
        container(
            text("No table of contents")
                .size(FontSize::MD)
                .color(tokens.text_secondary),
        )
        .padding(Spacing::LG)
        .width(Length::Fill)
        .into()
    } else {
        scrollable(outline_list(
            &app.outline,
            0,
            Vec::new(),
            &app.expanded_outline_paths,
            tokens,
        ))
        .height(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::Sidebar, status))
        .into()
    };

    container(
        column![section_heading("Contents", tokens), body]
            .spacing(Spacing::SM)
            .padding(Spacing::MD),
    )
    .width(app.layout().viewer_sidebar_width)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::Sidebar))
    .into()
}

fn outline_list<'a>(
    nodes: &'a [OutlineNode],
    depth: u16,
    parent_path: Vec<usize>,
    expanded_paths: &'a HashSet<Vec<usize>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut list = column![].spacing(Spacing::XS);

    for (index, node) in nodes.iter().enumerate() {
        let mut path = parent_path.clone();
        path.push(index);
        let has_children = !node.children.is_empty();
        let is_expanded = expanded_paths.contains(&path);
        let label = if node.title.trim().is_empty() {
            String::from("Untitled")
        } else {
            node.title.clone()
        };
        let mut row = row![
            text(" ".repeat(usize::from(depth) * 2)),
            text(if has_children {
                if is_expanded {
                    "v"
                } else {
                    ">"
                }
            } else {
                " "
            })
            .size(FontSize::SM)
            .color(tokens.text_secondary),
            text(label).size(FontSize::MD).color(tokens.text_primary)
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center);
        if let Some(page) = node.page {
            row = row.push(
                text(format!("{}", u32::from(page) + 1))
                    .size(FontSize::SM)
                    .color(tokens.text_secondary),
            );
            let message = if has_children {
                Message::ToggleOutlineNode(path.clone())
            } else {
                Message::JumpToPage(page)
            };
            list = list.push(outline_button(row, message, tokens));
        } else {
            list = list.push(outline_button(
                row,
                Message::ToggleOutlineNode(path.clone()),
                tokens,
            ));
        }

        if has_children && is_expanded {
            list = list.push(outline_list(
                &node.children,
                depth.saturating_add(1),
                path,
                expanded_paths,
                tokens,
            ));
        }
    }

    list.into()
}

fn outline_button<'a>(
    content: impl Into<Element<'a, Message>>,
    message: Message,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    toc_entry(content, tokens).on_press(message)
}

fn view_jump_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let max_page = app.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page")
            .size(FontSize::CONTROL)
            .color(tokens.text_primary),
        text_input("Page", &app.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(app.layout().jump_input_width),
        text(format!("of {max_page}"))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        toolbar_button("Go", tokens).on_press(Message::SubmitJump),
        toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::MD)
    .align_y(iced::Alignment::Center);

    container(dialog)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::JumpOverlay))
        .into()
}

async fn render_page(doc: Arc<PdfDoc>, key: TileKey) -> anyhow::Result<(TileKey, RenderedPage)> {
    let page =
        tokio::task::spawn_blocking(move || doc.render_page(key.page, key.width_px)).await??;
    Ok((key, page))
}

async fn load_or_render_thumbnail(
    entry: LibraryEntry,
    size: ThumbnailSize,
) -> anyhow::Result<(EntryId, ThumbnailSize, RenderedPage)> {
    tokio::task::spawn_blocking(move || {
        let path = thumbnail_variant_path(&entry.id, size)?;
        if path.exists() {
            let data = std::fs::read(&path)?;
            let width = size.width_px();
            let height = (data.len() / (usize::from(width) * 4)).clamp(1, usize::from(u16::MAX));
            return Ok((
                entry.id,
                size,
                RenderedPage {
                    width,
                    height: height as u16,
                    rgba: data,
                },
            ));
        }

        let doc = PdfDoc::open(&entry.path)?;
        let page = doc.render_page(0, size.width_px())?;
        std::fs::write(path, &page.rgba)?;
        Ok((entry.id, size, page))
    })
    .await?
}

fn thumbnail_variant_path(entry_id: &EntryId, size: ThumbnailSize) -> anyhow::Result<PathBuf> {
    let default_path = thumbnail_path(entry_id)?;
    let Some(suffix) = size.cache_suffix() else {
        return Ok(default_path);
    };
    Ok(default_path.with_file_name(format!("{}.{}.rgba", entry_id.as_str(), suffix)))
}

fn open_document_task(path: PathBuf) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || PdfDoc::open(&path)).await? },
        |result| match result {
            Ok(doc) => Message::DocumentOpened(Arc::new(doc)),
            Err(error) => Message::DocumentError(error.to_string()),
        },
    )
}

fn open_library_document_task(entry_id: EntryId, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || PdfDoc::open(&path)).await? },
        move |result| match result {
            Ok(doc) => Message::LibraryDocumentOpened {
                entry_id: entry_id.clone(),
                doc: Arc::new(doc),
            },
            Err(error) => Message::DocumentError(error.to_string()),
        },
    )
}

fn open_file_manager_task(path: PathBuf, reveal: bool) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let Some((program, args)) = file_manager_command(&path, reveal) else {
                    anyhow::bail!(
                        "Could not determine a containing folder for {}.",
                        path.display()
                    );
                };
                let status = std::process::Command::new(&program).args(&args).status()?;
                if !status.success() {
                    anyhow::bail!("File manager command failed for {}.", path.display());
                }
                Ok::<_, anyhow::Error>(())
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

fn persist_manual_entry_order_task(db: Arc<Db>, entry_ids: Vec<EntryId>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.set_manual_entry_order(&entry_ids)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::ManualEntryOrderSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn persist_manual_folder_order_task(
    db: Arc<Db>,
    parent_id: Option<FolderId>,
    folder_ids: Vec<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.set_manual_folder_order(parent_id.as_ref(), &folder_ids)
            })
            .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn bulk_operation_task<F>(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    label: String,
    operation: F,
) -> Task<Message>
where
    F: Fn(&Db, &EntryId) -> anyhow::Result<()> + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match operation(&db, &entry_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                (label, updated, errors)
            })
            .await
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn rename_folder_task(db: Arc<Db>, folder_id: FolderId, name: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.rename_folder(&folder_id, &name)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn move_folder_task(
    db: Arc<Db>,
    folder_id: FolderId,
    new_parent_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.move_folder(&folder_id, new_parent_id.as_ref()))
                .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn delete_folder_task(db: Arc<Db>, folder_id: FolderId) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.delete_folder(&folder_id)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn assign_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: FolderId,
) -> Task<Message> {
    Task::perform(
        async move {
            let completed_folder_id = folder_id.clone();
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match db.add_entry_to_folder(&entry_id, &folder_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                Ok::<_, anyhow::Error>((
                    completed_folder_id,
                    String::from("Added to folder"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((folder_id, label, updated, errors)) => Message::FolderAssignmentFinished {
                folder_id,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn edit_metadata_task(
    db: Arc<Db>,
    entry: LibraryEntry,
    display_title: String,
    display_author: String,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.update_display_metadata(&entry.id, Some(&display_title), Some(&display_author))?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let label = format!("Saved metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn reset_metadata_task(db: Arc<Db>, entry: LibraryEntry) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.reset_display_metadata(&entry.id)?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let label = format!("Reset metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn relink_entry_task(db: Arc<Db>, entry_id: EntryId, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking({
                let entry_id = entry_id.clone();
                let path = path.clone();
                move || {
                    db.relink_entry_path(&entry_id, &path)?;
                    Ok::<_, anyhow::Error>((entry_id, path))
                }
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, path)) => Message::RelinkFinished { entry_id, path },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn bulk_reset_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for mut entry in entries {
                    entry.display_title = None;
                    entry.display_author = None;
                    entry.metadata_locked = false;
                    match db
                        .reset_display_metadata(&entry.id)
                        .and_then(|()| reindex_entry(&search_index, &entry))
                    {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Reset metadata for"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn bulk_refresh_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for mut entry in entries {
                    match refresh_entry_metadata(&db, &search_index, &mut entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Refreshed metadata for"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn refresh_entry_metadata(
    db: &Db,
    search_index: &SearchIndex,
    entry: &mut LibraryEntry,
) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let author = attributed_author(&doc);
    let page_count = Some(doc.page_count());
    db.update_author_attribution(&entry.id, author.as_deref())?;
    db.update_page_count_attribution(&entry.id, page_count)?;
    entry.author = author;
    entry.page_count = page_count;
    entry.author_attributed = true;
    entry.page_count_attributed = true;
    reindex_entry(search_index, entry)
}

fn bulk_delete_metadata_task(db: Arc<Db>, entry_ids: Vec<EntryId>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match db
                        .delete_entry(&entry_id)
                        .and_then(|()| search_index.delete_entry(entry_id.as_str()))
                    {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Deleted from library"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn bulk_thumbnail_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry in entries {
                    match rebuild_entry_thumbnail(&entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                (String::from("Rebuilt thumbnails for"), updated, errors)
            })
            .await
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn rebuild_entry_thumbnail(entry: &LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    for size in [
        ThumbnailSize::Small,
        ThumbnailSize::Default,
        ThumbnailSize::Large,
    ] {
        let path = thumbnail_variant_path(&entry.id, size)?;
        let page = doc.render_page(0, size.width_px())?;
        std::fs::write(path, &page.rgba)?;
    }
    Ok(())
}

fn bulk_reindex_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry in entries {
                    match reindex_entry(&search_index, &entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Reindexed"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn reindex_entry(search_index: &SearchIndex, entry: &LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let title = entry_title(entry);
    let author = entry_author(entry);
    let mut documents = Vec::with_capacity(usize::from(doc.page_count()));
    for page in 0..doc.page_count() {
        documents.push(IndexDocument {
            id: entry.id.as_str().to_owned(),
            title: title.clone(),
            author: author.clone(),
            body: doc.text_on_page(page).unwrap_or_default(),
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;
    Ok(())
}

fn import_folder_with_index(db: &Db, root: &std::path::Path) -> anyhow::Result<ImportSummary> {
    let paths = scan_pdf_files(root)?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        match import_pdf_with_index(db, path.clone()) {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    Ok(ImportSummary { entries, errors })
}

fn import_pdf_with_index(db: &Db, path: PathBuf) -> anyhow::Result<ImportedEntry> {
    let id = EntryId::new(hash_file(&path)?);
    let inserted = db.entry_by_path(&path)?.is_none();
    let doc = PdfDoc::open(&path)?;
    let title = attributed_title(&doc).or_else(|| title_from_path(&path));
    let page_count = doc.page_count();
    let author = attributed_author(&doc);

    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.clone(),
        title: title.clone(),
        author: author.clone(),
        author_attributed: true,
        page_count_attributed: true,
        page_count: Some(page_count),
        cover_hash: None,
    })?;

    let search_index = SearchIndex::open_default()?;
    let mut documents = Vec::with_capacity(usize::from(page_count));
    for page in 0..page_count {
        let body = doc.text_on_page(page).unwrap_or_default();
        documents.push(IndexDocument {
            id: id.as_str().to_owned(),
            title: title.clone().unwrap_or_default(),
            author: author.clone().unwrap_or_default(),
            body,
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;

    Ok(ImportedEntry { id, path, inserted })
}

fn attribute_pending_metadata_task(db: Arc<Db>) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || attribute_pending_metadata(&db)).await? },
        |result| match result {
            Ok(()) => Message::AuthorAttributionFinished,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn attribute_pending_metadata(db: &Db) -> anyhow::Result<()> {
    for entry in db.get_all_entries()?.into_iter().filter(|entry| {
        !entry.missing && (!entry.author_attributed || !entry.page_count_attributed)
    }) {
        let doc = open_entry_doc(&entry);
        if !entry.author_attributed {
            let author = doc.as_ref().and_then(attributed_author);
            db.update_author_attribution(&entry.id, author.as_deref())?;
        }
        if !entry.page_count_attributed {
            let page_count = doc.as_ref().map(|doc| doc.page_count());
            db.update_page_count_attribution(&entry.id, page_count)?;
        }
    }

    Ok(())
}

fn open_entry_doc(entry: &LibraryEntry) -> Option<PdfDoc> {
    entry
        .path
        .exists()
        .then(|| PdfDoc::open(&entry.path).ok())
        .flatten()
}

fn attributed_author(doc: &PdfDoc) -> Option<String> {
    doc.metadata_author()
        .ok()
        .flatten()
        .and_then(|author| clean_author_candidate(&author))
        .or_else(|| author_from_contents(doc))
}

fn attributed_title(doc: &PdfDoc) -> Option<String> {
    doc.metadata_title()
        .ok()
        .flatten()
        .and_then(clean_import_title)
}

fn title_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(clean_import_title)
}

fn clean_import_title(value: impl AsRef<str>) -> Option<String> {
    let title = value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() || title.eq_ignore_ascii_case("untitled") {
        None
    } else {
        Some(title.chars().take(512).collect())
    }
}

fn author_from_contents(doc: &PdfDoc) -> Option<String> {
    let pages_to_scan = doc.page_count().min(3);
    for page in 0..pages_to_scan {
        let text = doc.text_on_page(page).ok()?;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(author) = author_from_line(line) {
                return Some(author);
            }
        }
    }
    None
}

fn author_from_line(line: &str) -> Option<String> {
    let normalized = line.trim().trim_matches(['.', ',', ';', ':']);
    for prefix in ["Author:", "Authors:", "By:", "Written by "] {
        if let Some(candidate) = normalized.strip_prefix(prefix) {
            return clean_author_candidate(candidate);
        }
    }

    normalized
        .strip_prefix("By ")
        .and_then(clean_author_candidate)
}

fn clean_author_candidate(candidate: &str) -> Option<String> {
    let candidate = candidate
        .trim()
        .trim_matches(['.', ',', ';', ':', '-', ' '])
        .replace('\n', " ");
    let candidate = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = candidate.to_lowercase();
    let digit_count = candidate.chars().filter(|ch| ch.is_ascii_digit()).count();

    if candidate.len() < 2
        || candidate.len() > 80
        || lower == "anonymous"
        || lower == "unknown"
        || lower.contains("http")
        || lower.contains("www.")
        || lower.contains("copyright")
        || digit_count > 4
    {
        return None;
    }

    Some(candidate)
}

async fn search_library_task(
    db: Arc<Db>,
    query: String,
    sort_mode: LibrarySortMode,
) -> anyhow::Result<(Vec<LibraryEntry>, HashMap<EntryId, u16>)> {
    tokio::task::spawn_blocking(move || {
        let entries = db.get_entries_sorted(sort_mode)?;
        let normalized = query.trim().to_lowercase();
        let search_index = SearchIndex::open_default()?;
        let hits = search_index.search(&query, 200).unwrap_or_default();
        let mut hit_pages = HashMap::new();
        let mut ordered_entries = Vec::new();

        for hit in hits {
            let id = EntryId::new(hit.id);
            if hit_pages.contains_key(&id) {
                continue;
            }
            hit_pages.insert(id.clone(), hit.page.min(u64::from(u16::MAX)) as u16);
            if let Some(entry) = entries.iter().find(|entry| entry.id == id) {
                ordered_entries.push(entry.clone());
            }
        }

        for entry in entries {
            if hit_pages.contains_key(&entry.id) || !entry_matches_query(&entry, &normalized) {
                continue;
            }
            ordered_entries.push(entry);
        }

        Ok((ordered_entries, hit_pages))
    })
    .await?
}

fn apply_watch_event(db: &Db, event: LibraryWatchEvent) -> anyhow::Result<()> {
    match event {
        LibraryWatchEvent::PdfCreated(path) => {
            if path.exists() {
                import_pdf_with_index(db, path)?;
            }
        }
        LibraryWatchEvent::PdfRemoved(path) => {
            db.set_missing_by_path(&path, true)?;
        }
    }
    Ok(())
}

fn entry_matches_query(entry: &LibraryEntry, normalized_query: &str) -> bool {
    entry_search_fields_match(
        &entry_title(entry),
        &entry_author(entry),
        &entry.path.to_string_lossy(),
        entry.tags.iter().map(String::as_str),
        entry.folders.iter().map(|folder| folder.name.as_str()),
        normalized_query,
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

fn search_match_source_label(entry: &LibraryEntry, normalized_query: &str) -> Option<String> {
    search_match_source_label_for_fields(
        &entry_title(entry),
        &entry_author(entry),
        &entry.path.to_string_lossy(),
        entry.tags.iter().map(String::as_str),
        entry.folders.iter().map(|folder| folder.name.as_str()),
        normalized_query,
    )
}

fn search_match_source_label_for_fields<'a>(
    title: &str,
    author: &str,
    path: &str,
    tags: impl IntoIterator<Item = &'a str>,
    folder_names: impl IntoIterator<Item = &'a str>,
    normalized_query: &str,
) -> Option<String> {
    if normalized_query.is_empty() {
        return None;
    }

    if title.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in title"))
    } else if author.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in author"))
    } else if tags
        .into_iter()
        .any(|tag| tag.to_lowercase().contains(normalized_query))
    {
        Some(String::from("Match in tag"))
    } else if folder_names
        .into_iter()
        .any(|folder| folder.to_lowercase().contains(normalized_query))
    {
        Some(String::from("Match in folder"))
    } else if path.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in path"))
    } else {
        None
    }
}

fn entry_search_fields_match<'a>(
    title: &str,
    author: &str,
    path: &str,
    tags: impl IntoIterator<Item = &'a str>,
    folder_names: impl IntoIterator<Item = &'a str>,
    normalized_query: &str,
) -> bool {
    title.to_lowercase().contains(normalized_query)
        || author.to_lowercase().contains(normalized_query)
        || path.to_lowercase().contains(normalized_query)
        || tags
            .into_iter()
            .any(|tag| tag.to_lowercase().contains(normalized_query))
        || folder_names
            .into_iter()
            .any(|folder| folder.to_lowercase().contains(normalized_query))
}

fn library_entry_reading_state(entry: &LibraryEntry) -> LibraryReadingFilter {
    library_reading_state(entry.last_page, entry.page_count)
}

fn library_reading_state(last_page: u16, page_count: Option<u16>) -> LibraryReadingFilter {
    if page_count.is_some_and(|count| count > 0 && last_page.saturating_add(1) >= count) {
        LibraryReadingFilter::Finished
    } else if last_page > 0 {
        LibraryReadingFilter::Reading
    } else {
        LibraryReadingFilter::Unread
    }
}

fn entry_title(entry: &LibraryEntry) -> String {
    entry
        .display_title
        .clone()
        .or_else(|| entry.title.clone())
        .unwrap_or_else(|| {
            entry
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Untitled PDF")
                .to_owned()
        })
}

fn entry_author(entry: &LibraryEntry) -> String {
    entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"))
}

fn clean_metadata_input(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn range_selection_ids(
    first_index: usize,
    second_index: usize,
    entry_ids: &[EntryId],
) -> Vec<EntryId> {
    let start = first_index.min(second_index);
    let end = first_index
        .max(second_index)
        .min(entry_ids.len().saturating_sub(1));
    entry_ids[start..=end].to_vec()
}

fn drag_auto_scroll_velocity(cursor_y: f32, viewport_y: f32, viewport_height: f32) -> f32 {
    if viewport_height <= 1.0 {
        return 0.0;
    }

    let viewport_bottom = viewport_y + viewport_height;
    let band = LIBRARY_DRAG_AUTOSCROLL_EDGE_BAND.min(viewport_height / 2.0);
    if band <= 0.0 {
        return 0.0;
    }

    let strength = if cursor_y < viewport_y + band {
        -((viewport_y + band - cursor_y) / band).clamp(0.0, 1.0)
    } else if cursor_y > viewport_bottom - band {
        ((cursor_y - (viewport_bottom - band)) / band).clamp(0.0, 1.0)
    } else {
        0.0
    };

    if strength == 0.0 {
        return 0.0;
    }

    let eased = strength.abs().powi(2);
    let speed = LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED
        + (LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED - LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED) * eased;
    strength.signum() * speed
}

fn distance_between(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
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

fn page_count_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || String::from("Unknown pages"),
        |pages| {
            if pages == 1 {
                String::from("1 Page")
            } else {
                format!("{pages} Pages")
            }
        },
    )
}

fn last_opened_label(entry: &LibraryEntry) -> String {
    entry.opened_at.map_or_else(
        || String::from("Never opened"),
        |opened_at| format!("Last opened {}", opened_at.format("%b %-d, %Y")),
    )
}

fn file_size_label(entry: &LibraryEntry) -> String {
    std::fs::metadata(&entry.path).map_or_else(
        |_| String::from("Unknown size"),
        |metadata| format_file_size(metadata.len()),
    )
}

fn library_card_metadata_label(
    density: LibraryMetadataDensity,
    entry: &LibraryEntry,
) -> Option<String> {
    match density {
        LibraryMetadataDensity::Minimal => None,
        LibraryMetadataDensity::Standard => Some(library_card_page_count_label(entry)),
        LibraryMetadataDensity::Detailed => Some(format!(
            "{}   •   {}",
            library_card_page_count_label(entry),
            file_size_label(entry)
        )),
    }
}

fn library_row_metadata_label(density: LibraryMetadataDensity, entry: &LibraryEntry) -> String {
    match density {
        LibraryMetadataDensity::Minimal => entry_author(entry),
        LibraryMetadataDensity::Standard => format!(
            "{}{}",
            entry_author(entry),
            entry
                .page_count
                .map_or(String::new(), |pages| format!(" . {pages} pages"))
        ),
        LibraryMetadataDensity::Detailed => format!(
            "{}{} . {}",
            entry_author(entry),
            entry
                .page_count
                .map_or(String::new(), |pages| format!(" . {pages} pages")),
            file_size_label(entry)
        ),
    }
}

fn library_card_page_count_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || String::from("Unknown pages"),
        |pages| {
            if pages == 1 {
                String::from("1 page")
            } else {
                format!("{pages} pages")
            }
        },
    )
}

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn progress_fraction(entry: &LibraryEntry) -> f32 {
    if entry.missing {
        return 0.0;
    }

    entry.page_count.map_or(0.0, |pages| {
        if pages == 0 {
            0.0
        } else {
            (f32::from(entry.last_page) + 1.0) / f32::from(pages)
        }
    })
}

fn truncate_for_width(label: &str, width: f32, reserved_width: f32) -> String {
    truncate_for_width_with_font(label, width, reserved_width, FontSize::SM)
}

fn file_tree_label(label: &str, width: f32) -> String {
    truncate_for_width_with_font(label, width, 0.0, FILE_TREE_LABEL_SIZE)
}

fn file_tree_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(FILE_TREE_FONT_FAMILY),
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

fn schedule_zoom_render(generation: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(140)).await;
            generation
        },
        Message::ZoomRenderSettled,
    )
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

#[derive(Debug)]
struct ViewerCanvas<'a> {
    app: &'a PDFolioApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoomRenderPolicy {
    Immediate,
    Debounced,
}

impl canvas::Program<Message> for ViewerCanvas<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
            return None;
        };

        let (delta_x, delta_y) = scroll_delta_pixels(*delta, self.app.layout().line_scroll_pixels);

        let cursor = cursor
            .position_in(bounds)
            .unwrap_or_else(|| Point::new(bounds.width / 2.0, bounds.height / 2.0));

        Some(
            canvas::Action::publish(Message::ViewportWheelScrolled {
                delta_x,
                delta_y,
                cursor,
                viewport_width: bounds.width,
                viewport_height: bounds.height,
            })
            .and_capture(),
        )
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = canvas::Path::rectangle(Point::ORIGIN, bounds.size());
        let tokens = self.app.theme.tokens(&self.app.style_book);
        let viewer_style = viewer_primitives(tokens);
        frame.fill(&background, viewer_style.canvas);

        let Some(doc) = &self.app.doc else {
            return vec![frame.into_geometry()];
        };

        let page_width = f32::from(self.app.zoom_width);
        let x = ((bounds.width - page_width) / 2.0).max(Spacing::PAGE_GUTTER)
            - self.app.horizontal_offset;
        let mut y = Spacing::PAGE_GUTTER - self.app.scroll_offset;

        for page in 0..doc.page_count() {
            let height = self.app.page_height(page);
            let key = TileKey {
                page,
                width_px: self.app.render_width_px(),
            };
            let rect = Rectangle::new(Point::new(x, y), Size::new(page_width, height));

            if let Some(rendered) = self.app.rendered_page_for_draw(key) {
                frame.draw_image(rect, canvas::Image::new(rendered.handle.clone()).snap(true));
            } else {
                let shadow = canvas::Path::rectangle(
                    Point::new(
                        rect.x + viewer_style.page_shadow.offset_x,
                        rect.y + viewer_style.page_shadow.offset_y,
                    ),
                    Size::new(rect.width, rect.height),
                );
                frame.fill(&shadow, viewer_style.page_shadow.color);
                let placeholder = canvas::Path::rectangle(rect.position(), rect.size());
                frame.fill(&placeholder, viewer_style.placeholder);
            }

            y += height + Spacing::PAGE_GAP;
        }

        vec![frame.into_geometry()]
    }
}

fn subscription(app: &PDFolioApp) -> Subscription<Message> {
    let keyboard = event::listen_with(|event, status, _window| {
        if status == event::Status::Captured {
            return None;
        }

        match event {
            Event::Window(iced::window::Event::Opened { size, .. })
            | Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized {
                width: size.width,
                height: size.height,
            }),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                text,
                modifiers,
                ..
            }) => match (key, text.as_deref()) {
                (_, Some("t") | Some("T")) if modifiers.control() && modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::ToggleTheme))
                }
                (_, Some("r") | Some("R")) if modifiers.control() && modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::ReloadStyles))
                }
                (_, Some("g") | Some("G")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::Jump))
                }
                (_, Some("f") | Some("F")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::FocusSearch))
                }
                (_, Some("a") | Some("A")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::SelectAll))
                }
                (_, Some("+") | Some("=")) => Some(Message::ShortcutPressed(Shortcut::In)),
                (_, Some("-")) => Some(Message::ShortcutPressed(Shortcut::Out)),
                (keyboard::Key::Named(keyboard::key::Named::Enter), _) => {
                    Some(Message::ShortcutPressed(Shortcut::OpenSelected))
                }
                (keyboard::Key::Named(keyboard::key::Named::Delete), _) => {
                    Some(Message::ShortcutPressed(Shortcut::DeleteSelected))
                }
                (keyboard::Key::Named(keyboard::key::Named::F2), _) => {
                    Some(Message::ShortcutPressed(Shortcut::RenameSelected))
                }
                (keyboard::Key::Character(value), _) if value.as_str() == "0" => {
                    Some(Message::ShortcutPressed(Shortcut::Reset))
                }
                (keyboard::Key::Named(keyboard::key::Named::Space), _) if modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::PageUp))
                }
                (keyboard::Key::Named(keyboard::key::Named::Space), _) => {
                    Some(Message::ShortcutPressed(Shortcut::PageDown))
                }
                (keyboard::Key::Named(keyboard::key::Named::ArrowDown), _) => {
                    Some(Message::ShortcutPressed(Shortcut::FineScroll(64)))
                }
                (keyboard::Key::Named(keyboard::key::Named::ArrowUp), _) => {
                    Some(Message::ShortcutPressed(Shortcut::FineScroll(-64)))
                }
                (keyboard::Key::Named(keyboard::key::Named::ArrowRight), _) => {
                    Some(Message::ShortcutPressed(Shortcut::HorizontalPan(96)))
                }
                (keyboard::Key::Named(keyboard::key::Named::ArrowLeft), _) => {
                    Some(Message::ShortcutPressed(Shortcut::HorizontalPan(-96)))
                }
                (keyboard::Key::Named(keyboard::key::Named::Escape), _) => {
                    Some(Message::ShortcutPressed(Shortcut::Escape))
                }
                _ => None,
            },
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            _ => None,
        }
    });

    let watcher = if app.settings.watch_directories.is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.settings.watch_directories.clone(),
            watch_directories_stream,
        )
    };

    let style_watcher = if app.style_book.style_dirs().is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.style_book.style_dirs().to_vec(),
            watch_style_directories_stream,
        )
    };

    let sidebar_resize = if app.resizing_library_tag_sidebar {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::TagSidebarResizeDragged(position.x))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::EndTagSidebarResize)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let library_drag = if app.library_drag.is_some() {
        Subscription::batch([
            event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::LibraryEntryDragMoved(position))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::EndLibraryEntryDrag)
                }
                _ => None,
            }),
            time::every(Duration::from_millis(LIBRARY_DRAG_AUTOSCROLL_TICK_MS))
                .map(Message::LibraryAutoScrollTick),
        ])
    } else {
        Subscription::none()
    };

    let folder_drag = if app.folder_drag.is_some() {
        Subscription::batch([
            event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::FolderDragMoved(position))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::EndFolderDrag)
                }
                _ => None,
            }),
            time::every(Duration::from_millis(LIBRARY_DRAG_AUTOSCROLL_TICK_MS))
                .map(Message::LibraryAutoScrollTick),
        ])
    } else {
        Subscription::none()
    };

    let animations = if app.library_card_hover_animation_active()
        || app.bulk_operation_progress.is_some()
        || app.folder_drop_flash.is_some()
    {
        time::every(Duration::from_millis(LIBRARY_CARD_HOVER_TICK_MS)).map(Message::AnimationFrame)
    } else {
        Subscription::none()
    };

    Subscription::batch([
        keyboard,
        watcher,
        style_watcher,
        sidebar_resize,
        library_drag,
        folder_drag,
        animations,
    ])
}

fn watch_style_directories_stream(
    paths: &Vec<PathBuf>,
) -> impl iced::futures::Stream<Item = Message> {
    let paths = paths.clone();
    stream::channel(20, async move |mut output| {
        if paths.iter().all(|path| !path.exists()) {
            return;
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(
            move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };

                if style_watch_event_should_reload(&event) {
                    let _ = sender.send(());
                }
            },
        ) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "Could not create style filesystem watcher; falling back to polling");
                None
            }
        };

        if let Some(watcher) = watcher.as_mut() {
            for path in paths.iter().filter(|path| path.exists()) {
                if let Err(error) = watcher.watch(path, RecursiveMode::Recursive) {
                    tracing::warn!(
                        path = %path.display(),
                        %error,
                        "Could not watch style directory; polling will still detect changes"
                    );
                }
            }
        }

        let receiver = Arc::new(std::sync::Mutex::new(receiver));
        let mut snapshot = style_files_snapshot(&paths);
        loop {
            let receiver = Arc::clone(&receiver);
            let event = tokio::task::spawn_blocking(move || {
                receiver
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv_timeout(Duration::from_millis(500))
            })
            .await;

            let next_snapshot = style_files_snapshot(&paths);
            let notify_changed = matches!(event, Ok(Ok(())));
            let poll_changed = next_snapshot != snapshot;

            if notify_changed || poll_changed {
                snapshot = next_snapshot;
                tokio::time::sleep(Duration::from_millis(75)).await;
                if output.send(Message::ReloadStyles).await.is_err() {
                    break;
                }
            } else if matches!(event, Ok(Err(RecvTimeoutError::Disconnected)) | Err(_)) {
                break;
            }
        }
    })
}

fn style_watch_event_should_reload(event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }

    event.paths.is_empty()
        || event.paths.iter().any(|path| {
            path.is_dir()
                || path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        })
}

fn style_files_snapshot(paths: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut files = Vec::new();
    for path in paths {
        collect_style_files(path, &mut files);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files.dedup_by(|left, right| left.0 == right.0);
    files
}

fn collect_style_files(
    path: &std::path::Path,
    files: &mut Vec<(PathBuf, Option<SystemTime>, u64)>,
) {
    if path.is_file() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        {
            let metadata = std::fs::metadata(path).ok();
            files.push((
                path.to_path_buf(),
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.modified().ok()),
                metadata.as_ref().map_or(0, std::fs::Metadata::len),
            ));
        }
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        collect_style_files(&entry.path(), files);
    }
}

fn file_manager_command(path: &Path, reveal: bool) -> Option<(String, Vec<String>)> {
    let parent = path.parent()?;
    if cfg!(target_os = "windows") {
        if reveal {
            Some((
                String::from("explorer"),
                vec![format!("/select,{}", path.display())],
            ))
        } else {
            Some((String::from("explorer"), vec![parent.display().to_string()]))
        }
    } else if cfg!(target_os = "macos") {
        if reveal {
            Some((
                String::from("open"),
                vec![String::from("-R"), path.display().to_string()],
            ))
        } else {
            Some((String::from("open"), vec![parent.display().to_string()]))
        }
    } else {
        Some((String::from("xdg-open"), vec![parent.display().to_string()]))
    }
}

fn watch_directories_stream(paths: &Vec<PathBuf>) -> impl iced::futures::Stream<Item = Message> {
    let paths = paths.clone();
    stream::channel(100, async move |mut output| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = match LibraryWatcher::new(sender) {
            Ok(watcher) => watcher,
            Err(error) => {
                let _ = output.send(Message::LibraryError(error.to_string())).await;
                return;
            }
        };

        for path in &paths {
            if let Err(error) = watcher.watch_directory(path) {
                let _ = output
                    .send(Message::LibraryError(format!(
                        "Could not watch {}: {error}",
                        path.display()
                    )))
                    .await;
            }
        }

        let receiver = Arc::new(std::sync::Mutex::new(receiver));
        loop {
            let receiver = Arc::clone(&receiver);
            let event = tokio::task::spawn_blocking(move || {
                receiver
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv()
            })
            .await;

            let Ok(Ok(event)) = event else {
                break;
            };

            if output
                .send(Message::LibraryWatchEvent(event))
                .await
                .is_err()
            {
                break;
            }
        }

        drop(watcher);
    })
}

fn scroll_delta_pixels(delta: mouse::ScrollDelta, line_scroll_pixels: f32) -> (f32, f32) {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => (x * line_scroll_pixels, y * line_scroll_pixels),
        mouse::ScrollDelta::Pixels { x, y } => (x, y),
    }
}

fn toggle_selection_entry_id(selection: &mut HashSet<EntryId>, entry_id: EntryId) {
    if !selection.insert(entry_id.clone()) {
        selection.remove(&entry_id);
    }
}

fn master_checkbox_state_for_counts(
    selected_visible: usize,
    visible_count: usize,
) -> MasterCheckboxState {
    match selected_visible {
        0 => MasterCheckboxState::None,
        count if count == visible_count && visible_count > 0 => MasterCheckboxState::All,
        _ => MasterCheckboxState::Partial,
    }
}

fn reorder_entry_ids_for_drag(
    entries: &[EntryId],
    dragged_entries: &[EntryId],
    drop_index: usize,
) -> Vec<EntryId> {
    let dragged = dragged_entries.iter().cloned().collect::<HashSet<_>>();
    if dragged.is_empty() {
        return entries.to_vec();
    }

    let moving = entries
        .iter()
        .filter(|entry_id| dragged.contains(*entry_id))
        .cloned()
        .collect::<Vec<_>>();
    if moving.is_empty() {
        return entries.to_vec();
    }

    let mut remaining = entries
        .iter()
        .filter(|entry_id| !dragged.contains(*entry_id))
        .cloned()
        .collect::<Vec<_>>();
    let insert_index = drop_index.min(remaining.len());
    remaining.splice(insert_index..insert_index, moving);
    remaining
}

fn reorder_folder_ids_before_target(
    folders: &[FolderId],
    dragged_folder: &FolderId,
    target_folder: &FolderId,
) -> Option<Vec<FolderId>> {
    if dragged_folder == target_folder {
        return None;
    }

    let dragged_index = folders
        .iter()
        .position(|folder_id| folder_id == dragged_folder)?;
    let target_index = folders
        .iter()
        .position(|folder_id| folder_id == target_folder)?;
    let mut next_order = folders.to_vec();
    let dragged = next_order.remove(dragged_index);
    let target_index = next_order
        .iter()
        .position(|folder_id| folder_id == target_folder)
        .unwrap_or(target_index.min(next_order.len()));
    next_order.insert(target_index, dragged);
    Some(next_order)
}

#[cfg(test)]
fn dragged_placeholder_count(entries: &[EntryId], dragged_entries: &[EntryId]) -> usize {
    let dragged = dragged_entries.iter().collect::<HashSet<_>>();
    entries
        .iter()
        .filter(|entry_id| dragged.contains(entry_id))
        .count()
}

fn folder_drop_target_ready(started_at: Instant, now: Instant) -> bool {
    now.checked_duration_since(started_at)
        .is_some_and(|elapsed| elapsed >= Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS))
}

fn folder_can_move_into(folders: &[Folder], folder_id: &FolderId, target_id: &FolderId) -> bool {
    if folder_id == target_id {
        return false;
    }

    let mut current = Some(target_id);
    while let Some(id) = current {
        if id == folder_id {
            return false;
        }
        current = folders
            .iter()
            .find(|folder| &folder.id == id)
            .and_then(|folder| folder.parent_id.as_ref());
    }

    folders.iter().any(|folder| &folder.id == target_id)
}

fn folder_card_target_at_cursor(
    cursor: Point,
    folders: &[Folder],
    dragged_folder_id: &FolderId,
    viewport_x: f32,
    viewport_y: f32,
    scroll_offset: f32,
    card_width: f32,
    row_height: f32,
    column_gap: f32,
    row_gap: f32,
    per_row: usize,
) -> Option<FolderId> {
    if folders.is_empty() || per_row == 0 {
        return None;
    }

    let content_x = cursor.x - viewport_x;
    let content_y = cursor.y - viewport_y + scroll_offset;
    if content_x < 0.0 || content_y < 0.0 {
        return None;
    }

    let column_pitch = card_width + column_gap;
    let row_pitch = row_height + row_gap;
    if column_pitch <= 0.0 || row_pitch <= 0.0 {
        return None;
    }

    let column = (content_x / column_pitch).floor() as usize;
    let row = (content_y / row_pitch).floor() as usize;
    if column >= per_row {
        return None;
    }

    let x_in_cell = content_x - column as f32 * column_pitch;
    let y_in_cell = content_y - row as f32 * row_pitch;
    if x_in_cell > card_width || y_in_cell > row_height {
        return None;
    }

    let index = row.saturating_mul(per_row).saturating_add(column);
    folders
        .get(index)
        .filter(|folder| &folder.id != dragged_folder_id)
        .map(|folder| folder.id.clone())
}

#[cfg(test)]
fn folder_drop_target_at_cursor(
    cursor: Point,
    targets: &[(FolderId, Rectangle)],
) -> Option<FolderId> {
    targets
        .iter()
        .find(|(_, bounds)| {
            cursor.x >= bounds.x
                && cursor.x <= bounds.x + bounds.width
                && cursor.y >= bounds.y
                && cursor.y <= bounds.y + bounds.height
        })
        .map(|(folder_id, _)| folder_id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db(label: &str) -> Db {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pdf-folio-ui-{label}-{}-{nanos}.db",
            std::process::id()
        ));
        Db::open(path).expect("test database should open")
    }

    #[test]
    fn range_selection_ids_preserves_visible_order_forward() {
        let entries = ["a", "b", "c", "d"].map(EntryId::new);
        let ids = range_selection_ids(1, 3, &entries);

        assert_eq!(
            ids.iter().map(EntryId::as_str).collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn range_selection_ids_preserves_visible_order_backward() {
        let entries = ["a", "b", "c", "d"].map(EntryId::new);
        let ids = range_selection_ids(3, 1, &entries);

        assert_eq!(
            ids.iter().map(EntryId::as_str).collect::<Vec<_>>(),
            vec!["b", "c", "d"]
        );
    }

    #[test]
    fn checkbox_toggle_adds_and_removes_one_entry_without_clearing_others() {
        let mut selected = HashSet::from([EntryId::new("a"), EntryId::new("b")]);

        toggle_selection_entry_id(&mut selected, EntryId::new("c"));
        assert!(selected.contains(&EntryId::new("a")));
        assert!(selected.contains(&EntryId::new("b")));
        assert!(selected.contains(&EntryId::new("c")));

        toggle_selection_entry_id(&mut selected, EntryId::new("b"));
        assert!(selected.contains(&EntryId::new("a")));
        assert!(!selected.contains(&EntryId::new("b")));
        assert!(selected.contains(&EntryId::new("c")));
    }

    #[test]
    fn master_checkbox_state_reflects_none_partial_and_all_visible_selection() {
        assert_eq!(
            master_checkbox_state_for_counts(0, 4),
            MasterCheckboxState::None
        );
        assert_eq!(
            master_checkbox_state_for_counts(2, 4),
            MasterCheckboxState::Partial
        );
        assert_eq!(
            master_checkbox_state_for_counts(4, 4),
            MasterCheckboxState::All
        );
        assert_eq!(
            master_checkbox_state_for_counts(0, 0),
            MasterCheckboxState::None
        );
    }

    #[test]
    fn metadata_density_round_trips_visible_field_preferences() {
        for density in [
            LibraryMetadataDensity::Minimal,
            LibraryMetadataDensity::Standard,
            LibraryMetadataDensity::Detailed,
        ] {
            assert_eq!(
                LibraryMetadataDensity::from_visible_fields(&density.visible_fields()),
                density
            );
        }
    }

    #[test]
    fn reading_state_uses_progress_and_known_page_count() {
        assert_eq!(
            library_reading_state(0, Some(12)),
            LibraryReadingFilter::Unread
        );
        assert_eq!(
            library_reading_state(3, Some(12)),
            LibraryReadingFilter::Reading
        );
        assert_eq!(
            library_reading_state(11, Some(12)),
            LibraryReadingFilter::Finished
        );
        assert_eq!(
            library_reading_state(4, None),
            LibraryReadingFilter::Reading
        );
    }

    #[test]
    fn multi_drag_reorder_preserves_selected_relative_order() {
        let entries = ["a", "b", "c", "d", "e"].map(EntryId::new);
        let dragged = ["b", "d"].map(EntryId::new);
        let result = reorder_entry_ids_for_drag(&entries, &dragged, 2);

        assert_eq!(
            result.iter().map(EntryId::as_str).collect::<Vec<_>>(),
            vec!["a", "c", "b", "d", "e"]
        );
    }

    #[test]
    fn multi_drag_reorder_can_append_selected_group() {
        let entries = ["a", "b", "c", "d"].map(EntryId::new);
        let dragged = ["a", "c"].map(EntryId::new);
        let result = reorder_entry_ids_for_drag(&entries, &dragged, usize::MAX);

        assert_eq!(
            result.iter().map(EntryId::as_str).collect::<Vec<_>>(),
            vec!["b", "d", "a", "c"]
        );
    }

    #[test]
    fn multi_drag_placeholder_count_matches_visible_selection_size() {
        let entries = ["a", "b", "c", "d"].map(EntryId::new);
        let dragged = ["b", "d", "missing"].map(EntryId::new);

        assert_eq!(dragged_placeholder_count(&entries, &dragged), 2);
    }

    #[test]
    fn folder_drag_reorder_moves_folder_before_hovered_sibling() {
        let folders = ["a", "b", "c", "d"].map(FolderId::new);

        let result =
            reorder_folder_ids_before_target(&folders, &FolderId::new("d"), &FolderId::new("b"))
                .unwrap();

        assert_eq!(
            result.iter().map(FolderId::as_str).collect::<Vec<_>>(),
            vec!["a", "d", "b", "c"]
        );
    }

    #[test]
    fn folder_drop_target_activates_after_dwell_delay() {
        let started_at = Instant::now();

        assert!(!folder_drop_target_ready(
            started_at,
            started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS - 1)
        ));
        assert!(folder_drop_target_ready(
            started_at,
            started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS)
        ));
    }

    #[test]
    fn folder_drop_target_hit_testing_resolves_cursor_bounds() {
        let reading = FolderId::new("reading");
        let archive = FolderId::new("archive");
        let targets = vec![
            (
                reading.clone(),
                Rectangle {
                    x: 10.0,
                    y: 20.0,
                    width: 100.0,
                    height: 40.0,
                },
            ),
            (
                archive.clone(),
                Rectangle {
                    x: 10.0,
                    y: 72.0,
                    width: 100.0,
                    height: 40.0,
                },
            ),
        ];

        assert_eq!(
            folder_drop_target_at_cursor(Point::new(24.0, 36.0), &targets),
            Some(reading)
        );
        assert_eq!(
            folder_drop_target_at_cursor(Point::new(24.0, 84.0), &targets),
            Some(archive)
        );
        assert_eq!(
            folder_drop_target_at_cursor(Point::new(4.0, 84.0), &targets),
            None
        );
    }

    #[test]
    fn folder_move_target_rejects_self_and_descendants() {
        let db = test_db("folder-move-target");
        let root = db.create_folder("Root", None).unwrap();
        let child = db.create_folder("Child", Some(&root)).unwrap();
        let sibling = db.create_folder("Sibling", None).unwrap();
        let folders = db.get_folders().unwrap();

        assert!(!folder_can_move_into(&folders, &root, &root));
        assert!(!folder_can_move_into(&folders, &root, &child));
        assert!(folder_can_move_into(&folders, &child, &sibling));
    }

    #[test]
    fn folder_card_target_hit_test_uses_grid_cells() {
        let db = test_db("folder-card-hit-test");
        let first = db.create_folder("First", None).unwrap();
        let second = db.create_folder("Second", None).unwrap();
        let third = db.create_folder("Third", None).unwrap();
        let folders = db.get_folders().unwrap();

        assert_eq!(
            folder_card_target_at_cursor(
                Point::new(132.0, 18.0),
                &folders,
                &first,
                0.0,
                0.0,
                0.0,
                100.0,
                40.0,
                12.0,
                8.0,
                2,
            ),
            Some(second)
        );
        assert_eq!(
            folder_card_target_at_cursor(
                Point::new(18.0, 18.0),
                &folders,
                &first,
                0.0,
                0.0,
                0.0,
                100.0,
                40.0,
                12.0,
                8.0,
                2,
            ),
            None
        );
        assert_eq!(
            folder_card_target_at_cursor(
                Point::new(108.0, 18.0),
                &folders,
                &third,
                0.0,
                0.0,
                0.0,
                100.0,
                40.0,
                12.0,
                8.0,
                2,
            ),
            None
        );
        assert_eq!(
            folder_card_target_at_cursor(
                Point::new(18.0, 58.0),
                &folders,
                &first,
                0.0,
                0.0,
                0.0,
                100.0,
                40.0,
                12.0,
                8.0,
                2,
            ),
            Some(third)
        );
    }

    #[test]
    fn entry_search_fields_match_folder_names() {
        assert!(entry_search_fields_match(
            "Quarterly Report",
            "Analyst",
            "/tmp/report.pdf",
            ["finance"].into_iter(),
            ["Research Archive"].into_iter(),
            "archive",
        ));
        assert!(!entry_search_fields_match(
            "Quarterly Report",
            "Analyst",
            "/tmp/report.pdf",
            ["finance"].into_iter(),
            ["Research Archive"].into_iter(),
            "cookbook",
        ));
    }

    #[test]
    fn search_match_source_label_reports_metadata_source() {
        assert_eq!(
            search_match_source_label_for_fields(
                "Quarterly Report",
                "Analyst",
                "/tmp/reports/quarterly.pdf",
                ["finance"],
                ["Research Archive"],
                "quarterly"
            ),
            Some(String::from("Match in title"))
        );
        assert_eq!(
            search_match_source_label_for_fields(
                "Quarterly Report",
                "Analyst",
                "/tmp/reports/quarterly.pdf",
                ["finance"],
                ["Research Archive"],
                "analyst"
            ),
            Some(String::from("Match in author"))
        );
        assert_eq!(
            search_match_source_label_for_fields(
                "Quarterly Report",
                "Analyst",
                "/tmp/reports/quarterly.pdf",
                ["finance"],
                ["Research Archive"],
                "finance"
            ),
            Some(String::from("Match in tag"))
        );
        assert_eq!(
            search_match_source_label_for_fields(
                "Quarterly Report",
                "Analyst",
                "/tmp/reports/quarterly.pdf",
                ["finance"],
                ["Research Archive"],
                "archive"
            ),
            Some(String::from("Match in folder"))
        );
        assert_eq!(
            search_match_source_label_for_fields(
                "Quarterly Report",
                "Analyst",
                "/tmp/reports/quarterly.pdf",
                ["finance"],
                ["Research Archive"],
                "reports"
            ),
            Some(String::from("Match in path"))
        );
    }

    #[test]
    fn import_title_cleanup_rejects_poor_titles_and_cleans_spacing() {
        assert_eq!(
            clean_import_title("  Quarterly   Report  "),
            Some(String::from("Quarterly Report"))
        );
        assert_eq!(clean_import_title("Untitled"), None);
        assert_eq!(
            title_from_path(Path::new("/tmp/Research Notes.pdf")),
            Some(String::from("Research Notes"))
        );
    }

    #[test]
    fn file_manager_command_targets_parent_for_containing_folder() {
        let path = PathBuf::from("/tmp/pdf-folio/report.pdf");

        let (_, args) = file_manager_command(&path, false).unwrap();
        assert!(args.iter().any(|arg| arg.contains("pdf-folio")));

        let (program, reveal_args) = file_manager_command(&path, true).unwrap();
        assert!(!program.is_empty());
        assert!(!reveal_args.is_empty());
    }

    #[test]
    fn duplicate_status_label_reports_unique_and_matching_count() {
        assert_eq!(duplicate_status_label_for_count(0), "Unique content hash");
        assert_eq!(duplicate_status_label_for_count(2), "2 matching duplicates");
    }

    #[test]
    fn folder_smart_count_labels_include_progress_and_missing_state() {
        let counts = FolderSmartCounts {
            total: 12,
            in_progress: 3,
            missing: 1,
        };

        assert_eq!(
            folder_meta_label(counts, 2),
            "12 PDFs . 2 Folders . 3 reading . 1 missing"
        );
        assert_eq!(
            folder_sidebar_count_label(counts),
            "12 PDFs . 3 reading . 1 missing"
        );
        assert_eq!(folder_meta_label(FolderSmartCounts::default(), 0), "Empty");
    }

    #[test]
    fn indeterminate_progress_value_stays_inside_progress_bar_range() {
        for elapsed in [0.0, 0.25, 0.75, 1.5, 4.25, 12.0] {
            let value = indeterminate_progress_value(elapsed);
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn folder_drop_flash_expires_after_success_window() {
        let folder_id = FolderId::new("reading");
        let started_at = Instant::now();

        assert!(folder_drop_flash_active_at(
            &folder_id,
            Some((&folder_id, started_at)),
            started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS - 1)
        ));
        assert!(!folder_drop_flash_active_at(
            &folder_id,
            Some((&folder_id, started_at)),
            started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
        ));
    }

    #[test]
    fn drag_auto_scroll_is_idle_outside_edge_bands() {
        assert_eq!(drag_auto_scroll_velocity(240.0, 100.0, 400.0), 0.0);
    }

    #[test]
    fn drag_auto_scroll_velocity_tracks_nearest_edge_direction() {
        let up = drag_auto_scroll_velocity(110.0, 100.0, 400.0);
        let down = drag_auto_scroll_velocity(490.0, 100.0, 400.0);

        assert!(up < 0.0);
        assert!(down > 0.0);
        assert!((up.abs() - down).abs() < 0.01);
    }

    #[test]
    fn drag_auto_scroll_velocity_clamps_outside_viewport() {
        let above = drag_auto_scroll_velocity(0.0, 100.0, 400.0);
        let below = drag_auto_scroll_velocity(600.0, 100.0, 400.0);

        assert_eq!(above, -LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED);
        assert_eq!(below, LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED);
    }

    #[test]
    fn drag_auto_scroll_edge_band_shrinks_for_short_viewports() {
        let center = drag_auto_scroll_velocity(125.0, 100.0, 50.0);
        let top = drag_auto_scroll_velocity(101.0, 100.0, 50.0);

        assert_eq!(center, 0.0);
        assert!(top < 0.0);
    }

    #[test]
    fn masonry_target_index_uses_card_midpoints_as_insertion_slots() {
        let layout = LibraryMasonryLayout {
            columns: vec![vec![
                LibraryMasonryItem {
                    index: 0,
                    top: 0.0,
                    height: 100.0,
                },
                LibraryMasonryItem {
                    index: 2,
                    top: 120.0,
                    height: 100.0,
                },
            ]],
            content_height: 220.0,
        };

        assert_eq!(masonry_target_index(&layout, 0, 49.0), Some(0));
        assert_eq!(masonry_target_index(&layout, 0, 50.0), Some(2));
        assert_eq!(masonry_target_index(&layout, 0, 220.0), Some(3));
    }

    #[test]
    fn masonry_target_index_empty_column_appends_to_compact_items() {
        let layout = LibraryMasonryLayout {
            columns: vec![
                vec![LibraryMasonryItem {
                    index: 0,
                    top: 0.0,
                    height: 100.0,
                }],
                Vec::new(),
            ],
            content_height: 100.0,
        };

        assert_eq!(masonry_target_index(&layout, 1, 20.0), Some(1));
    }

    #[test]
    fn style_watch_event_reloads_for_kdl_changes() {
        let event = notify::Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("styles/components/library/sidebar.kdl")],
            attrs: notify::event::EventAttributes::new(),
        };

        assert!(style_watch_event_should_reload(&event));
    }

    #[test]
    fn style_watch_event_reloads_for_directory_changes() {
        let root =
            std::env::temp_dir().join(format!("pdf-folio-style-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("test style dir should be created");
        let event = notify::Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )),
            paths: vec![root.clone()],
            attrs: notify::event::EventAttributes::new(),
        };

        assert!(style_watch_event_should_reload(&event));

        std::fs::remove_dir_all(root).expect("test style dir should be removed");
    }

    #[test]
    fn style_watch_event_ignores_unrelated_paths() {
        let event = notify::Event {
            kind: EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content,
            )),
            paths: vec![PathBuf::from("README.md")],
            attrs: notify::event::EventAttributes::new(),
        };

        assert!(!style_watch_event_should_reload(&event));
    }
}
