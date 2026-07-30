//! PDF page geometry primitives for text hit-testing and selection.
//!
//! Coordinates are normalized to the unit square so the same bounds work at
//! any zoom level or display scale. The origin is top-left of the page (y
//! increases downward), matching typical UI layout rather than PDF's bottom-
//! left origin. Values are produced by [`super::document::PdfDoc::text_layer`].
//!
//! # See also
//!
//! - [`super::document::PageTextChar`] embeds a [`TextRect`] per glyph.
//! - [`super::document::PageTextLayer`] groups characters for one page.

/// A normalized top-left-origin rectangle in page coordinates.
///
/// All fields are fractions of page width/height in `0.0..=1.0` (width and
/// height may be clamped slightly outside when Pdfium bounds are noisy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRect {
    /// Left edge as a fraction of page width (`0.0` = left side of the page).
    pub x: f32,
    /// Top edge as a fraction of page height (`0.0` = top of the page).
    pub y: f32,
    /// Width as a fraction of page width.
    pub width: f32,
    /// Height as a fraction of page height.
    pub height: f32,
}
