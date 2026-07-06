//! Library storage, search indexing, and filesystem watching for PDF-Folio.
//!
//! This crate manages all persistent data for the user's PDF library:
//!
//! - [`db`] provides the [`Db`] handle backed by SQLite, including entries,
//!   folders, tags, and library preferences ([`LibraryPreferences`]).
//! - [`importer`] scans directories for PDF files, hashes them with BLAKE3,
//!   imports them into the database, and manages thumbnail generation.
//! - [`indexer`] wraps [`tantivy`] to build a full-text search index over
//!   extracted PDF text content ([`SearchIndex`], [`SearchHit`]).
//! - [`watcher`] uses [`notify`] to react to filesystem changes in watched
//!   directories and emits [`LibraryWatchEvent`]s.
//!
//! [`tantivy`]: https://docs.rs/tantivy
//! [`notify`]: https://docs.rs/notify

pub mod db;
pub mod importer;
pub mod indexer;
pub mod watcher;

pub use db::{
    Db, EntryFolderMembership, EntryId, EntryTrashState, Folder, FolderId, ImportSource,
    LibraryEntry, LibraryFolderSnapshot, LibraryLayoutMode, LibraryOrganizationSnapshot,
    LibraryPreferences, LibrarySortMode, NewLibraryEntry, RaindropCollectionMapping,
    RaindropEntryMapping, SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow,
    SyncEntryRow, SyncFolderRow, SyncSeedSummary,
};
pub use importer::{
    hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_cache_dir, thumbnail_path,
    ImportSummary, ImportedEntry,
};
pub use indexer::{IndexDocument, SearchHit, SearchIndex};
pub use watcher::{LibraryWatchEvent, LibraryWatcher};
