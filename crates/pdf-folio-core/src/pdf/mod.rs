//! PDF document loading, text extraction, page rendering, and tile cache.
//!
//! This module is the viewer/core boundary for PDF file access. Higher layers
//! open a [`PdfDoc`], request page bitmaps or text layers, and optionally
//! memoize rendered RGBA strips in a [`TileCache`]. Pdfium is bound once
//! process-wide; document methods serialize access through an internal mutex.
//!
//! # Submodules
//!
//! - [`document`] — open PDFs, outlines ([`OutlineNode`]), page text
//!   ([`PageTextLayer`]), metadata, and [`RenderedPage`] bitmaps via
//!   [`pdfium-render`].
//! - [`geometry`] — normalized page rectangles ([`TextRect`]) used by the
//!   text layer for hit-testing and selection highlighting.
//! - [`renderer`] — [`TileKey`] / [`TileCache`] LRU cache for on-demand
//!   page rendering at a target pixel width (used by the UI viewer).
//!
//! Types are re-exported at this module root and again at the crate root.
//!
//! [`pdfium-render`]: https://docs.rs/pdfium-render

pub mod document;
pub mod geometry;
pub mod renderer;

pub use document::{OutlineNode, PageTextChar, PageTextLayer, PdfDoc, RenderedPage};
pub use geometry::TextRect;
pub use renderer::{TileCache, TileKey};

#[cfg(test)]
mod tests;
