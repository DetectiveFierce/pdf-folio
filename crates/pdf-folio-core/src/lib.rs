//! Core PDF loading, rendering, cache, and annotation types for PDF-Folio.

pub mod annotations;
pub mod document;
pub mod renderer;

pub use annotations::{Annotation, AnnotationId, AnnotationKind, ColorRgba, PagePoint, PageRect};
pub use document::{OutlineNode, PdfDoc, RenderedPage};
pub use renderer::{TileCache, TileKey};
