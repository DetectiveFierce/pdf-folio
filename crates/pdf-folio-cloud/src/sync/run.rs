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
use pdf_folio_core::{Db, SyncCrdtPrepareSummary};

use super::blobs::BlobCache;
use super::client::SyncClient;
use super::crdt::{prepare_local_crdt_operations, PreparedCrdtOperations};
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
        let db_path = db.path().to_path_buf();
        let library_id_owned = library_id.to_owned();
        let device_id_owned = device_id.to_owned();
        let (prepared, used_local_snapshot) = tokio::task::spawn_blocking(move || {
            let db = Db::open(db_path)?;
            prepare_incremental_local_snapshot(&db, &library_id_owned, &device_id_owned)
        })
        .await??;
        let remote = self.turso.remote().await?;
        let local_cursor = db.sync_crdt_remote_cursor(library_id, device_id)?;
        let remote_sequence = super::crdt::remote_sync_head_sequence(&remote, library_id).await?;
        Ok(SyncCrdtPreflight {
            used_local_snapshot,
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

/// Reuses the persisted local snapshot when source tables have not changed.
///
/// The revision is checked again after a scan. If an edit lands concurrently,
/// the older snapshot marker is retained so the next pass safely rescans.
fn prepare_incremental_local_snapshot(
    db: &Db,
    library_id: &str,
    device_id: &str,
) -> Result<(PreparedCrdtOperations, bool)> {
    let revision_before = db.local_change_revision()?;
    if db.sync_local_snapshot_revision(library_id)? == Some(revision_before) {
        let pending_operations = db.pending_sync_crdt_operations(library_id, device_id)?;
        return Ok((
            PreparedCrdtOperations {
                summary: SyncCrdtPrepareSummary {
                    generated: 0,
                    pending_push: pending_operations.len(),
                },
                pending_operations,
            },
            true,
        ));
    }

    db.seed_sync_metadata(library_id)?;
    let prepared = prepare_local_crdt_operations(db, library_id, device_id)?;
    let revision_after = db.local_change_revision()?;
    if revision_after == revision_before {
        db.remember_sync_local_snapshot(library_id, revision_after)?;
    }
    Ok((prepared, false))
}

#[cfg(test)]
mod tests {
    use super::prepare_incremental_local_snapshot;
    use chrono::Utc;
    use pdf_folio_core::{Db, EntryId, NewLibraryEntry};
    use std::path::PathBuf;

    #[test]
    fn unchanged_library_reuses_persisted_local_snapshot() {
        let path = std::env::temp_dir().join(format!(
            "pdf-folio-sync-snapshot-{}-{}.db",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let db = Db::open(path).unwrap();
        db.insert_entry(&NewLibraryEntry {
            id: EntryId::new("entry"),
            path: PathBuf::from("/missing/entry.pdf"),
            title: Some(String::from("Entry")),
            author: None,
            author_attributed: false,
            page_count_attributed: false,
            page_count: None,
            file_size: None,
            cover_hash: None,
        })
        .unwrap();

        let (first, first_used_snapshot) =
            prepare_incremental_local_snapshot(&db, "library", "device").unwrap();
        assert!(!first_used_snapshot);
        assert!(first.summary.generated > 0);

        let (second, second_used_snapshot) =
            prepare_incremental_local_snapshot(&db, "library", "device").unwrap();
        assert!(second_used_snapshot);
        assert_eq!(second.summary.generated, 0);

        db.add_tag(&EntryId::new("entry"), "changed").unwrap();
        let (third, third_used_snapshot) =
            prepare_incremental_local_snapshot(&db, "library", "device").unwrap();
        assert!(!third_used_snapshot);
        assert!(third.summary.generated > 0);
    }
}
