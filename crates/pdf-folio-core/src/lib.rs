//! Core PDF loading, rendering, cache, and annotation types for PDF-Folio.
//!
//! This crate is the foundational layer shared by the database, viewer, and
//! UI crates:
//!
//! - [`document`] wraps [`pdfium-render`] to open PDFs, extract outlines,
//!   page text layers, and produce [`RenderedPage`] bitmaps.
//! - [`renderer`] provides the [`TileCache`] and [`TileKey`] abstraction
//!   for on-demand, LRU-cached page rendering at a target width.
//! - [`annotations`] defines the data model for user annotations (stamps,
//!   highlights, free text) keyed by [`AnnotationId`].
//!
//! [`pdfium-render`]: https://docs.rs/pdfium-render


pub mod annotations;
pub mod document;
pub mod renderer;

pub use annotations::{Annotation, AnnotationId, AnnotationKind, ColorRgba, PagePoint, PageRect};
pub use document::{OutlineNode, PageTextChar, PageTextLayer, PdfDoc, RenderedPage, TextRect};
pub use renderer::{TileCache, TileKey};
