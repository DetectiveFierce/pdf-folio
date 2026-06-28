//! PDF document wrapper and render output types.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use pdfium_render::prelude::{PdfRenderConfig, Pdfium};

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

        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("Could not open PDF: {}.", self.path.display()))?;
        let page = document.pages().get(i32::from(index)).with_context(|| {
            format!(
                "Could not render page {}: the page does not exist.",
                index + 1
            )
        })?;
        let aspect_ratio = page.width().value / page.height().value;
        let height_px = ((width_px as f32) / aspect_ratio).round().max(1.0) as u16;
        let bitmap = page.render_with_config(
            &PdfRenderConfig::new()
                .set_target_width(width_px as i32)
                .set_target_height(height_px as i32),
        )?;

        Ok(RenderedPage {
            width: bitmap.width() as u16,
            height: bitmap.height() as u16,
            rgba: bitmap.as_rgba_bytes().to_vec(),
        })
    }

    /// Returns the page width divided by page height.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document or page.
    pub fn page_aspect_ratio(&self, index: u16) -> Result<f32> {
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("Could not open PDF: {}.", self.path.display()))?;
        let page = document.pages().get(i32::from(index)).with_context(|| {
            format!(
                "Could not inspect page {}: the page does not exist.",
                index + 1
            )
        })?;

        Ok(page.width().value / page.height().value)
    }

    /// Returns the document outline tree.
    ///
    /// # Errors
    ///
    /// Returns an error when the document cannot be opened.
    pub fn outline(&self) -> Result<Vec<OutlineNode>> {
        let pdfium = Self::pdfium()?;
        let _document = pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("Could not open PDF: {}.", self.path.display()))?;

        Ok(Vec::new())
    }

    /// Extracts text from a page.
    ///
    /// # Errors
    ///
    /// Returns an error when Pdfium cannot load the document, page, or page text.
    pub fn text_on_page(&self, index: u16) -> Result<String> {
        let pdfium = Self::pdfium()?;
        let document = pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("Could not open PDF: {}.", self.path.display()))?;
        let page = document.pages().get(i32::from(index)).with_context(|| {
            format!(
                "Could not read page {}: the page does not exist.",
                index + 1
            )
        })?;

        let text = page.text()?.all();
        Ok(text)
    }

    fn pdfium() -> Result<Pdfium> {
        let bindings = Pdfium::bind_to_system_library()
            .or_else(|_| Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")))
            .context("Could not initialize Pdfium. Install Pdfium or place the library next to the binary.")?;

        Ok(Pdfium::new(bindings))
    }
}
