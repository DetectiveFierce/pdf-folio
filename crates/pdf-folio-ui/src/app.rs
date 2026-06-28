//! Top-level application state and launch entrypoint.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use iced::futures::SinkExt;
use iced::mouse;
use iced::stream;
use iced::widget::{button, canvas, column, container, image, row, scrollable, text, text_input};
use iced::{
    event, keyboard, Background, Border, Color, Element, Event, Length, Point, Rectangle, Renderer,
    Size,
};
use iced::{Subscription, Task, Theme};
use pdf_folio_core::{Annotation, OutlineNode, PdfDoc, RenderedPage, TileCache, TileKey};
use pdf_folio_library::{
    hash_file, scan_pdf_files, thumbnail_path, Db, EntryId, ImportSummary, ImportedEntry,
    IndexDocument, LibraryEntry, LibraryWatchEvent, LibraryWatcher, NewLibraryEntry, SearchIndex,
};

use crate::messages::{Message, Shortcut};
use crate::theme::{AppTheme, ThemeTokens};

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

impl PDFolioApp {
    /// Creates application state using the default database location.
    ///
    /// # Errors
    ///
    /// Returns an error when the library database cannot be opened.
    pub fn new() -> Result<Self> {
        let settings = Settings::default();
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
            compact_view_mode: false,
            jump_dialog_open: false,
            jump_input: String::new(),
            annotations: Vec::new(),
            library_entries: Vec::new(),
            search_query: String::new(),
            search_results: None,
            search_hit_pages: HashMap::new(),
            search_generation: 0,
            library_scroll_offset: 0.0,
            library_viewport_height: 720.0,
            thumbnails: HashMap::new(),
            pending_thumbnails: HashSet::new(),
            active_tag_filter: None,
            tag_entry_id: None,
            tag_input: String::new(),
            library_status: None,
            last_library_click: None,
            theme: AppTheme::Dark,
            settings,
            db: Arc::new(Db::open_default()?),
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
        let bottom = top + self.viewport_height.max(1.0) + PAGE_GAP;
        let mut y = PAGE_PADDING;
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

            y = page_bottom + PAGE_GAP;
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
            .map(|page| self.page_height(page) + PAGE_GAP)
            .sum();
        pages + PAGE_PADDING * 2.0
    }

    fn content_width(&self) -> f32 {
        f32::from(self.zoom_width) + PAGE_PADDING * 2.0
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
            3
        }
    }

    fn library_row_height(&self) -> f32 {
        if self.compact_view_mode {
            LIBRARY_LIST_ROW_HEIGHT
        } else {
            LIBRARY_GRID_ROW_HEIGHT
        }
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
        Task::perform(
            async move { tokio::task::spawn_blocking(move || db.get_all_entries()).await? },
            |result| match result {
                Ok(entries) => Message::LibraryLoaded(entries),
                Err(error) => Message::LibraryError(error.to_string()),
            },
        )
    }

    fn page_top(&self, target_page: u16) -> f32 {
        let mut y = PAGE_PADDING;
        for page in 0..target_page {
            y += self.page_height(page) + PAGE_GAP;
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
            (app.clone(), Task::batch([open_task, load_task]))
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
    .window_size([960.0, 1080.0])
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
        }
        Message::LibraryLoaded(entries) => {
            app.library_entries = entries;
            app.library_status = Some(format!("{} PDFs in library", app.library_entries.len()));
            if !app.search_query.trim().is_empty() {
                return Task::done(Message::SearchDebounced(app.search_query.clone()));
            }
            return app.request_visible_thumbnails();
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
        Message::SearchQueryChanged(query) => {
            app.search_query = query;
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
                return Task::perform(search_library_task(db, query), |result| match result {
                    Ok((entries, hit_pages)) => Message::SearchResults { entries, hit_pages },
                    Err(error) => Message::LibraryError(error.to_string()),
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
            viewport_height,
        } => {
            app.library_scroll_offset = offset_y.max(0.0);
            app.library_viewport_height = viewport_height.max(1.0);
            return app.request_visible_thumbnails();
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
        Message::ProgressSaved => {}
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
    let content: Element<'_, Message> = if app.doc.is_some() {
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

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens.background, tokens.text_primary, tokens.border, 0.0))
        .into()
}

fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let entries = app.visible_library_entries();
    let window = app.visible_library_entry_window(entries.len());
    let sidebar = view_library_tag_sidebar(app);
    let header = row![
        text_input("Search library", &app.search_query)
            .on_input(Message::SearchQueryChanged)
            .width(Length::Fill),
        shell_button("Import folder", tokens).on_press(Message::ImportFolderDialog),
    ]
    .spacing(10)
    .align_y(iced::Alignment::Center);

    let status = app
        .library_status
        .as_deref()
        .unwrap_or("No PDFs imported yet");
    let mut content = column![header, text(status).size(13).color(tokens.text_secondary),]
        .spacing(10)
        .padding(12);

    if entries.is_empty() {
        content = content.push(
            container(text("Import a folder of PDFs to build your library.").size(16))
                .center(Length::Fill)
                .height(Length::Fill),
        );
    } else if app.compact_view_mode {
        let mut rows = column![].spacing(6);
        let top_spacer = window.start as f32 * LIBRARY_LIST_ROW_HEIGHT;
        let bottom_spacer =
            entries.len().saturating_sub(window.end) as f32 * LIBRARY_LIST_ROW_HEIGHT;
        if top_spacer > 0.0 {
            rows = rows.push(container("").height(top_spacer));
        }
        for entry in entries[window.clone()].iter() {
            rows = rows.push(library_entry_row(app, entry.clone(), tokens));
        }
        if bottom_spacer > 0.0 {
            rows = rows.push(container("").height(bottom_spacer));
        }
        content = content.push(library_scrollable(rows));
    } else {
        let mut rows = column![].spacing(10);
        let per_row = app.library_entries_per_row();
        let top_rows = window.start / per_row;
        let total_rows = entries.len().div_ceil(per_row);
        let bottom_rows = total_rows.saturating_sub(window.end.div_ceil(per_row));
        if top_rows > 0 {
            rows = rows.push(container("").height(top_rows as f32 * LIBRARY_GRID_ROW_HEIGHT));
        }
        for chunk in entries[window.clone()].chunks(per_row) {
            let mut card_row = row![].spacing(10);
            for entry in chunk {
                card_row = card_row.push(library_entry_card(app, entry.clone(), tokens));
            }
            rows = rows.push(card_row);
        }
        if bottom_rows > 0 {
            rows = rows.push(container("").height(bottom_rows as f32 * LIBRARY_GRID_ROW_HEIGHT));
        }
        content = content.push(library_scrollable(rows));
    }

    row![
        sidebar,
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container_style(
                tokens.background,
                tokens.text_primary,
                tokens.border,
                0.0
            ))
    ]
    .height(Length::Fill)
    .into()
}

fn library_scrollable<'a>(content: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    scrollable(content)
        .height(Length::Fill)
        .on_scroll(|viewport| {
            let offset = viewport.absolute_offset();
            let bounds = viewport.bounds();
            Message::LibraryScrolled {
                offset_y: offset.y,
                viewport_height: bounds.height,
            }
        })
        .into()
}

fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let mut tags = column![
        text("Tags").size(16).color(tokens.text_primary),
        shell_button("All", tokens).on_press(Message::TagFilterChanged(None)),
    ]
    .spacing(8)
    .padding(10);

    for tag in app.all_tags() {
        tags = tags
            .push(shell_button(tag.clone(), tokens).on_press(Message::TagFilterChanged(Some(tag))));
    }

    container(tags)
        .width(180)
        .height(Length::Fill)
        .style(move |_| container_style(tokens.surface, tokens.text_primary, tokens.border, 0.0))
        .into()
}

fn library_entry_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let title = entry_title(&entry);
    let author = entry
        .author
        .clone()
        .unwrap_or_else(|| String::from("Unknown author"));
    let progress = progress_label(&entry);
    let search_page = app.search_hit_pages.get(&entry_id).copied();
    let tags = entry.tags.clone();
    let mut body = column![
        thumbnail_element(app, &entry_id, tokens, 132.0),
        text(title).size(15).color(tokens.text_primary),
        text(author).size(12).color(tokens.text_secondary),
        text(progress).size(12).color(tokens.text_secondary),
        tags_row(entry_id.clone(), tags, tokens),
    ]
    .spacing(6)
    .padding(10);

    if app.tag_entry_id.as_ref() == Some(&entry_id) {
        body = body.push(
            text_input("Tag", &app.tag_input)
                .on_input(Message::TagInputChanged)
                .on_submit(Message::SubmitTag),
        );
    }
    if let Some(page) = search_page {
        body = body.push(
            text(format!("Match on page {}", u32::from(page) + 1))
                .size(12)
                .color(tokens.accent),
        );
    }

    button(body)
        .on_press(Message::LibraryEntryClicked(entry_id))
        .style(move |_, status| button_style(tokens, status))
        .width(Length::FillPortion(1))
        .into()
}

fn library_entry_row<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let title = entry_title(&entry);
    let details = format!(
        "{}{}",
        entry.author.as_deref().unwrap_or("Unknown author"),
        entry
            .page_count
            .map_or(String::new(), |pages| format!(" . {pages} pages"))
    );
    let tags = entry.tags.clone();
    let progress = progress_label(&entry);
    let search_page = app.search_hit_pages.get(&entry_id).copied();
    let mut detail_column = column![
        text(title).size(15).color(tokens.text_primary),
        text(details).size(12).color(tokens.text_secondary),
    ]
    .spacing(3)
    .width(Length::Fill);
    if let Some(page) = search_page {
        detail_column = detail_column.push(
            text(format!("Match on page {}", u32::from(page) + 1))
                .size(12)
                .color(tokens.accent),
        );
    }
    detail_column = detail_column.push(tags_row(entry_id.clone(), tags, tokens));
    let row_content = row![
        thumbnail_element(app, &entry_id, tokens, 52.0),
        detail_column,
        text(progress).size(12).color(tokens.text_secondary),
    ]
    .spacing(10)
    .padding(8)
    .align_y(iced::Alignment::Center);

    button(row_content)
        .on_press(Message::LibraryEntryClicked(entry_id))
        .style(move |_, status| button_style(tokens, status))
        .width(Length::Fill)
        .into()
}

fn thumbnail_element<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    width: f32,
) -> Element<'a, Message> {
    if let Some(thumbnail) = app.thumbnails.get(entry_id) {
        let height = width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1));
        image(thumbnail.handle.clone())
            .width(width)
            .height(height)
            .into()
    } else {
        container(text("PDF").size(13).color(tokens.text_secondary))
            .center(width)
            .height(width * 1.32)
            .style(move |_| {
                container_style(
                    tokens.placeholder,
                    tokens.text_secondary,
                    tokens.border,
                    1.0,
                )
            })
            .into()
    }
}

fn tags_row<'a>(entry_id: EntryId, tags: Vec<String>, tokens: ThemeTokens) -> Element<'a, Message> {
    let mut row = row![].spacing(4).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(
            button(text(tag.clone()).size(11).color(tokens.text_primary))
                .on_press(Message::TagFilterChanged(Some(tag.clone())))
                .style(move |_, status| button_style(tokens, status)),
        );
    }
    row.push(
        button(text("+ tag").size(11).color(tokens.text_secondary))
            .on_press(Message::StartTagEntry(entry_id))
            .style(move |_, status| button_style(tokens, status)),
    )
    .into()
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
        .spacing(10)
        .padding(10)
        .align_y(iced::Alignment::Center);

    if app.mode == AppMode::Viewer {
        toolbar = toolbar.push(shell_button("<", tokens).on_press(Message::BackToLibrary));
    }

    toolbar = toolbar.push(shell_button("Open", tokens).on_press(Message::OpenFileDialog));

    if app.mode == AppMode::Viewer {
        toolbar = toolbar
            .push(
                shell_button(if app.toc_open { "Hide TOC" } else { "Show TOC" }, tokens)
                    .on_press(Message::ToggleSidebar),
            )
            .push(shell_button("-", tokens).on_press(Message::ZoomOut))
            .push(
                text(format!("{} px", app.zoom_width))
                    .size(15)
                    .color(tokens.text_secondary),
            )
            .push(shell_button("+", tokens).on_press(Message::ZoomIn))
            .push(shell_button(page_label, tokens).on_press(Message::OpenJumpDialog));
    }

    toolbar = toolbar
        .push(shell_button(view_label, tokens).on_press(Message::ToggleViewMode))
        .push(
            shell_button(
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
                .size(16)
                .color(tokens.text_primary)
                .width(Length::Fill),
        );
    container(toolbar)
        .width(Length::Fill)
        .style(move |_| container_style(tokens.surface, tokens.text_primary, tokens.border, 0.0))
        .into()
}

fn view_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let body: Element<'_, Message> = if app.outline.is_empty() {
        container(
            text("No table of contents")
                .size(14)
                .color(tokens.text_secondary),
        )
        .padding(12)
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
        .into()
    };

    container(
        column![text("Contents").size(16).color(tokens.text_primary), body]
            .spacing(8)
            .padding(10),
    )
    .width(240)
    .height(Length::Fill)
    .style(move |_| container_style(tokens.surface, tokens.text_primary, tokens.border, 0.0))
    .into()
}

fn outline_list<'a>(
    nodes: &'a [OutlineNode],
    depth: u16,
    parent_path: Vec<usize>,
    expanded_paths: &'a HashSet<Vec<usize>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut list = column![].spacing(4);

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
            .size(13)
            .color(tokens.text_secondary),
            text(label).size(14).color(tokens.text_primary)
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        if let Some(page) = node.page {
            row = row.push(
                text(format!("{}", u32::from(page) + 1))
                    .size(12)
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
    button(content)
        .on_press(message)
        .style(move |_, status| button_style(tokens, status))
        .width(Length::Fill)
}

fn view_jump_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens();
    let max_page = app.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page").size(15).color(tokens.text_primary),
        text_input("Page", &app.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .width(90),
        text(format!("of {max_page}"))
            .size(14)
            .color(tokens.text_secondary),
        shell_button("Go", tokens).on_press(Message::SubmitJump),
        shell_button("Cancel", tokens).on_press(Message::CloseOverlay),
    ]
    .spacing(10)
    .padding(10)
    .align_y(iced::Alignment::Center);

    container(dialog)
        .width(Length::Fill)
        .style(move |_| container_style(tokens.surface, tokens.text_primary, tokens.border, 0.0))
        .into()
}

fn shell_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(text(label.into()).color(tokens.text_primary))
        .style(move |_, status| button_style(tokens, status))
}

fn container_style(
    background: Color,
    text_color: Color,
    border_color: Color,
    border_width: f32,
) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text_color),
        border: Border {
            width: border_width,
            color: border_color,
            ..Border::default()
        },
        ..container::Style::default()
    }
}

fn button_style(tokens: ThemeTokens, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Active => tokens.surface,
        button::Status::Hovered => mix_color(tokens.surface, tokens.accent, 0.16),
        button::Status::Pressed => mix_color(tokens.surface, tokens.accent, 0.28),
        button::Status::Disabled => tokens.background,
    };
    let text_color = if matches!(status, button::Status::Disabled) {
        tokens.text_secondary
    } else {
        tokens.text_primary
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            width: 1.0,
            color: tokens.border,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

fn mix_color(base: Color, overlay: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color {
        r: base.r + (overlay.r - base.r) * amount,
        g: base.g + (overlay.g - base.g) * amount,
        b: base.b + (overlay.b - base.b) * amount,
        a: base.a + (overlay.a - base.a) * amount,
    }
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

    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.clone(),
        title: title.clone(),
        author: None,
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
            author: String::new(),
            body,
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;

    Ok(ImportedEntry { id, path, inserted })
}

async fn search_library_task(
    db: Arc<Db>,
    query: String,
) -> anyhow::Result<(Vec<LibraryEntry>, HashMap<EntryId, u16>)> {
    tokio::task::spawn_blocking(move || {
        let entries = db.get_all_entries()?;
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
        || entry
            .author
            .as_deref()
            .is_some_and(|author| author.to_lowercase().contains(normalized_query))
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
    entry.title.clone().unwrap_or_else(|| {
        entry
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned()
    })
}

fn progress_label(entry: &LibraryEntry) -> String {
    if entry.missing {
        return String::from("Missing");
    }

    entry.page_count.map_or_else(
        || String::from("Not opened"),
        |pages| {
            if pages == 0 {
                String::from("Not opened")
            } else {
                format!("Page {} / {}", u32::from(entry.last_page) + 1, pages)
            }
        },
    )
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
        frame.fill(&background, tokens.canvas);

        let Some(doc) = &self.app.doc else {
            return vec![frame.into_geometry()];
        };

        let page_width = f32::from(self.app.zoom_width);
        let x = ((bounds.width - page_width) / 2.0).max(PAGE_PADDING) - self.app.horizontal_offset;
        let mut y = PAGE_PADDING - self.app.scroll_offset;

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
                    Point::new(rect.x + 2.0, rect.y + 2.0),
                    Size::new(rect.width, rect.height),
                );
                frame.fill(&shadow, Color::from_rgba8(0, 0, 0, 0.20));
                let placeholder = canvas::Path::rectangle(rect.position(), rect.size());
                frame.fill(&placeholder, tokens.placeholder);
            }

            y += height + PAGE_GAP;
        }

        vec![frame.into_geometry()]
    }
}

const PAGE_PADDING: f32 = 32.0;
const PAGE_GAP: f32 = 24.0;
const LINE_SCROLL_PIXELS: f32 = 48.0;
const LIBRARY_OVERSCAN_ROWS: usize = 4;
const LIBRARY_GRID_ROW_HEIGHT: f32 = 290.0;
const LIBRARY_LIST_ROW_HEIGHT: f32 = 92.0;

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

    Subscription::batch([keyboard, watcher])
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
