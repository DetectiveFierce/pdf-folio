//! Core PDF loading, rendering, cache, and database types for PDF-Folio.
//!
//! This crate is the foundational layer shared by the cloud integration
//! (`pdf-folio-cloud`), desktop UI (`pdf-folio-ui`), and binary
//! (`pdf-folio-main`) crates. It owns no UI state: callers pass paths and
//! library IDs in, and receive pure data types and `Result`s back.
//!
//! # Modules
//!
//! - [`pdf`] wraps [`pdfium-render`] to open documents ([`PdfDoc`]), extract
//!   outlines ([`OutlineNode`]) and per-character text layers
//!   ([`PageTextLayer`]), render pages to RGBA ([`RenderedPage`]), and cache
//!   on-demand tiles via [`TileCache`] / [`TileKey`]. Geometry for text
//!   selection lives in [`pdf::geometry::TextRect`].
//! - [`db`] owns the SQLite-backed [`Db`] handle: library entries, folders,
//!   tags, preferences, import/watching, Tantivy search, Raindrop mapping
//!   tables, and local sync/CRDT metadata used by the cloud crate.
//!
//! Most library and search types are re-exported at the crate root so
//! callers can write `pdf_folio_core::{Db, EntryId, PdfDoc, ...}`.
//!
//! [`pdfium-render`]: https://docs.rs/pdfium-render

pub mod db;
pub mod pdf;

pub use pdf::{
    OutlineNode, PageTextChar, PageTextLayer, PdfDoc, RenderedPage, TextRect, TileCache, TileKey,
};

// Re-export the database public surface at the crate root so callers can reach
// it as `pdf_folio_core::{Db, EntryId, ...}` after the crate consolidation.
pub use db::import::{
    clean_import_title, hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_path,
    title_from_path, ImportSummary, ImportedEntry, LibraryWatchEvent, LibraryWatcher,
};
pub use db::search::{IndexDocument, SearchHit, SearchIndex};
pub use db::{
    Db, EntryFolderMembership, EntryId, EntryTrashState, Folder, FolderId, ImportSource,
    LibraryEntry, LibraryFolderSnapshot, LibraryLayoutMode, LibraryOrganizationSnapshot,
    LibraryPreferences, LibrarySortMode, NewLibraryEntry, RaindropEntryMapping,
    SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow, SyncEntryRow, SyncFolderRow,
    SyncSeedSummary,
};
