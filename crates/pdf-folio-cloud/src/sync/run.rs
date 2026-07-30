//! Full sync-pass orchestration for automatic / idle sync.
//!
//! Extends [`SyncClient`] with the preflight gate used by the UI so background
//! sync can avoid expensive work when the library is already consistent with
//! Turso. The full path is: preflight → optional blob upload → CRDT push/pull →
//! hydration.
//!
//! # Related
//!
//! - Preflight generation uses [`super::crdt::prepare_local_crdt_operations`]
//! - Full forced pass: [`SyncClient::sync_crdt_metadata`](super::crdt)
//! - Report types: [`super::status`]

use anyhow::Result;
use pdf_folio_core::Db;

use super::blobs::BlobCache;
use super::client::SyncClient;
use super::crdt::prepare_local_crdt_operations;
use super::status::{SyncCrdtPreflight, SyncHydrationReport, SyncRunReport};

impl SyncClient {
    /// Returns a cheap CRDT sync preflight and records local changed snapshots.
    ///
    /// This is the startup/idle gate for automatic sync: local state is seeded
    /// and snapshotted so unsynced local edits become durable operations, then
    /// the remote head is compared with the local cursor. Callers can skip the
    /// full upload/replay/hydration pass when this returns no metadata work and
    /// there are no blobs waiting for upload.
    ///
    /// # Errors
    ///
    /// Returns an error when local CRDT state or the remote head cannot be read.
    pub async fn preflight_crdt_sync(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncCrdtPreflight> {
        db.seed_sync_metadata(library_id)?;
        let prepared = prepare_local_crdt_operations(db, library_id, device_id)?;
        let remote = self.turso.remote().await?;
        let local_cursor = db.sync_crdt_remote_cursor(library_id, device_id)?;
        let remote_sequence = super::crdt::remote_sync_head_sequence(&remote, library_id).await?;
        Ok(SyncCrdtPreflight {
            generated_operations: prepared.summary.generated,
            pending_operations: prepared.pending_operations.len(),
            remote_sequence,
            local_cursor,
        })
    }

    /// Runs an automatic sync pass only when preflight finds work.
    ///
    /// # Errors
    ///
    /// Returns an error when sync metadata, blobs, or remote state cannot be
    /// read or written.
    pub async fn sync_library_if_needed(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
        cache: &BlobCache,
    ) -> Result<SyncRunReport> {
        let preflight = self.preflight_crdt_sync(db, library_id, device_id).await?;
        let has_blobs_to_upload = db.has_entries_needing_sync_blob_upload()?;
        if !preflight.needs_metadata_sync() && !has_blobs_to_upload {
            return Ok(SyncRunReport {
                skipped: true,
                preflight,
                ..SyncRunReport::default()
            });
        }

        let uploads = self.upload_local_blobs(db, cache).await?;
        let crdt = self
            .sync_crdt_metadata_after_preflight(
                db,
                library_id,
                device_id,
                preflight.generated_operations,
            )
            .await?;
        let hydration = if crdt.pulled_operations > 0 || crdt.materialized_entries > 0 {
            self.hydrate_remote_library(db, library_id, cache).await?
        } else {
            SyncHydrationReport::default()
        };
        Ok(SyncRunReport {
            skipped: false,
            preflight,
            uploads,
            crdt,
            hydration,
        })
    }
}
