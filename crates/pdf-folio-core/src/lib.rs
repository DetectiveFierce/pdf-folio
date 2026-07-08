//! Core PDF loading, rendering, cache, and annotation types for PDF-Folio.
//!
//! This crate is the foundational layer shared by the cloud integration,
//! viewer, and UI crates:
//!
//! - [`pdf`] wraps [`pdfium-render`] to open PDFs, extract outlines, page
//!   text layers, and produce [`pdf::RenderedPage`] bitmaps, plus the
//!   [`pdf::TileCache`] / [`pdf::TileKey`] on-demand rendering cache.
//! - [`annotations`] defines the data model for user annotations (stamps,
//!   highlights, free text) keyed by [`AnnotationId`].
//! - [`db`] provides the [`Db`] handle backed by SQLite, including library
//!   entries, folders, tags, search indexing, import/watching, and the
//!   raindrop-collection/entry mapping tables.
//!
//! [`pdfium-render`]: https://docs.rs/pdfium-render

pub mod annotations;
pub mod db;
pub mod pdf;

pub use annotations::{Annotation, AnnotationId, AnnotationKind, ColorRgba, PagePoint, PageRect};
pub use pdf::{OutlineNode, PageTextChar, PageTextLayer, PdfDoc, RenderedPage, TileCache, TileKey};

// Re-export the database public surface at the crate root so that callers can
// reach it as `pdf_folio_core::{Db, EntryId, ...}` mirroring the old standalone
// `pdf-folio-db` crate, and so the (transitional) `pdf-folio-db` shim can glob
// these through.
pub use db::{
    Db, EntryFolderMembership, EntryId, EntryTrashState, Folder, FolderId, ImportSource,
    LibraryEntry, LibraryFolderSnapshot, LibraryLayoutMode, LibraryOrganizationSnapshot,
    LibraryPreferences, LibrarySortMode, NewLibraryEntry, RaindropCollectionMapping,
    RaindropEntryMapping, SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow,
    SyncEntryRow, SyncFolderRow, SyncSeedSummary,
};
pub use db::import::{
    hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_cache_dir, thumbnail_path,
    ImportSummary, ImportedEntry, LibraryWatchEvent, LibraryWatcher,
};
pub use db::search::{IndexDocument, SearchHit, SearchIndex};
