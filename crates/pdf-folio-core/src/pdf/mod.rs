//! PDF document wrapper, render output types, and tile cache.
//!
//! - [`document`] wraps [`pdfium-render`] to open PDFs, extract outlines,
//!   page text layers, and produce [`RenderedPage`] bitmaps.
//! - [`renderer`] provides the [`TileCache`] and [`TileKey`] abstraction
//!   for on-demand, LRU-cached page rendering at a target width.
//!
//! [`pdfium-render`]: https://docs.rs/pdfium-render

pub mod document;
pub mod renderer;

pub use document::{OutlineNode, PageTextChar, PageTextLayer, PdfDoc, RenderedPage, TextRect};
pub use renderer::{TileCache, TileKey};
