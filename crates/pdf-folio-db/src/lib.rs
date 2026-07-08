//! Transitional shim crate.
//!
//! The standalone `pdf-folio-db` crate has been merged into `pdf-folio-core`
//! as the `pdf_folio_core::db` module. This crate exists only to keep existing
//! `use pdf_folio_db::*` imports compiling during the crate consolidation; it
//! glob-re-exports the merged surface from `pdf-folio-core`. It will be
//! removed once all consumers have been rewired (see Phase 2/6 of the
//! consolidation).

pub use pdf_folio_core::{
    hash_file, import_folder, import_pdf, scan_pdf_files, thumbnail_cache_dir, thumbnail_path,
    ImportSummary, ImportedEntry, LibraryWatchEvent, LibraryWatcher,
};
pub use pdf_folio_core::db::{
    Db, EntryFolderMembership, EntryId, EntryTrashState, Folder, FolderId, ImportSource,
    LibraryEntry, LibraryFolderSnapshot, LibraryLayoutMode, LibraryOrganizationSnapshot,
    LibraryPreferences, LibrarySortMode, NewLibraryEntry, RaindropCollectionMapping,
    RaindropEntryMapping, SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow,
    SyncEntryRow, SyncFolderRow, SyncSeedSummary,
};
pub use pdf_folio_core::{IndexDocument, SearchHit, SearchIndex};
