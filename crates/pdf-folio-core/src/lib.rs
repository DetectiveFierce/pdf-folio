//! Core PDF loading, rendering, cache, and database types for PDF-Folio.
//!
//! This crate is the foundational layer shared by the cloud integration,
//! viewer, and UI crates:
//!
//! - [`pdf`] wraps [`pdfium-render`] to open PDFs, extract outlines, page
//!   text layers, and produce [`pdf::RenderedPage`] bitmaps, plus the
//!   [`pdf::TileCache`] / [`pdf::TileKey`] on-demand rendering cache.
//! - [`db`] provides the [`Db`] handle backed by SQLite, including library
//!   entries, folders, tags, search indexing, import/watching, and the
//!   raindrop-collection/entry mapping tables.
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
    hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_cache_dir, thumbnail_path,
    ImportSummary, ImportedEntry, LibraryWatchEvent, LibraryWatcher,
};
pub use db::search::{IndexDocument, SearchHit, SearchIndex};
pub use db::{
    Db, EntryFolderMembership, EntryId, EntryTrashState, Folder, FolderId, ImportSource,
    LibraryEntry, LibraryFolderSnapshot, LibraryLayoutMode, LibraryOrganizationSnapshot,
    LibraryPreferences, LibrarySortMode, NewLibraryEntry, RaindropCollectionMapping,
    RaindropEntryMapping, SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow,
    SyncEntryRow, SyncFolderRow, SyncSeedSummary,
};
