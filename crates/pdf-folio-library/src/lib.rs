//! Library storage, search indexing, and filesystem watching for PDF-Folio.

pub mod db;
pub mod indexer;
pub mod watcher;

pub use db::{Db, EntryId, LibraryEntry, NewLibraryEntry};
pub use indexer::{IndexDocument, SearchIndex};
pub use watcher::LibraryWatcher;
