//! Library storage, search indexing, import, and filesystem watching for
//! PDF-Folio.
//!
//! This module manages all persistent data for the user's PDF library:
//!
//! - [`Db`] is the SQLite handle for entries, folders, tags, library
//!   preferences, and the raindrop/sync mapping tables.
//! - [`import`] scans directories for PDF files, hashes them with BLAKE3,
//!   imports them into the database, manages thumbnail cache paths, and
//!   watches configured folders via [`import::LibraryWatcher`].
//! - [`search`] wraps [`tantivy`] to build a full-text search index over
//!   extracted PDF text content ([`search::SearchIndex`],
//!   [`search::SearchHit`]).
//!
//! [`tantivy`]: https://docs.rs/tantivy

use std::path::PathBuf;


mod naming;


mod types;
pub use types::*;

pub mod import;
pub mod library;
pub mod metadata;
pub mod organization;
pub mod raindrop;
pub mod schema;
pub mod search;
pub mod sync;

/// SQLite-backed PDF-Folio library database.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
}

impl Db {



}




#[cfg(test)]
mod tests;
