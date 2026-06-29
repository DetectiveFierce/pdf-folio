//! Top-level application state and launch entrypoint.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use iced::futures::SinkExt;
use iced::mouse;
use iced::stream;
use iced::time;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, canvas, column, container, image, mouse_area, pick_list, pin, row, scrollable, stack,
    text, text_input, tooltip, Svg,
};
use iced::widget::{operation, Id};
use iced::{event, keyboard, Element, Event, Length, Point, Rectangle, Renderer, Size};
use iced::{Subscription, Task, Theme};
use pdf_folio_core::{Annotation, OutlineNode, PdfDoc, RenderedPage, TileCache, TileKey};
use pdf_folio_library::{
    hash_file, scan_pdf_files, thumbnail_path, Db, EntryId, ImportSummary, ImportedEntry,
    IndexDocument, LibraryEntry, LibraryLayoutMode, LibraryPreferences, LibrarySortMode,
    LibraryWatchEvent, LibraryWatcher, NewLibraryEntry, SearchIndex,
};

use crate::messages::{Message, Shortcut};
use crate::style::layout::{
    JUMP_INPUT_WIDTH, LIBRARY_CARD_THUMBNAIL_WIDTH, LIBRARY_GRID_ROW_HEIGHT,
    LIBRARY_LIST_ROW_HEIGHT, LIBRARY_ROW_PROGRESS_WIDTH, LIBRARY_ROW_THUMBNAIL_WIDTH,
    LIBRARY_SIDEBAR_MAX_WIDTH, LIBRARY_SIDEBAR_MIN_WIDTH, SIDEBAR_RESIZE_HANDLE_VISUAL_WIDTH,
    SIDEBAR_RESIZE_HANDLE_WIDTH, VIEWER_SIDEBAR_WIDTH,
};
use crate::style::{
    container_style, empty_state, icon_button, menu_style, mix_color, pick_list_style,
    progress_bar, scrollable_style, search_input, section_heading, sidebar_button, tag_pill,
    text_input_style, toc_entry, toolbar_button, viewer_primitives, Class, FontSize, Spacing,
    ThemeTokens, CARD_GRID_COLUMNS,
};
use crate::style::{LIBRARY_OVERSCAN_ROWS, LINE_SCROLL_PIXELS, WINDOW_SIZE};
use crate::theme::AppTheme;

const CHEVRON_LEFT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"##;
const CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"##;
const LIBRARY_CARD_TITLE_WIDTH: f32 = LIBRARY_CARD_THUMBNAIL_WIDTH;
const LIBRARY_ROW_TITLE_WIDTH: f32 = 520.0;
const LIBRARY_DRAG_PREVIEW_GRID_X_OFFSET: f32 = 32.0;
const LIBRARY_DRAG_PREVIEW_GRID_Y_OFFSET: f32 = 28.0;
const LIBRARY_DRAG_PREVIEW_LIST_X_OFFSET: f32 = 28.0;
const LIBRARY_DRAG_PREVIEW_LIST_Y_OFFSET: f32 = 24.0;
const LIBRARY_DRAG_PLACEHOLDER_CONTENT_ALPHA: f32 = 0.42;
const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
const LIBRARY_DRAG_AUTOSCROLL_TICK_MS: u64 = 16;
const LIBRARY_DRAG_AUTOSCROLL_EDGE_BAND: f32 = 96.0;
const LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED: f32 = 980.0;
const LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED: f32 = 80.0;
const LIBRARY_DRAG_AUTOSCROLL_MAX_DT: f32 = 1.0 / 20.0;
const LIBRARY_DRAG_ACTIVATION_DISTANCE: f32 = 6.0;
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
    /// Whether the jump-to-page overlay is open.
    pub jump_dialog_open: bool,
    /// Current jump-to-page input text.
    pub jump_input: String,
    /// In-memory annotations for the open document.
    pub annotations: Vec<Annotation>,
    /// Loaded library entries.
    pub library_entries: Vec<LibraryEntry>,
    /// Active library sort mode.
    pub library_sort_mode: LibrarySortMode,
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
    /// Lazily loaded cover thumbnails keyed by entry id.
    pub thumbnails: HashMap<EntryId, ThumbnailView>,
    /// Thumbnail loads/renders currently in flight.
    pub pending_thumbnails: HashSet<EntryId>,
    /// Active tag filter.
    pub active_tag_filter: Option<String>,
    /// Entry currently showing inline tag input.
    pub tag_entry_id: Option<EntryId>,
    /// Current inline tag text.
    pub tag_input: String,
    /// Latest library/import status.
    pub library_status: Option<String>,
    /// Last library entry click used to detect double-click opens.
    pub last_library_click: Option<(EntryId, Instant)>,
    /// Active library entry drag state.
    pub library_drag: Option<LibraryDragState>,
    /// Current visual theme.
    pub theme: AppTheme,
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

/// Current manual-reorder drag state for the library view.
#[derive(Debug, Clone)]
pub struct LibraryDragState {
    /// Entry being dragged.
    pub entry_id: EntryId,
    /// Original zero-based index in the visible manual-order list.
    pub source_index: usize,
    /// Current insertion index after removing the dragged entry from the visible list.
    pub target_index: usize,
    /// Whether pointer movement has crossed the drag threshold.
    pub active: bool,
    /// Cursor position recorded when the press began.
    pub press_cursor: Option<Point>,
    /// Latest cursor position in window coordinates.
    pub cursor: Option<Point>,
    /// Last auto-scroll tick used for frame-rate independent scrolling.
    pub last_auto_scroll_tick: Option<Instant>,
}

#[derive(Debug, Clone)]
enum LibraryRenderItem {
    Entry(LibraryEntry),
    Ghost(LibraryEntry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryEntryRenderMode {
    Normal,
    Placeholder,
    Floating,
}

impl PDFolioApp {
    /// Creates application state using the default database location.
    ///
    /// # Errors
    ///
    /// Returns an error when the library database cannot be opened.
    pub fn new() -> Result<Self> {
        let settings = Settings::default();
        let db = Arc::new(Db::open_default()?);
        let preferences = db.library_preferences().unwrap_or_default();
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
            jump_dialog_open: false,
            jump_input: String::new(),
            annotations: Vec::new(),
            library_entries: Vec::new(),
            library_sort_mode: preferences.sort_mode,
            search_query: String::new(),
            search_results: None,
            search_hit_pages: HashMap::new(),
            search_generation: 0,
            library_scroll_offset: 0.0,
            library_viewport_height: 720.0,
            library_viewport_x: 0.0,
            library_viewport_y: 0.0,
            library_viewport_width: 960.0,
            library_tag_sidebar_width: preferences
                .sidebar_width
                .clamp(LIBRARY_SIDEBAR_MIN_WIDTH, LIBRARY_SIDEBAR_MAX_WIDTH),
            library_tag_sidebar_open: true,
            resizing_library_tag_sidebar: false,
            thumbnails: HashMap::new(),
            pending_thumbnails: HashSet::new(),
            active_tag_filter: None,
            tag_entry_id: None,
            tag_input: String::new(),
            library_status: None,
            last_library_click: None,
            library_drag: None,
            theme: AppTheme::Dark,
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
        Task::batch([self.refresh_library(), self.request_visible_thumbnails()])
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
            .cloned()
            .collect()
    }

    fn visible_library_entry_window(&self, entries_len: usize) -> std::ops::Range<usize> {
        if entries_len == 0 {
            return 0..0;
        }

        let per_row = self.library_entries_per_row();
        let row_height = self.library_row_height();
        let first_row = (self.library_scroll_offset / row_height).floor().max(0.0) as usize;
        let visible_rows = (self.library_viewport_height / row_height).ceil().max(1.0) as usize;
        let start_row = first_row.saturating_sub(LIBRARY_OVERSCAN_ROWS);
        let end_row = first_row
            .saturating_add(visible_rows)
            .saturating_add(LIBRARY_OVERSCAN_ROWS)
            .saturating_add(1);

        let start = (start_row * per_row).min(entries_len);
        let end = (end_row * per_row).min(entries_len);
        start..end
    }

    fn library_entries_per_row(&self) -> usize {
        if self.compact_view_mode {
            1
        } else {
            CARD_GRID_COLUMNS
        }
    }

    fn library_row_height(&self) -> f32 {
        if self.compact_view_mode {
            LIBRARY_LIST_ROW_HEIGHT
        } else {
            LIBRARY_GRID_ROW_HEIGHT
        }
    }

    fn can_drag_reorder_library(&self) -> bool {
        self.library_sort_mode == LibrarySortMode::Manual
            && self.search_query.trim().is_empty()
            && self.search_results.is_none()
            && self.active_tag_filter.is_none()
    }

    fn begin_library_drag(&mut self, entry_id: EntryId) {
        let Some(source_index) = self
            .visible_library_entries()
            .iter()
            .position(|entry| entry.id == entry_id)
        else {
            return;
        };

        self.library_drag = Some(LibraryDragState {
            entry_id,
            source_index,
            target_index: source_index,
            active: false,
            press_cursor: None,
            cursor: None,
            last_auto_scroll_tick: None,
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
            if can_drag_reorder
                && !drag.active
                && distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE
            {
                drag.active = true;
            } else if !can_drag_reorder
                && distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE
            {
                self.library_status = Some(String::from(
                    "Switch to unfiltered Manual sort to reorder PDFs.",
                ));
            }
        }

        if self.library_drag.as_ref().is_some_and(|drag| drag.active) {
            self.update_library_drag_target_from_cursor();
        }
    }

    fn update_library_drag_target_from_cursor(&mut self) {
        let entries_len = self.visible_library_entries().len();
        if entries_len == 0 {
            return;
        }

        let Some(cursor) = self.library_drag.as_ref().and_then(|drag| drag.cursor) else {
            return;
        };

        let content_y = (cursor.y - self.library_viewport_y + self.library_scroll_offset).max(0.0);
        let row = (content_y / self.library_row_height()).floor().max(0.0) as usize;
        let mut index = row.saturating_mul(self.library_entries_per_row());

        if !self.compact_view_mode {
            let per_row = self.library_entries_per_row();
            let gap_width = Spacing::MD * per_row.saturating_sub(1) as f32;
            let card_width = ((self.library_viewport_width - gap_width) / per_row.max(1) as f32)
                .max(LIBRARY_CARD_THUMBNAIL_WIDTH);
            let column_step = (card_width + Spacing::MD).max(1.0);
            let content_x = (cursor.x - self.library_viewport_x).max(0.0);
            let column = (content_x / column_step)
                .floor()
                .clamp(0.0, per_row.saturating_sub(1) as f32) as usize;
            index = index.saturating_add(column);
        }

        let target_index = index.min(entries_len.saturating_sub(1));
        if let Some(drag) = &mut self.library_drag {
            drag.target_index = target_index;
        }
    }

    fn library_content_height_for_len(&self, entries_len: usize) -> f32 {
        if entries_len == 0 {
            return 0.0;
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
            return Task::done(Message::LibraryEntryClicked(drag.entry_id));
        }

        if drag.source_index == drag.target_index || !self.can_drag_reorder_library() {
            return scroll_library_to_offset_task(self.library_scroll_offset);
        }

        let mut entries = self.visible_library_entries();
        if drag.source_index >= entries.len() || drag.target_index >= entries.len() {
            return Task::none();
        }

        let moved = entries.remove(drag.source_index);
        let insert_index = drag.target_index.min(entries.len());
        entries.insert(insert_index, moved);

        let entry_ids: Vec<EntryId> = entries.iter().map(|entry| entry.id.clone()).collect();
        self.library_entries = entries;
        self.library_status = Some(String::from("Saving manual PDF order..."));
        Task::batch([
            persist_manual_entry_order_task(Arc::clone(&self.db), entry_ids),
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
        let window = self.visible_library_entry_window(entries.len());
        for entry in entries[window].iter().cloned() {
            if self.thumbnails.contains_key(&entry.id)
                || self.pending_thumbnails.contains(&entry.id)
            {
                continue;
            }
            self.pending_thumbnails.insert(entry.id.clone());
            tasks.push(Task::perform(
                load_or_render_thumbnail(entry),
                |result| match result {
                    Ok((entry_id, page)) => Message::ThumbnailReady {
                        entry_id,
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
            let load_task = app.clone().refresh_library();
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
    .subscription(subscription)
    .scale_factor(|app| app.scale_factor)
    .window_size(WINDOW_SIZE)
    .centered()
    .run()?;

    Ok(())
}

fn update(app: &mut PDFolioApp, message: Message) -> Task<Message> {
    match message {
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
        Message::LibraryLoaded(entries) => {
            app.library_entries = entries;
            app.library_status = Some(format!("{} PDFs in library", app.library_entries.len()));
            if !app.search_query.trim().is_empty() {
                return Task::done(Message::SearchDebounced(app.search_query.clone()));
            }
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(app.library_scroll_offset),
            ]);
        }
        Message::LibraryRefresh => return app.refresh_library(),
        Message::LibraryError(error) => {
            app.library_status = Some(error);
            app.pending_thumbnails.clear();
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
        Message::BeginLibraryEntryDrag(entry_id) => {
            app.begin_library_drag(entry_id);
        }
        Message::LibraryEntryDragMoved(position) => {
            app.update_library_drag_target(position);
        }
        Message::LibraryAutoScrollTick(tick) => {
            return app.auto_scroll_library_drag(tick);
        }
        Message::EndLibraryEntryDrag => {
            return app.finish_library_drag();
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
            app.library_tag_sidebar_open = false;
            app.resizing_library_tag_sidebar = false;
        }
        Message::ExpandLibrarySidebar => {
            app.library_tag_sidebar_open = true;
        }
        Message::BeginTagSidebarResize => {
            app.resizing_library_tag_sidebar = true;
        }
        Message::TagSidebarResizeDragged(width) => {
            if app.resizing_library_tag_sidebar {
                app.library_tag_sidebar_width =
                    width.clamp(LIBRARY_SIDEBAR_MIN_WIDTH, LIBRARY_SIDEBAR_MAX_WIDTH);
            }
        }
        Message::EndTagSidebarResize => {
            app.resizing_library_tag_sidebar = false;
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
            return app.request_visible_thumbnails();
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
        Message::ThumbnailReady {
            entry_id,
            data,
            width,
            height,
        } => {
            app.pending_thumbnails.remove(&entry_id);
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.thumbnails.insert(
                entry_id,
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
        Message::ShortcutPressed(Shortcut::Jump) => {
            app.jump_dialog_open = true;
            app.jump_input = (u32::from(app.current_page()) + 1).to_string();
        }
        Message::ShortcutPressed(Shortcut::Escape) => {
            if app.jump_dialog_open {
                app.jump_dialog_open = false;
                app.jump_input.clear();
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

fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
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
            view_toolbar(app),
            row![sidebar, main.width(Length::Fill)].height(Length::Fill)
        ]
        .into()
    } else {
        column![view_toolbar(app), view_library(app)].into()
    };

    let content = if let Some(floating) = floating_library_drag_preview(app, tokens) {
        stack![base_content, library_drag_capture_layer(), floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        base_content
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into()
}

fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let entries = app.visible_library_entries();
    let render_items = library_render_items(app, &entries);
    let window = app.visible_library_entry_window(entries.len());
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
            search_input(
                "Search library",
                &app.search_query,
                tokens,
                Message::SearchQueryChanged,
            )
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
            .style(move |_, status| pick_list_style(tokens, Class::ToolbarButton, status))
            .menu_style(move |_| menu_style(tokens)),
        )
        .push(toolbar_button("Import folder", tokens).on_press(Message::ImportFolderDialog))
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center);

    let status = app
        .library_status
        .as_deref()
        .unwrap_or("No PDFs imported yet");
    let reorder_hint = if app.can_drag_reorder_library() {
        "Manual reorder enabled"
    } else {
        "Reordering requires unfiltered Manual sort"
    };
    let mut content = column![
        header,
        row![
            text(status).size(FontSize::SM).color(tokens.text_secondary),
            text(reorder_hint)
                .size(FontSize::SM)
                .color(if app.can_drag_reorder_library() {
                    tokens.accent
                } else {
                    tokens.text_secondary
                }),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    if entries.is_empty() {
        content = content.push(empty_state(
            "Import a folder of PDFs to build your library.",
            tokens,
        ));
    } else if app.compact_view_mode {
        let mut rows = column![].spacing(Spacing::SM);
        let top_spacer = window.start as f32 * LIBRARY_LIST_ROW_HEIGHT;
        let bottom_spacer =
            entries.len().saturating_sub(window.end) as f32 * LIBRARY_LIST_ROW_HEIGHT;
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
            });
        }
        if bottom_spacer > 0.0 {
            rows = rows.push(container("").height(bottom_spacer));
        }
        content = content.push(library_scrollable(rows, tokens));
    } else {
        let mut rows = column![].spacing(Spacing::MD);
        let per_row = app.library_entries_per_row();
        let top_rows = window.start / per_row;
        let total_rows = entries.len().div_ceil(per_row);
        let bottom_rows = total_rows.saturating_sub(window.end.div_ceil(per_row));
        if top_rows > 0 {
            rows = rows.push(container("").height(top_rows as f32 * LIBRARY_GRID_ROW_HEIGHT));
        }
        for chunk in render_items[window.clone()].chunks(per_row) {
            let mut card_row = row![].spacing(Spacing::MD);
            for item in chunk.iter().cloned() {
                card_row = card_row.push(match item {
                    LibraryRenderItem::Entry(entry) => {
                        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Normal)
                    }
                    LibraryRenderItem::Ghost(entry) => {
                        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Placeholder)
                    }
                });
            }
            rows = rows.push(card_row);
        }
        if bottom_rows > 0 {
            rows = rows.push(container("").height(bottom_rows as f32 * LIBRARY_GRID_ROW_HEIGHT));
        }
        content = content.push(library_scrollable(rows, tokens));
    }

    let main_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell));

    let mut layout = row![].height(Length::Fill);
    if app.library_tag_sidebar_open {
        layout = layout.push(view_library_tag_sidebar(app));
    }
    layout.push(main_content).height(Length::Fill).into()
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

fn scroll_library_to_offset_task(offset_y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(LIBRARY_SCROLLABLE_ID),
        operation::AbsoluteOffset {
            x: Some(0.0),
            y: Some(offset_y.max(0.0)),
        },
    )
}

fn library_render_items(app: &PDFolioApp, entries: &[LibraryEntry]) -> Vec<LibraryRenderItem> {
    let Some(drag) = app.library_drag.as_ref().filter(|drag| drag.active) else {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    };
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

    items
}

fn floating_library_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app.library_drag.as_ref().filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let entry = app
        .visible_library_entries()
        .into_iter()
        .find(|entry| entry.id == drag.entry_id)?;

    let preview = if app.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    let x_offset = if app.compact_view_mode {
        LIBRARY_DRAG_PREVIEW_LIST_X_OFFSET
    } else {
        LIBRARY_DRAG_PREVIEW_GRID_X_OFFSET
    };
    let y_offset = if app.compact_view_mode {
        LIBRARY_DRAG_PREVIEW_LIST_Y_OFFSET
    } else {
        LIBRARY_DRAG_PREVIEW_GRID_Y_OFFSET
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

fn library_drag_capture_layer<'a>() -> Element<'a, Message> {
    mouse_area(container("").width(Length::Fill).height(Length::Fill))
        .on_move(Message::LibraryEntryDragMoved)
        .on_release(Message::EndLibraryEntryDrag)
        .interaction(mouse::Interaction::Grabbing)
        .into()
}

fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let sidebar_width = app.library_tag_sidebar_width;
    let heading = row![
        section_heading("Tags", tokens),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);
    let mut tags = column![
        heading,
        sidebar_button(truncate_for_width("All", sidebar_width, 0.0), tokens)
            .on_press(Message::TagFilterChanged(None)),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    for tag in app.all_tags() {
        let label = truncate_for_width(&tag, sidebar_width, 0.0);
        tags =
            tags.push(sidebar_button(label, tokens).on_press(Message::TagFilterChanged(Some(tag))));
    }

    let sidebar = container(tags)
        .width(sidebar_width)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::Sidebar));

    let handle_color = if app.resizing_library_tag_sidebar {
        tokens.focus
    } else {
        tokens.border
    };
    let handle_visual_width = if app.resizing_library_tag_sidebar {
        SIDEBAR_RESIZE_HANDLE_WIDTH
    } else {
        SIDEBAR_RESIZE_HANDLE_VISUAL_WIDTH
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
        .width(SIDEBAR_RESIZE_HANDLE_WIDTH)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(Message::BeginTagSidebarResize)
    .on_release(Message::EndTagSidebarResize)
    .interaction(mouse::Interaction::ResizingHorizontally);

    row![sidebar, resize_handle].height(Length::Fill).into()
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
    .style(move |_, _| iced::widget::button::Style {
        background: None,
        text_color: tokens.text_primary,
        border: iced::Border {
            width: 0.0,
            color: tokens.surface,
            radius: 4.0.into(),
        },
        ..iced::widget::button::Style::default()
    })
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

fn library_entry_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let title = entry_title(&entry);
    let author = entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"));
    let pages = page_count_label(&entry);
    let size = file_size_label(&entry);
    let opened = last_opened_label(&entry);
    let search_page = app.search_hit_pages.get(&entry_id).copied();
    let tags = entry.tags.clone();
    let content_alpha = library_entry_content_alpha(mode);
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let mut body = column![
        thumbnail_element(
            app,
            &entry_id,
            tokens,
            LIBRARY_CARD_THUMBNAIL_WIDTH,
            content_alpha
        ),
        truncated_title(title, LIBRARY_CARD_TITLE_WIDTH, tokens, content_alpha),
        text(author).size(FontSize::SM).color(text_secondary),
        row![
            text(pages)
                .size(FontSize::SM)
                .color(text_secondary)
                .wrapping(Wrapping::None)
                .width(Length::Fill),
            text("-").size(FontSize::SM).color(text_secondary),
            text(size)
                .size(FontSize::SM)
                .color(text_secondary)
                .wrapping(Wrapping::None)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
        text(opened).size(FontSize::SM).color(text_secondary),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    if mode != LibraryEntryRenderMode::Normal {
        body = body.push(ghost_tags_row(tags, tokens, content_alpha));
    } else {
        body = body.push(tags_row(entry_id.clone(), tags, tokens));
    }

    if mode == LibraryEntryRenderMode::Normal && app.tag_entry_id.as_ref() == Some(&entry_id) {
        body = body.push(
            text_input("Tag", &app.tag_input)
                .on_input(Message::TagInputChanged)
                .on_submit(Message::SubmitTag),
        );
    }
    if let Some(page) = search_page {
        body = body.push(
            text(format!("Match on page {}", u32::from(page) + 1))
                .size(FontSize::SM)
                .color(accent),
        );
    }

    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(LIBRARY_CARD_THUMBNAIL_WIDTH + Spacing::MD * 2.0)
    } else {
        Length::FillPortion(1)
    };
    let surface = container(body)
        .width(width)
        .style(move |_| library_entry_container_style(tokens, Class::LibraryCard, mode));

    if mode != LibraryEntryRenderMode::Normal {
        surface.into()
    } else {
        let area = mouse_area(surface)
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
    let title = entry_title(&entry);
    let details = format!(
        "{}{}",
        entry_author(&entry),
        entry
            .page_count
            .map_or(String::new(), |pages| format!(" . {pages} pages"))
    );
    let tags = entry.tags.clone();
    let progress_value = progress_fraction(&entry);
    let search_page = app.search_hit_pages.get(&entry_id).copied();
    let content_alpha = library_entry_content_alpha(mode);
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let mut detail_column = column![
        truncated_title(title, LIBRARY_ROW_TITLE_WIDTH, tokens, content_alpha),
        text(details).size(FontSize::SM).color(text_secondary),
    ]
    .spacing(Spacing::XS)
    .width(Length::Fill);
    if let Some(page) = search_page {
        detail_column = detail_column.push(
            text(format!("Match on page {}", u32::from(page) + 1))
                .size(FontSize::SM)
                .color(accent),
        );
    }
    detail_column = detail_column.push(if mode != LibraryEntryRenderMode::Normal {
        ghost_tags_row(tags, tokens, content_alpha)
    } else {
        tags_row(entry_id.clone(), tags, tokens)
    });
    let row_content = row![
        thumbnail_element(
            app,
            &entry_id,
            tokens,
            LIBRARY_ROW_THUMBNAIL_WIDTH,
            content_alpha
        ),
        detail_column,
        column![progress_bar(progress_value, tokens),]
            .spacing(Spacing::XS)
            .width(LIBRARY_ROW_PROGRESS_WIDTH),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(720.0)
    } else {
        Length::Fill
    };
    let surface = container(row_content)
        .width(width)
        .style(move |_| library_entry_container_style(tokens, Class::LibraryRow, mode));

    if mode != LibraryEntryRenderMode::Normal {
        surface.into()
    } else {
        let area = mouse_area(surface)
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
) -> iced::widget::container::Style {
    let mut style = container_style(tokens, class);
    match mode {
        LibraryEntryRenderMode::Normal => {}
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

fn library_entry_content_alpha(mode: LibraryEntryRenderMode) -> f32 {
    if mode == LibraryEntryRenderMode::Placeholder {
        LIBRARY_DRAG_PLACEHOLDER_CONTENT_ALPHA
    } else {
        1.0
    }
}

fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

fn thumbnail_element<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    width: f32,
    alpha: f32,
) -> Element<'a, Message> {
    let max_height = width * 1.32;
    if let Some(thumbnail) = app.thumbnails.get(entry_id) {
        let height = width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1));
        let display_height = height.min(max_height);
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .opacity(alpha),
        )
        .width(width)
        .align_top(display_height)
        .clip(true)
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

fn view_toolbar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let page_label = app.doc.as_ref().map_or_else(
        || String::from("- / -"),
        |doc| {
            format!(
                "{} / {}",
                u32::from(app.current_page()) + 1,
                doc.page_count()
            )
        },
    );
    let view_label = if app.compact_view_mode {
        "List"
    } else {
        "Grid"
    };
    let title = app
        .doc
        .as_ref()
        .and_then(|doc| doc.path().file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("PDF-Folio");

    let mut toolbar = row![]
        .spacing(Spacing::MD)
        .padding(Spacing::MD)
        .align_y(iced::Alignment::Center);

    if app.mode == AppMode::Viewer {
        toolbar = toolbar.push(icon_button("<", tokens).on_press(Message::BackToLibrary));
    }

    toolbar = toolbar.push(toolbar_button("Open", tokens).on_press(Message::OpenFileDialog));

    if app.mode == AppMode::Viewer {
        toolbar = toolbar
            .push(
                toolbar_button(if app.toc_open { "Hide TOC" } else { "Show TOC" }, tokens)
                    .on_press(Message::ToggleSidebar),
            )
            .push(icon_button("-", tokens).on_press(Message::ZoomOut))
            .push(
                text(format!("{} px", app.zoom_width))
                    .size(FontSize::CONTROL)
                    .color(tokens.text_secondary),
            )
            .push(icon_button("+", tokens).on_press(Message::ZoomIn))
            .push(toolbar_button(page_label, tokens).on_press(Message::OpenJumpDialog));
    }

    toolbar = toolbar
        .push(toolbar_button(view_label, tokens).on_press(Message::ToggleViewMode))
        .push(
            toolbar_button(
                match app.theme {
                    AppTheme::Light => "Dark",
                    AppTheme::Dark => "Light",
                },
                tokens,
            )
            .on_press(Message::ThemeToggled),
        )
        .push(
            text(title)
                .size(FontSize::HEADING)
                .color(tokens.text_primary)
                .width(Length::Fill),
        );
    container(toolbar)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::Toolbar))
        .into()
}

fn view_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
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
    .width(VIEWER_SIDEBAR_WIDTH)
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
    let tokens = app.theme.tokens();
    let max_page = app.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page")
            .size(FontSize::CONTROL)
            .color(tokens.text_primary),
        text_input("Page", &app.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(JUMP_INPUT_WIDTH),
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

async fn load_or_render_thumbnail(entry: LibraryEntry) -> anyhow::Result<(EntryId, RenderedPage)> {
    tokio::task::spawn_blocking(move || {
        let path = thumbnail_path(&entry.id)?;
        if path.exists() {
            let data = std::fs::read(&path)?;
            let width = 200_u16;
            let height = (data.len() / (usize::from(width) * 4)).clamp(1, usize::from(u16::MAX));
            return Ok((
                entry.id,
                RenderedPage {
                    width,
                    height: height as u16,
                    rgba: data,
                },
            ));
        }

        let doc = PdfDoc::open(&entry.path)?;
        let page = doc.render_page(0, 200)?;
        std::fs::write(path, &page.rgba)?;
        Ok((entry.id, page))
    })
    .await?
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

fn save_library_preferences_task(app: &PDFolioApp) -> Task<Message> {
    let db = Arc::clone(&app.db);
    let preferences = LibraryPreferences {
        sort_mode: app.library_sort_mode,
        layout_mode: if app.compact_view_mode {
            LibraryLayoutMode::List
        } else {
            LibraryLayoutMode::Grid
        },
        selected_folder: None,
        sidebar_width: app.library_tag_sidebar_width,
        visible_metadata_fields: vec![
            String::from("author"),
            String::from("page_count"),
            String::from("file_size"),
        ],
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
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned);
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
    entry_title(entry).to_lowercase().contains(normalized_query)
        || entry_author(entry)
            .to_lowercase()
            .contains(normalized_query)
        || entry
            .path
            .to_string_lossy()
            .to_lowercase()
            .contains(normalized_query)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(normalized_query))
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
) -> Element<'a, Message> {
    let visible = truncate_for_width(&title, width, 0.0);
    let is_truncated = visible != title;
    let text_color = with_alpha(tokens.text_primary, alpha);
    let label = text(visible)
        .size(FontSize::CONTROL)
        .color(text_color)
        .wrapping(Wrapping::None)
        .width(width);

    if !is_truncated {
        return label.into();
    }

    tooltip(
        label,
        container(text(title).size(FontSize::SM).color(text_color))
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
    const APPROX_CHAR_WIDTH: f32 = 7.5;
    const ELLIPSIS: &str = "...";

    let available = (width - reserved_width - (Spacing::MD * 2.0)).max(0.0);
    let max_chars = (available / APPROX_CHAR_WIDTH).floor().max(0.0) as usize;
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

        let (delta_x, delta_y) = scroll_delta_pixels(*delta);

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
        let tokens = self.app.theme.tokens();
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
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                text,
                modifiers,
                ..
            }) => match (key, text.as_deref()) {
                (_, Some("t") | Some("T")) if modifiers.control() && modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::ToggleTheme))
                }
                (_, Some("g") | Some("G")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::Jump))
                }
                (_, Some("+") | Some("=")) => Some(Message::ShortcutPressed(Shortcut::In)),
                (_, Some("-")) => Some(Message::ShortcutPressed(Shortcut::Out)),
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

    Subscription::batch([keyboard, watcher, sidebar_resize, library_drag])
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

fn scroll_delta_pixels(delta: mouse::ScrollDelta) -> (f32, f32) {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => (x * LINE_SCROLL_PIXELS, y * LINE_SCROLL_PIXELS),
        mouse::ScrollDelta::Pixels { x, y } => (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
