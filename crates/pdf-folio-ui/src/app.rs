//! Top-level application state and launch entrypoint.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use pdf_folio_core::{Annotation, PdfDoc, TileCache, TileKey};
use pdf_folio_library::{Db, LibraryEntry};

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

/// PDF-Folio application state.
#[derive(Debug)]
pub struct PDFolioApp {
    /// Current view mode.
    pub mode: AppMode,
    /// Open document.
    pub doc: Option<Arc<PdfDoc>>,
    /// Rendered tile cache.
    pub cache: TileCache,
    /// Current vertical scroll offset.
    pub scroll_offset: f32,
    /// Current rendered page width.
    pub zoom_width: u16,
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
            cache: TileCache::with_default_capacity(),
            scroll_offset: 0.0,
            zoom_width: settings.default_zoom_width,
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
}

/// Launches the PDF-Folio UI.
///
/// # Errors
///
/// Returns an error when startup state cannot be created.
pub fn run(initial_file: Option<PathBuf>) -> Result<()> {
    let mut app = PDFolioApp::new()?;
    if let Some(path) = initial_file {
        app.doc = Some(Arc::new(PdfDoc::open(&path)?));
        app.mode = AppMode::Viewer;
    }

    tracing::info!(
        mode = ?app.mode,
        has_document = app.doc.is_some(),
        "Initialized PDF-Folio application state"
    );
    Ok(())
}
