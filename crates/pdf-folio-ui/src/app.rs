//! Top-level application state and launch entrypoint.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use iced::mouse;
use iced::widget::{button, canvas, column, container, image, row, text};
use iced::{event, keyboard, Color, Element, Event, Length, Point, Rectangle, Renderer, Size};
use iced::{Subscription, Task, Theme};
use pdf_folio_core::{Annotation, PdfDoc, RenderedPage, TileCache, TileKey};
use pdf_folio_library::{Db, LibraryEntry};

use crate::messages::{Message, Shortcut};
use crate::theme::AppTheme;

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
    /// Rendered page images keyed by page and zoom width.
    pub rendered_pages: std::collections::HashMap<TileKey, RenderedPageView>,
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
    /// In-memory annotations for the open document.
    pub annotations: Vec<Annotation>,
    /// Loaded library entries.
    pub library_entries: Vec<LibraryEntry>,
    /// Current library search query.
    pub search_query: String,
    /// Search results, if search mode is active.
    pub search_results: Option<Vec<LibraryEntry>>,
    /// Current visual theme.
    pub theme: AppTheme,
    /// User settings.
    pub settings: Settings,
    /// Library database handle.
    pub db: Arc<Db>,
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
            annotations: Vec::new(),
            library_entries: Vec::new(),
            search_query: String::new(),
            search_results: None,
            theme: AppTheme::Dark,
            settings,
            db: Arc::new(Db::open_default()?),
        })
    }

    /// Creates application state and records the startup PDF path when available.
    pub fn with_initial_file(initial_file: Option<PathBuf>) -> Result<Self> {
        let mut app = Self::new()?;
        let Some(path) = initial_file.or_else(default_fixture_path) else {
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
        self.pending_renders.clear();
        self.scroll_offset = 0.0;
        self.horizontal_offset = 0.0;
        self.zoom_preview_width_px = None;
        self.zoom_generation = self.zoom_generation.wrapping_add(1);
        self.document_error = None;

        self.request_visible_pages()
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
    let startup_file = initial_file.clone().or_else(default_fixture_path);
    let app = PDFolioApp::with_initial_file(initial_file)?;

    tracing::info!(
        mode = ?app.mode,
        has_document = app.doc.is_some(),
        "Initialized PDF-Folio application state"
    );

    iced::application(
        move || {
            let task = startup_file
                .clone()
                .map(open_document_task)
                .unwrap_or_else(Task::none);
            (app.clone(), task)
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
        Message::FileSelected(path) => return open_document_task(path),
        Message::DocumentOpened(doc) => return app.open_document(doc),
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
        Message::ScrollChanged(offset) => {
            app.scroll_offset = offset;
            app.clamp_scroll_offset();
            return app.request_visible_pages();
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
        Message::ZoomSet(width) => {
            return app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        }
        _ => {}
    }

    Task::none()
}

fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let content: Element<'_, Message> = match &app.doc {
        Some(doc) => {
            let toolbar = row![
                button(text("-")).on_press(Message::ZoomOut),
                text(format!("{} px", app.zoom_width)).size(16),
                button(text("+")).on_press(Message::ZoomIn),
                text(doc.path().display().to_string()).size(16),
            ]
            .spacing(12)
            .padding(12)
            .align_y(iced::Alignment::Center);

            let viewer = canvas(ViewerCanvas { app })
                .width(Length::Fill)
                .height(Length::Fill);

            column![toolbar, viewer].into()
        }
        _ => container(
            text("Open a PDF from the command line to view it. Also: hello, empty state.").size(18),
        )
        .center(Length::Fill)
        .into(),
    };

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

async fn render_page(doc: Arc<PdfDoc>, key: TileKey) -> anyhow::Result<(TileKey, RenderedPage)> {
    let page =
        tokio::task::spawn_blocking(move || doc.render_page(key.page, key.width_px)).await??;
    Ok((key, page))
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

fn schedule_zoom_render(generation: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(140)).await;
            generation
        },
        Message::ZoomRenderSettled,
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
        frame.fill(&background, Color::from_rgb8(34, 38, 46));

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
                frame.fill(&placeholder, Color::from_rgb8(207, 212, 220));
            }

            y += height + PAGE_GAP;
        }

        vec![frame.into_geometry()]
    }
}

const PAGE_PADDING: f32 = 32.0;
const PAGE_GAP: f32 = 24.0;
const LINE_SCROLL_PIXELS: f32 = 48.0;

fn default_fixture_path() -> Option<PathBuf> {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");

    let path = fixture_dir.join("phase1-multipage.pdf");
    if path.exists() {
        return Some(path);
    }

    let path = fixture_dir.join("phase1-single-page.pdf");

    path.exists().then_some(path)
}

fn subscription(_app: &PDFolioApp) -> Subscription<Message> {
    event::listen_with(|event, status, _window| {
        if status == event::Status::Captured {
            return None;
        }

        match event {
            Event::Keyboard(keyboard::Event::KeyPressed { key, text, .. }) => {
                match (key, text.as_deref()) {
                    (_, Some("+") | Some("=")) => Some(Message::ShortcutPressed(Shortcut::In)),
                    (_, Some("-")) => Some(Message::ShortcutPressed(Shortcut::Out)),
                    (keyboard::Key::Character(value), _) if value.as_str() == "0" => {
                        Some(Message::ShortcutPressed(Shortcut::Reset))
                    }
                    _ => None,
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            _ => None,
        }
    })
}

fn scroll_delta_pixels(delta: mouse::ScrollDelta) -> (f32, f32) {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => (x * LINE_SCROLL_PIXELS, y * LINE_SCROLL_PIXELS),
        mouse::ScrollDelta::Pixels { x, y } => (x, y),
    }
}
