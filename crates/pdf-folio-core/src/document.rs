//! PDF document wrapper and render output types.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::{anyhow, Context, Result};
use pdfium_render::prelude::{PdfBookmark, PdfDocument, PdfRenderConfig, Pdfium};

/// A rendered PDF page in RGBA8 format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels.
    pub height: u16,
    /// Pixel data in RGBA8 order.
    pub rgba: Vec<u8>,
}

/// A node in a PDF outline tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineNode {
    /// Display title for the outline entry.
    pub title: String,
    /// Target zero-based page index, if known.
    pub page: Option<u16>,
    /// Child outline entries.
    pub children: Vec<OutlineNode>,
}

/// A loaded PDF document.
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

    /// Returns the path used to open the document.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the number of pages in the document.
    pub fn page_count(&self) -> u16 {
        self.page_count
    }

    /// Renders a page to RGBA8 at the requested pixel width.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document or page, or when rendering fails.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_pdf() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/phase1-single-page.pdf")
    }

    fn multipage_fixture_pdf() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase1-multipage.pdf")
    }

    #[test]
    fn opens_fixture_pdf() -> Result<()> {
        let doc = PdfDoc::open(&fixture_pdf())?;

        assert_eq!(doc.page_count(), 1);
        assert!(doc.path().ends_with("phase1-single-page.pdf"));

        Ok(())
    }

    #[test]
    fn opens_multipage_fixture_pdf() -> Result<()> {
        let doc = PdfDoc::open(&multipage_fixture_pdf())?;

        assert_eq!(doc.page_count(), 84);
        assert!(doc.path().ends_with("phase1-multipage.pdf"));
        let outline = doc.outline()?;
        assert!(!outline.is_empty());
        assert!(outline_has_page_target(&outline));

        Ok(())
    }

    #[test]
    fn renders_page_zero_as_rgba() -> Result<()> {
        let doc = PdfDoc::open(&fixture_pdf())?;
        let rendered = doc.render_page(0, 320)?;

        assert_eq!(rendered.width, 320);
        assert!(rendered.height > 0);
        assert_eq!(
            rendered.rgba.len(),
            rendered.width as usize * rendered.height as usize * 4
        );

        Ok(())
    }

    #[test]
    fn reports_plausible_page_aspect_ratio() -> Result<()> {
        let doc = PdfDoc::open(&fixture_pdf())?;
        let ratio = doc.page_aspect_ratio(0)?;

        assert!(ratio > 0.5 && ratio < 3.0, "unexpected ratio: {ratio}");

        Ok(())
    }

    #[test]
    fn extracts_fixture_text() -> Result<()> {
        let doc = PdfDoc::open(&fixture_pdf())?;
        let text = doc.text_on_page(0)?;

        assert!(text.contains("PDF-Folio Phase 1 Fixture"));

        Ok(())
    }

    #[test]
    fn returns_empty_outline_for_fixture_without_bookmarks() -> Result<()> {
        let doc = PdfDoc::open(&fixture_pdf())?;

        assert!(doc.outline()?.is_empty());

        Ok(())
    }

    fn outline_has_page_target(nodes: &[OutlineNode]) -> bool {
        nodes
            .iter()
            .any(|node| node.page.is_some() || outline_has_page_target(&node.children))
    }
}
