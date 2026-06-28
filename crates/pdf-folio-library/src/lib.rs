//! Library storage, search indexing, and filesystem watching for PDF-Folio.

pub mod db;
pub mod importer;
pub mod indexer;
pub mod watcher;

pub use db::{Db, EntryId, LibraryEntry, NewLibraryEntry};
pub use importer::{
    hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_cache_dir, thumbnail_path,
    ImportSummary, ImportedEntry,
};
pub use indexer::{IndexDocument, SearchHit, SearchIndex};
pub use watcher::{LibraryWatchEvent, LibraryWatcher};
