//! Library storage, search indexing, import, and filesystem watching for
//! PDF-Folio.
//!
//! All durable user library state lives here. The central type is [`Db`], a
//! thin path-based handle that opens a short-lived rusqlite connection per
//! call (no long-lived connection pool). Methods are split across submodules
//! by domain so related SQL stays together while the public surface remains
//! `Db::…` via inherent impls.
//!
//! # Submodules
//!
//! - [`schema`] — open/migrate the SQLite file ([`Db::open`], [`Db::open_default`]).
//! - [`library`] — entry CRUD, tags, trash, missing-file flags, path relink.
//! - [`organization`] — folder tree, memberships, manual order, undo snapshots.
//! - [`metadata`] — display overrides, preferences, reading progress, ratings.
//! - [`import`] — BLAKE3 hashing, folder scan/import, thumbnails, FS watcher.
//! - [`search`] — Tantivy full-text index over page text ([`search::SearchIndex`]).
//! - [`raindrop`] — Raindrop.io collection/entry mapping tables.
//! - [`sync`] — local sync metadata, CRDT op log, blob upload markers.
//!
//! Shared row/DTO types (`EntryId`, `LibraryEntry`, sync rows, …) are defined
//! alongside this module and re-exported from both `db` and the crate root.
//! Private helpers for sort keys and gap-spaced manual order live in `naming`.
//!
//! # See also
//!
//! - [`crate::pdf::PdfDoc`] for extracting metadata/text that feed import and search.
//! - Cloud crate sync runners consume [`sync`] and the sync-related DTOs.
//!
//! [`tantivy`]: https://docs.rs/tantivy

use std::path::PathBuf;

/// Sort keys, cleaned text, and gap-spaced manual order helpers.
mod naming;

/// Shared row/DTO types re-exported at this module and the crate root.
mod types;
pub use types::*;

/// Import scanning, hashing, thumbnails, and filesystem watching.
pub mod import;
/// Entry CRUD, tags, trash, missing-file flags, and path relink.
pub mod library;
/// Display overrides, preferences, reading progress, and ratings.
pub mod metadata;
/// Folder tree, memberships, manual order, and undo snapshots.
pub mod organization;
/// Raindrop.io collection/entry mapping tables.
pub mod raindrop;
/// Open/migrate the SQLite file and run schema upgrades.
pub mod schema;
/// Tantivy full-text index over page text.
pub mod search;
/// Local sync metadata, CRDT op log, and blob upload markers.
pub mod sync;

/// SQLite-backed PDF-Folio library database handle.
///
/// Stores only the on-disk database path. Each public method opens a fresh
/// connection (with foreign keys enabled), runs its work, and closes. Open
/// via [`Db::open`] or [`Db::open_default`]; migrations run automatically on
/// open. Domain methods are implemented in the `library`, `organization`,
/// `metadata`, `import`, `raindrop`, and `sync` submodules.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
}

impl Db {}

#[cfg(test)]
mod tests;
