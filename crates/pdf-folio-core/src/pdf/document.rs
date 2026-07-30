//! PDF document wrapper, text/outline extraction, and render output types.
//!
//! [`PdfDoc`] is a lightweight handle: it stores only the on-disk path and
//! page count. Each method that needs page data re-opens the file under the
//! process-wide Pdfium lock, so documents are cheap to clone and safe to
//! share across threads that serialize on that lock.
//!
//! # Key types
//!
//! - [`PdfDoc`] — open, inspect aspect ratio, render pages, extract text.
//! - [`RenderedPage`] — RGBA8 bitmap plus pixel dimensions.
//! - [`PageTextLayer`] / [`PageTextChar`] — per-character text with
//!   normalized [`super::geometry::TextRect`] bounds for selection UI.
//! - [`OutlineNode`] — hierarchical bookmark tree with optional page targets.
//!
//! # See also
//!
//! - [`super::renderer::TileCache`] for caching render output in the viewer.
//! - [`crate::db::import`] for turning file paths into library entries.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::{anyhow, Context, Result};
use pdfium_render::prelude::{
    PdfBookmark, PdfDocument, PdfDocumentMetadataTagType, PdfRenderConfig, Pdfium,
};

use super::geometry::TextRect;

/// A rendered PDF page bitmap in tightly packed RGBA8 (4 bytes per pixel).
///
/// `rgba.len()` is expected to equal `width * height * 4`. Produced by
/// [`PdfDoc::render_page`] and commonly stored in [`super::renderer::TileCache`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels (depends on page aspect ratio and width).
    pub height: u16,
    /// Pixel data in row-major RGBA8 order.
    pub rgba: Vec<u8>,
}

/// A single extracted text character and its normalized page-relative bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct PageTextChar {
    /// Zero-based Pdfium character index within the page text run.
    pub index: usize,
    /// Unicode string for the character (may be empty for some control glyphs).
    pub text: String,
    /// Loose glyph bounds suitable for hit-testing and selection highlighting.
    pub bounds: TextRect,
}

/// Per-character text layer for one PDF page, used by text selection UI.
///
/// Character bounds are normalized to the unit square via [`TextRect`].
/// Characters whose bounds Pdfium cannot resolve are omitted.
#[derive(Debug, Clone, PartialEq)]
pub struct PageTextLayer {
    /// Zero-based page index.
    pub page: u16,
    /// Page width in PDF points (minimum 1.0 after clamping).
    pub width_points: f32,
    /// Page height in PDF points (minimum 1.0 after clamping).
    pub height_points: f32,
    /// Characters in Pdfium text order (not necessarily visual reading order).
    pub chars: Vec<PageTextChar>,
}

/// A node in a PDF outline (bookmark) tree.
///
/// `page` is `None` when the bookmark has no resolvable destination page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineNode {
    /// Display title for the outline entry.
    pub title: String,
    /// Target zero-based page index, if known.
    pub page: Option<u16>,
    /// Nested child outline entries (may be empty).
    pub children: Vec<OutlineNode>,
}

/// A loaded PDF document handle (path + page count; reopens file per operation).
///
/// Thread-safe to clone. Concurrent Pdfium calls are serialized by an internal
/// process-wide mutex. Requires a system (or adjacent) Pdfium shared library.
#[derive(Debug, Clone)]
pub struct PdfDoc {
    path: PathBuf,
    page_count: u16,
}

impl PdfDoc {
    /// Opens a PDF document from disk and records basic metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when the file does not exist, Pdfium cannot be bound, or the document
    /// cannot be loaded.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!("Could not open file: the path does not exist."));
        }

        let _guard = Self::pdfium_guard();
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(path, None)
            .with_context(|| format!("Could not open PDF: {}.", path.display()))?;
        let page_count = u16::try_from(document.pages().len())
            .context("Could not open PDF: the document has too many pages.")?;

        Ok(Self {
            path: path.to_path_buf(),
            page_count,
        })
    }

    /// Filesystem path passed to [`PdfDoc::open`] (not canonicalized).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Page count captured at open time (not re-read on each call).
    pub fn page_count(&self) -> u16 {
        self.page_count
    }

    /// Renders page `index` to RGBA8 scaled to `width_px` (height follows aspect ratio).
    ///
    /// # Errors
    ///
    /// Returns an error when `width_px` is zero, the page is out of range, Pdfium
    /// cannot load the document, or rendering fails.
    pub fn render_page(&self, index: u16, width_px: u16) -> Result<RenderedPage> {
        if width_px == 0 {
            return Err(anyhow!(
                "Could not render page: width must be greater than zero."
            ));
        }

        self.with_document(|document| {
            let page = document.pages().get(i32::from(index)).with_context(|| {
                format!(
                    "Could not render page {}: the page does not exist.",
                    index + 1
                )
            })?;
            let bitmap = page.render_with_config(
                &PdfRenderConfig::new().set_target_width(i32::from(width_px)),
            )?;

            Ok(RenderedPage {
                width: bitmap.width() as u16,
                height: bitmap.height() as u16,
                rgba: bitmap.as_rgba_bytes(),
            })
        })
    }

    /// Returns the page width divided by page height.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document or page.
    pub fn page_aspect_ratio(&self, index: u16) -> Result<f32> {
        self.with_document(|document| {
            let page = document.pages().get(i32::from(index)).with_context(|| {
                format!(
                    "Could not inspect page {}: the page does not exist.",
                    index + 1
                )
            })?;

            Ok(page.width().value / page.height().value)
        })
    }

    /// Returns the document outline tree.
    ///
    /// # Errors
    ///
    /// Returns an error when the document cannot be opened.
    pub fn outline(&self) -> Result<Vec<OutlineNode>> {
        self.with_document(|document| {
            let Some(root) = document.bookmarks().root() else {
                return Ok(Vec::new());
            };

            Ok(Self::outline_nodes_from_first(root))
        })
    }

    /// Extracts text from a page.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document, page, or page text.
    pub fn text_on_page(&self, index: u16) -> Result<String> {
        self.with_document(|document| {
            let page = document.pages().get(i32::from(index)).with_context(|| {
                format!(
                    "Could not read page {}: the page does not exist.",
                    index + 1
                )
            })?;

            let text = page.text()?.all();
            Ok(text)
        })
    }

    /// Returns per-character text and normalized character bounds for a page.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document, page, or page text.
    pub fn text_layer(&self, index: u16) -> Result<PageTextLayer> {
        self.with_document(|document| {
            let page = document.pages().get(i32::from(index)).with_context(|| {
                format!(
                    "Could not read page {}: the page does not exist.",
                    index + 1
                )
            })?;
            let width_points = page.width().value.max(1.0);
            let height_points = page.height().value.max(1.0);
            let text = page.text()?;
            let mut chars = Vec::with_capacity(text.chars().len());

            for character in text.chars().iter() {
                let bounds = character
                    .loose_bounds()
                    .or_else(|_| character.tight_bounds())
                    .ok();
                let Some(bounds) = bounds else {
                    continue;
                };
                let text = character
                    .unicode_char()
                    .map(|character| character.to_string())
                    .unwrap_or_default();
                let x = bounds.left().value / width_points;
                let y = (height_points - bounds.top().value) / height_points;
                let width = bounds.width().value / width_points;
                let height = bounds.height().value / height_points;

                chars.push(PageTextChar {
                    index: character.index(),
                    text,
                    bounds: TextRect {
                        x: x.clamp(0.0, 1.0),
                        y: y.clamp(0.0, 1.0),
                        width: width.clamp(0.0, 1.0),
                        height: height.clamp(0.0, 1.0),
                    },
                });
            }

            Ok(PageTextLayer {
                page: index,
                width_points,
                height_points,
                chars,
            })
        })
    }

    /// Returns the document author metadata, if present and non-empty.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document.
    pub fn metadata_author(&self) -> Result<Option<String>> {
        self.with_document(|document| {
            Ok(document
                .metadata()
                .get(PdfDocumentMetadataTagType::Author)
                .map(|tag| tag.value().trim().to_owned())
                .filter(|author| !author.is_empty()))
        })
    }

    /// Returns the document title metadata, if present and non-empty.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document.
    pub fn metadata_title(&self) -> Result<Option<String>> {
        self.with_document(|document| {
            Ok(document
                .metadata()
                .get(PdfDocumentMetadataTagType::Title)
                .map(|tag| tag.value().trim().to_owned())
                .filter(|title| !title.is_empty()))
        })
    }

    fn with_document<T>(&self, f: impl for<'a> FnOnce(PdfDocument<'a>) -> Result<T>) -> Result<T> {
        let _guard = Self::pdfium_guard();
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("Could not open PDF: {}.", self.path.display()))?;
        f(document)
    }

    fn outline_nodes_from_first(first: PdfBookmark<'_>) -> Vec<OutlineNode> {
        let mut nodes = Vec::new();
        let mut current = Some(first);

        while let Some(bookmark) = current {
            current = bookmark.next_sibling();
            nodes.push(Self::outline_node_from_bookmark(bookmark));
        }

        nodes
    }

    fn outline_node_from_bookmark(bookmark: PdfBookmark<'_>) -> OutlineNode {
        let page = bookmark
            .destination()
            .and_then(|destination| destination.page_index().ok())
            .and_then(|index| u16::try_from(index).ok());
        let children = bookmark
            .first_child()
            .map(Self::outline_nodes_from_first)
            .unwrap_or_default();

        OutlineNode {
            title: bookmark.title().unwrap_or_default(),
            page,
            children,
        }
    }

    fn pdfium() -> Result<&'static Pdfium> {
        static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

        PDFIUM
            .get_or_init(|| {
                let bindings = Pdfium::bind_to_system_library().or_else(|_| {
                    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
                });

                bindings.map(Pdfium::new).map_err(|error| {
                    format!(
                        "Could not initialize Pdfium. Install libpdfium, set LD_LIBRARY_PATH to \
                         a Pdfium build, or place the Pdfium shared library next to the binary: \
                         {error}"
                    )
                })
            })
            .as_ref()
            .map_err(|error| anyhow!("{error}"))
    }

    fn pdfium_guard() -> MutexGuard<'static, ()> {
        static PDFIUM_MUTEX: Mutex<()> = Mutex::new(());

        PDFIUM_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
