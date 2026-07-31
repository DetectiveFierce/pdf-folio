//! Open-document runtime state for the viewer subsystem.
//!
//! [`ViewerRuntime`] is the long-lived bag mounted on
//! [`crate::PDFolioApp::viewer`]. It holds the open [`PdfDoc`], tile cache and
//! rendered page map, viewport and scroll offsets, zoom UI state, text layers
//! for selection/find, and outline/sidebar chrome flags.
//!
//! Behavior methods live elsewhere: navigation/zoom on `PDFolioApp` in
//! [`super::navigation`], find/open helpers in [`super::state`], async open and
//! render in [`super::tasks`]. Session restore snapshots a subset via
//! [`crate::shell::session::SessionViewer`].

use crate::*;

/// Runtime state owned by the PDF viewer surface.
///
/// Cleared or reinitialized when a new document opens or the user returns to
/// the library without keeping the document hot. `viewport_*` tracks the
/// application window; `viewer_viewport_*` tracks the canvas area inside chrome.
#[derive(Debug, Clone)]
pub struct ViewerRuntime {
    /// Open PDF document, if any.
    pub doc: Option<Arc<PdfDoc>>,
    /// Library entry id when opened from the library (for progress tracking).
    pub current_entry_id: Option<EntryId>,
    /// Filesystem path of the open document.
    pub current_document_path: Option<PathBuf>,
    /// Raster tiles keyed by page and render width.
    pub rendered_pages: HashMap<TileKey, RenderedPageView>,
    /// Cached aspect ratios used for layout before tiles arrive.
    pub page_aspect_ratios: Vec<f32>,
    /// Application window height (logical pixels).
    pub viewport_height: f32,
    /// Application window width (logical pixels).
    pub viewport_width: f32,
    /// Viewer canvas height excluding toolbar/sidebar.
    pub viewer_viewport_height: f32,
    /// Viewer canvas width excluding toolbar/sidebar.
    pub viewer_viewport_width: f32,
    /// User-visible document error banner text.
    pub document_error: Option<String>,
    /// True while an open task is in flight.
    pub pending_document_open: bool,
    /// When the current open attempt started (status UI).
    pub document_open_started_at: Option<Instant>,
    /// Error strings the user has dismissed (suppress re-show).
    pub dismissed_document_errors: HashSet<String>,
    /// LRU tile cache shared with the core renderer.
    pub cache: TileCache,
    /// Current page index in page-scroll mode.
    pub page_scroll_page: u16,
    /// Vertical scroll offset of the document scrollable.
    pub scroll_offset: f32,
    /// Horizontal scroll offset of the document scrollable.
    pub horizontal_offset: f32,
    /// Continuous vs page vs horizontal vs wrapped scrolling.
    pub viewer_scroll_mode: ViewerScrollMode,
    /// Single-page vs odd/even two-page spreads.
    pub viewer_spread_mode: ViewerSpreadMode,
    /// Target page width in logical pixels for layout/zoom.
    pub zoom_width: u16,
    /// Active named zoom preset, if the user selected one.
    pub active_zoom_preset: Option<ZoomPreset>,
    /// Whether the toolbar zoom percent field is being edited.
    pub zoom_editing: bool,
    /// Text contents of the zoom percent input.
    pub zoom_input: String,
    /// Whether the zoom preset dropdown is open.
    pub zoom_menu_open: bool,
    /// Width used to keep showing an old tile while debounced zoom settles.
    pub zoom_preview_width_px: Option<u16>,
    /// Monotonic generation to ignore stale zoom-settled messages.
    pub zoom_generation: u64,
    /// Previous vertical offset (direction heuristics / progress).
    pub last_scroll_offset: f32,
    /// DPI scale factor passed to iced.
    pub scale_factor: f32,
    /// Current keyboard modifiers (Ctrl-wheel zoom, Shift-pan).
    pub modifiers: keyboard::Modifiers,
    /// Active PDF text selection drag, if any.
    pub viewer_text_selection: Option<ViewerTextSelection>,
    /// Extracted text layers keyed by zero-based page index.
    pub viewer_text_layers: HashMap<u16, Arc<PageTextLayer>>,
    /// Pages with an in-flight text-layer extraction task.
    pub pending_text_layers: HashSet<u16>,
    /// True while waiting for text layers before completing a copy.
    pub viewer_copy_pending: bool,
    /// Find-in-document bar state and matches.
    pub viewer_find: ViewerFindState,
    /// In-flight page renders mapped to optional zoom generation.
    pub pending_renders: HashMap<TileKey, Option<u64>>,
    /// When each tile began its fade-in animation.
    pub page_fade_started: HashMap<TileKey, Instant>,
    /// Whether the outline/thumbnail sidebar is open.
    pub toc_open: bool,
    /// Active viewer sidebar tab (contents vs thumbnails).
    pub viewer_sidebar_tab: ViewerSidebarTab,
    /// Document outline tree from the PDF.
    pub outline: Vec<OutlineNode>,
    /// Expanded outline node paths (indices into the tree).
    pub expanded_outline_paths: HashSet<Vec<usize>>,
    /// Whether the jump-to-page overlay is open.
    pub jump_dialog_open: bool,
    /// Whether the toolbar page field is being edited inline.
    pub page_input_editing: bool,
    /// Jump / page input text (1-based page number as typed).
    pub jump_input: String,
    /// Monotonic generation for debounced reading-progress writes.
    pub progress_save_generation: u64,
    /// Last page index written (or accepted) for library reading progress.
    pub last_saved_progress_page: Option<u16>,
    /// Monotonic generation for progressive find text-layer loading.
    pub find_text_generation: u64,
    /// Monotonic generation for the open document identity.
    ///
    /// Bumped whenever a new PDF is installed so in-flight text-layer tasks
    /// from a previous document cannot mutate the new document's find/selection
    /// state when they complete.
    pub document_generation: u64,
    /// Residual signed wheel delta for page-mode turns (positive = next page).
    ///
    /// Accumulates trackpad/momentum micro-events until a page-turn threshold is
    /// reached so one gesture does not skip multiple pages.
    pub page_mode_wheel_accum: f32,
    /// When the last page-mode page turn was applied (momentum cooldown).
    pub page_mode_wheel_turned_at: Option<Instant>,
}
