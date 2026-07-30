//! Sync status, report, plan, and checkpoint types.
//!
//! These are pure data structures returned by [`super::crdt`] and [`super::run`]
//! methods so UI and CLI code can display counters without depending on
//! implementation details. No I/O lives here.
//!
//! # Registry stream
//!
//! [`REGISTRY_LIBRARY_ID`] is a synthetic library id for the app-level library
//! registry CRDT (existence, rename, tombstone of libraries). Per-library
//! content ops use the real library id as the stream key.
//!
//! # Related
//!
//! Produced by: [`super::crdt`], [`super::run`], [`super::cli`]

use chrono::{DateTime, Utc};

/// Synthetic CRDT stream id for app-level library registry operations.
///
/// Not a user-visible library; used only as the `library_id` partition for
/// registry ops so library **existence** can sync independently of contents.
pub const REGISTRY_LIBRARY_ID: &str = "__pdf_folio_registry__";

/// Sync-visible app library profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncLibraryRow {
    /// Stable library id from the PDF-Folio library registry.
    pub id: String,
    /// User-visible library name.
    pub name: String,
    /// Last local update timestamp as a Unix timestamp.
    pub updated_at: i64,
    /// Tombstone timestamp, when deleted.
    pub deleted_at: Option<i64>,
}

/// Durable sync checkpoint for one device/library pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCheckpoint {
    /// Local library identifier.
    pub library_id: String,
    /// Stable device identifier.
    pub device_id: String,
    /// Last fully completed sync time.
    pub last_synced_at: DateTime<Utc>,
}

/// Counts of local work a future push/pull pass should perform.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlan {
    /// Local entries newer than the checkpoint.
    pub entries_to_push: usize,
    /// Local folders newer than the checkpoint.
    pub folders_to_push: usize,
    /// Entry-folder memberships newer than the checkpoint.
    pub memberships_to_push: usize,
}

/// Summary for one CRDT-based metadata sync pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncCrdtReport {
    /// Local snapshot operations created before pushing.
    pub generated_operations: usize,
    /// Locally-originated operations uploaded to the remote log.
    pub pushed_operations: usize,
    /// Remote log operations pulled into the local operation store.
    pub pulled_operations: usize,
    /// Entry metadata rows materialized after CRDT replay.
    pub materialized_entries: usize,
    /// Folder metadata rows materialized after CRDT replay.
    pub materialized_folders: usize,
    /// Entry-folder membership rows materialized after CRDT replay.
    pub materialized_memberships: usize,
}

/// Cheap preflight for deciding whether a full CRDT sync pass is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncCrdtPreflight {
    /// Local snapshot operations created while checking for local changes.
    pub generated_operations: usize,
    /// Locally-originated operations waiting to be uploaded.
    pub pending_operations: usize,
    /// Latest remote operation sequence in Turso.
    pub remote_sequence: i64,
    /// Last remote operation sequence applied locally.
    pub local_cursor: i64,
}

impl SyncCrdtPreflight {
    /// Returns true when metadata should be synced.
    #[must_use]
    pub fn needs_metadata_sync(self) -> bool {
        self.generated_operations > 0
            || self.pending_operations > 0
            || self.remote_sequence > self.local_cursor
    }
}

/// Summary for uploading local PDF blobs during automatic sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncBlobUploadReport {
    /// PDF blobs uploaded into remote object storage.
    pub uploaded_blobs: usize,
    /// PDF blobs that were already present remotely.
    pub already_remote_blobs: usize,
    /// Local entries skipped because they are not content-addressed PDFs or their files are absent.
    pub skipped_blobs: usize,
    /// Local PDF blobs that could not be uploaded during this pass.
    pub failed_blobs: usize,
}

/// Summary for a complete automatic sync pass that may be skipped when clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncRunReport {
    /// Whether the expensive sync work was skipped after preflight.
    pub skipped: bool,
    /// Preflight state used to decide whether metadata had work.
    pub preflight: SyncCrdtPreflight,
    /// PDF blob upload results.
    pub uploads: SyncBlobUploadReport,
    /// CRDT metadata replay results.
    pub crdt: SyncCrdtReport,
    /// Remote hydration results.
    pub hydration: SyncHydrationReport,
}

/// Summary for hydrating local library rows from remote sync metadata and blobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncHydrationReport {
    /// Remote entries inserted into the local library.
    pub hydrated_entries: usize,
    /// Existing local entries relinked to a present managed blob.
    pub relinked_entries: usize,
    /// Remote folders inserted into the local library.
    pub hydrated_folders: usize,
    /// Remote folder memberships inserted into the local library.
    pub hydrated_memberships: usize,
    /// PDF blobs downloaded into the local sync cache.
    pub downloaded_blobs: usize,
    /// Entries whose blob was already cached locally.
    pub cached_blobs: usize,
    /// Remote entry blobs that were not available during this hydration pass.
    pub missing_blobs: usize,
    /// Remote rows skipped because they are deleted, invalid, already local, or unavailable.
    pub skipped_entries: usize,
}
