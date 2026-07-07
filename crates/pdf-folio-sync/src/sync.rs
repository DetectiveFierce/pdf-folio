use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use pdf_folio_db::{
    hash_file, Db, LibraryEntry, SyncCrdtOperation, SyncCrdtPrepareSummary, SyncEntryFolderRow,
    SyncEntryRow, SyncFolderRow,
};
use serde::{Deserialize, Serialize};

use crate::blob_cache::BlobCache;
use crate::r2_client::R2Client;
use crate::session::Session;
use crate::turso_client::{TursoClient, TursoRemote, TursoValue};

const ENTITY_ENTRY: &str = "entry";
const ENTITY_FOLDER: &str = "folder";
const ENTITY_ENTRY_FOLDER: &str = "entry_folder";
const ENTITY_LIBRARY: &str = "library";

/// Synthetic CRDT stream used for app-level library registry operations.
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

/// High-level sync coordinator.
#[derive(Debug, Clone)]
pub struct SyncClient {
    /// Session used for all control-plane calls.
    pub session: Session,
    /// Turso credential client.
    pub turso: TursoClient,
    /// R2 blob client.
    pub r2: R2Client,
}

impl SyncClient {
    /// Creates a sync coordinator from a cached session.
    pub fn new(session: Session) -> Self {
        Self {
            turso: TursoClient::new(session.clone()),
            r2: R2Client::new(session.clone()),
            session,
        }
    }

    /// Returns a lightweight push plan from local sync tracking tables.
    ///
    /// # Errors
    ///
    /// Returns an error when local sync metadata cannot be queried.
    pub fn plan_push(&self, db: &Db, library_id: &str, device_id: &str) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?;
        let since = checkpoint.unwrap_or(0);
        Ok(SyncPlan {
            entries_to_push: db.sync_entries_updated_since(library_id, since)?.len(),
            folders_to_push: db.sync_folders_updated_since(library_id, since)?.len(),
            memberships_to_push: db
                .sync_entry_folders_updated_since(library_id, since)?
                .len(),
        })
    }

    /// Pushes app-level library records to Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be written.
    pub async fn push_libraries(&self, libraries: &[SyncLibraryRow]) -> Result<usize> {
        if libraries.is_empty() {
            return Ok(0);
        }
        let remote = self.turso.remote().await?;
        upsert_remote_libraries(&remote, libraries).await?;
        Ok(libraries.len())
    }

    /// Pulls all non-deleted app-level library records from Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be read.
    pub async fn pull_libraries(&self) -> Result<Vec<SyncLibraryRow>> {
        let remote = self.turso.remote().await?;
        remote_libraries(&remote).await
    }

    /// Returns the latest remote CRDT operation sequence for one library.
    ///
    /// This is intentionally much cheaper than a full sync pass and is used by
    /// the UI's live watcher to decide when another device has appended work.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote log cannot be queried.
    pub async fn remote_crdt_head_sequence(&self, library_id: &str) -> Result<i64> {
        let remote = self.turso.remote().await?;
        remote_sync_head_sequence(&remote, library_id).await
    }

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
        let remote_sequence = remote_sync_head_sequence(&remote, library_id).await?;
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

    /// Runs one CRDT pass for the app-level library registry.
    ///
    /// The registry CRDT only decides library existence, deletion, and the
    /// current label carried by library records. Library contents continue to
    /// sync on each library's own stream.
    ///
    /// # Errors
    ///
    /// Returns an error when local CRDT state or remote operations cannot be
    /// read or written.
    pub async fn sync_library_registry(
        &self,
        db: &Db,
        rows: &[SyncLibraryRow],
        device_id: &str,
    ) -> Result<Vec<SyncLibraryRow>> {
        let mut logical_time = db
            .sync_crdt_max_logical_time(REGISTRY_LIBRARY_ID)?
            .map_or_else(
                || Utc::now().timestamp_millis(),
                |time| Utc::now().timestamp_millis().max(time + 1),
            );
        for row in rows {
            let payload = LibraryPayload {
                id: row.id.clone(),
                name: row.name.clone(),
                updated_at: row.updated_at,
                deleted_at: row.deleted_at,
            };
            if record_payload_operation(
                db,
                REGISTRY_LIBRARY_ID,
                device_id,
                ENTITY_LIBRARY,
                row.id.as_str(),
                &payload,
                logical_time,
            )? {
                logical_time += 1;
            }
        }

        let remote = self.turso.remote().await?;
        let cursor = db.sync_crdt_remote_cursor(REGISTRY_LIBRARY_ID, device_id)?;
        let pulled_before_push =
            remote_sync_operations_since(&remote, REGISTRY_LIBRARY_ID, cursor).await?;
        apply_pulled_crdt_operations(db, REGISTRY_LIBRARY_ID, device_id, &pulled_before_push)?;

        let pending = db.pending_sync_crdt_operations(REGISTRY_LIBRARY_ID, device_id)?;
        upsert_remote_sync_operations(&remote, &pending).await?;
        db.mark_sync_crdt_operations_pushed(
            pending.iter().map(|operation| operation.op_id.as_str()),
        )?;

        let cursor = db.sync_crdt_remote_cursor(REGISTRY_LIBRARY_ID, device_id)?;
        let pulled_after_push =
            remote_sync_operations_since(&remote, REGISTRY_LIBRARY_ID, cursor).await?;
        apply_pulled_crdt_operations(db, REGISTRY_LIBRARY_ID, device_id, &pulled_after_push)?;

        let winners =
            winners_for_operations(db.sync_crdt_operations_for_library(REGISTRY_LIBRARY_ID)?);
        let mut output = Vec::new();
        for ((entity_kind, _entity_id), operation) in winners {
            if entity_kind != ENTITY_LIBRARY {
                continue;
            }
            let payload = serde_json::from_str::<LibraryPayload>(&operation.payload)
                .context("Could not decode library registry CRDT payload.")?;
            output.push(SyncLibraryRow {
                id: payload.id,
                name: payload.name,
                updated_at: payload.updated_at,
                deleted_at: payload.deleted_at,
            });
        }
        output.sort_by(|left, right| {
            left.deleted_at
                .is_some()
                .cmp(&right.deleted_at.is_some())
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(output)
    }

    /// Creates the remote Turso schema if it does not already exist.
    ///
    /// # Errors
    ///
    /// Returns an error when the remote database cannot execute the schema batch.
    pub async fn ensure_remote_schema(&self) -> Result<()> {
        let remote = self.turso.remote().await?;
        remote
            .execute_batch(include_str!("../turso_schema.sql"))
            .await
            .context("Could not create PDF-Folio sync schema in Turso.")?;
        Ok(())
    }

    /// Ingests local content-addressed PDF files into the managed blob cache and uploads them.
    ///
    /// PDF bytes are always uploaded from PDF-Folio's content-addressed cache,
    /// not from an arbitrary user-selected path. When a source PDF is first
    /// seen, this pass copies it into the cache and relinks the local library
    /// entry to that managed path. Individual failures are counted instead of
    /// aborting the whole pass.
    ///
    /// # Errors
    ///
    /// Returns an error when local entries cannot be read.
    pub async fn upload_local_blobs(
        &self,
        db: &Db,
        cache: &BlobCache,
    ) -> Result<SyncBlobUploadReport> {
        let mut report = SyncBlobUploadReport::default();
        for entry in db.entries_needing_sync_blob_upload()? {
            if !is_blob_hash(entry.id.as_str()) {
                report.skipped_blobs += 1;
                continue;
            }
            let Some(upload_path) = managed_blob_path_for_entry(cache, &entry).await? else {
                report.skipped_blobs += 1;
                continue;
            };
            if entry.path != upload_path {
                db.relink_entry_path(&entry.id, &upload_path)?;
            }
            match self
                .r2
                .upload_pdf_if_missing(entry.id.as_str(), &upload_path)
                .await
            {
                Ok(response) => {
                    if response.upload_url.is_some() {
                        report.uploaded_blobs += 1;
                    } else {
                        report.already_remote_blobs += 1;
                    }
                    db.remember_sync_blob_uploaded(entry.id.as_str())?;
                }
                Err(_) => {
                    report.failed_blobs += 1;
                }
            }
        }
        Ok(report)
    }

    /// Runs one CRDT-based metadata sync pass for a library.
    ///
    /// The pass is idempotent: local state is snapshotted into immutable
    /// operations only when entity payloads change, pending local operations are
    /// inserted into Turso with `ON CONFLICT DO NOTHING`, remote operations are
    /// pulled by append sequence, and the local sync metadata tables are
    /// materialized from deterministic latest-writer-wins registers.
    ///
    /// # Errors
    ///
    /// Returns an error when local metadata cannot be read/written or remote
    /// operations cannot be pushed/pulled.
    pub async fn sync_crdt_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncCrdtReport> {
        db.seed_sync_metadata(library_id)?;
        let prepared = prepare_local_crdt_operations(db, library_id, device_id)?;
        self.sync_prepared_crdt_metadata(db, library_id, device_id, prepared)
            .await
    }

    async fn sync_crdt_metadata_after_preflight(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
        generated_operations: usize,
    ) -> Result<SyncCrdtReport> {
        let pending_operations = db.pending_sync_crdt_operations(library_id, device_id)?;
        let prepared = PreparedCrdtOperations {
            summary: SyncCrdtPrepareSummary {
                generated: generated_operations,
                pending_push: pending_operations.len(),
            },
            pending_operations,
        };
        self.sync_prepared_crdt_metadata(db, library_id, device_id, prepared)
            .await
    }

    async fn sync_prepared_crdt_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
        prepared: PreparedCrdtOperations,
    ) -> Result<SyncCrdtReport> {
        let remote = self.turso.remote().await?;

        let cursor = db.sync_crdt_remote_cursor(library_id, device_id)?;
        let pulled_before_push = remote_sync_operations_since(&remote, library_id, cursor).await?;
        apply_pulled_crdt_operations(db, library_id, device_id, &pulled_before_push)?;
        let mut affected_entities = affected_entities_for_operations(&prepared.pending_operations);
        affected_entities.extend(affected_entities_for_operations(&pulled_before_push));
        materialize_crdt_entities(db, library_id, affected_entities.iter().cloned())?;

        upsert_remote_sync_operations(&remote, &prepared.pending_operations).await?;
        db.mark_sync_crdt_operations_pushed(
            prepared
                .pending_operations
                .iter()
                .map(|operation| operation.op_id.as_str()),
        )?;

        let cursor = db.sync_crdt_remote_cursor(library_id, device_id)?;
        let pulled_after_push = remote_sync_operations_since(&remote, library_id, cursor).await?;
        apply_pulled_crdt_operations(db, library_id, device_id, &pulled_after_push)?;
        affected_entities.extend(affected_entities_for_operations(&pulled_after_push));

        let materialized = materialize_crdt_entities(db, library_id, affected_entities)?;
        Ok(SyncCrdtReport {
            generated_operations: prepared.summary.generated,
            pushed_operations: prepared.pending_operations.len(),
            pulled_operations: pulled_before_push.len() + pulled_after_push.len(),
            materialized_entries: materialized.entries_to_push,
            materialized_folders: materialized.folders_to_push,
            materialized_memberships: materialized.memberships_to_push,
        })
    }

    /// Downloads missing remote blobs and creates local library rows for pulled entries.
    ///
    /// Hydration is intentionally separate from CRDT metadata replay: the CRDT
    /// pass updates sync-visible state, then this step turns non-deleted remote
    /// entry rows into normal local library entries when the content-addressed
    /// PDF blob is available from R2.
    ///
    /// # Errors
    ///
    /// Returns an error when local metadata cannot be read/written or a required
    /// remote blob download fails.
    pub async fn hydrate_remote_library(
        &self,
        db: &Db,
        library_id: &str,
        cache: &BlobCache,
    ) -> Result<SyncHydrationReport> {
        let rows = db.sync_entries_needing_hydration(library_id)?;
        let mut report = SyncHydrationReport::default();
        for row in rows {
            if row.deleted_at.is_some() || !is_blob_hash(row.id.as_str()) {
                report.skipped_entries += 1;
                continue;
            }
            let path = cache.path_for_hash(row.id.as_str());
            let mut blob_available = cache.contains(row.id.as_str());
            if cache.contains(row.id.as_str()) {
                report.cached_blobs += 1;
            } else {
                match self.r2.download_pdf(row.id.as_str(), &path).await {
                    Ok(()) => {
                        blob_available = true;
                        report.downloaded_blobs += 1;
                    }
                    Err(_) => {
                        report.missing_blobs += 1;
                    }
                }
            }

            if let Some(entry) = db.entry_by_id(&row.id)? {
                if blob_available && (entry.missing || !entry.path.is_file() || entry.path != path)
                {
                    db.relink_entry_path(&row.id, &path)?;
                    report.relinked_entries += 1;
                }
                if let Some(payload) = winning_entry_payload(db, library_id, row.id.as_str())? {
                    apply_entry_payload_to_local(db, &payload)?;
                }
                report.skipped_entries += 1;
                continue;
            }

            if db.hydrate_sync_entry(
                &row,
                &path,
                blob_available
                    .then(|| std::fs::metadata(&path).ok().map(|metadata| metadata.len()))
                    .flatten(),
                !blob_available,
            )? {
                report.hydrated_entries += 1;
            }
            if let Some(payload) = winning_entry_payload(db, library_id, row.id.as_str())? {
                apply_entry_payload_to_local(db, &payload)?;
            }
        }

        let folders = db.sync_folders_updated_since(library_id, 0)?;
        for row in &folders {
            if db.hydrate_sync_folder(row)? {
                report.hydrated_folders += 1;
            }
        }
        for row in &folders {
            if row.parent_id.is_some() && db.hydrate_sync_folder(row)? {
                report.hydrated_folders += 1;
            }
        }

        for row in db.sync_entry_folders_updated_since(library_id, 0)? {
            if db.hydrate_sync_entry_folder(&row)? {
                report.hydrated_memberships += 1;
            }
        }
        Ok(report)
    }

    /// Pushes local metadata rows newer than the current checkpoint to Turso.
    ///
    /// # Errors
    ///
    /// Returns an error when local rows cannot be read, remote rows cannot be written,
    /// or the local checkpoint cannot be saved.
    pub async fn push_local_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?.unwrap_or(0);
        let entries = db.sync_entries_updated_since(library_id, checkpoint)?;
        let folders = db.sync_folders_updated_since(library_id, checkpoint)?;
        let memberships = db.sync_entry_folders_updated_since(library_id, checkpoint)?;
        let plan = SyncPlan {
            entries_to_push: entries.len(),
            folders_to_push: folders.len(),
            memberships_to_push: memberships.len(),
        };
        if entries.is_empty() && folders.is_empty() && memberships.is_empty() {
            return Ok(plan);
        }

        let remote = self.turso.remote().await?;
        upsert_remote_entries(&remote, &entries).await?;
        upsert_remote_folders(&remote, &folders).await?;
        upsert_remote_entry_folders(&remote, library_id, &memberships).await?;
        let last_synced_at = entries
            .iter()
            .map(|row| row.updated_at)
            .chain(folders.iter().map(|row| row.updated_at))
            .chain(memberships.iter().map(|row| row.updated_at))
            .max()
            .unwrap_or(checkpoint);
        db.set_sync_checkpoint(library_id, device_id, last_synced_at)?;
        Ok(plan)
    }

    /// Pulls remote metadata newer than the local checkpoint into local sync tables.
    ///
    /// This records remote state and tombstones locally; downloading unknown PDF blobs
    /// and creating full local `entries` rows is intentionally a higher-level step
    /// because it needs a destination library path.
    ///
    /// # Errors
    ///
    /// Returns an error when remote rows cannot be read or local sync rows cannot be saved.
    pub async fn pull_remote_metadata(
        &self,
        db: &Db,
        library_id: &str,
        device_id: &str,
    ) -> Result<SyncPlan> {
        let checkpoint = db.sync_checkpoint(library_id, device_id)?.unwrap_or(0);
        let remote = self.turso.remote().await?;
        let entries = remote_entries_updated_since(&remote, library_id, checkpoint).await?;
        let folders = remote_folders_updated_since(&remote, library_id, checkpoint).await?;
        let memberships =
            remote_entry_folders_updated_since(&remote, library_id, checkpoint).await?;

        for row in &entries {
            db.upsert_sync_entry(row)?;
        }
        for row in &folders {
            db.upsert_sync_folder(row)?;
        }
        for row in &memberships {
            db.upsert_sync_entry_folder(row)?;
        }
        let last_synced_at = entries
            .iter()
            .map(|row| row.updated_at)
            .chain(folders.iter().map(|row| row.updated_at))
            .chain(memberships.iter().map(|row| row.updated_at))
            .max()
            .unwrap_or(checkpoint);
        db.set_sync_checkpoint(library_id, device_id, last_synced_at)?;
        Ok(SyncPlan {
            entries_to_push: entries.len(),
            folders_to_push: folders.len(),
            memberships_to_push: memberships.len(),
        })
    }
}

#[derive(Debug)]
struct PreparedCrdtOperations {
    summary: SyncCrdtPrepareSummary,
    pending_operations: Vec<SyncCrdtOperation>,
}

async fn managed_blob_path_for_entry(
    cache: &BlobCache,
    entry: &LibraryEntry,
) -> Result<Option<PathBuf>> {
    let hash = entry.id.as_str();
    let path = cache.path_for_hash(hash);
    if path.is_file() {
        return Ok(Some(path));
    }
    if !entry.path.is_file() {
        return Ok(None);
    }

    let source_path = entry.path.clone();
    let expected_hash = hash.to_owned();
    let source_hash = tokio::task::spawn_blocking(move || hash_file(&source_path)).await??;
    if source_hash != expected_hash {
        return Ok(None);
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temp_path = path.with_file_name(format!(
        "{}.{}.tmp",
        hash,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    tokio::fs::copy(&entry.path, &temp_path)
        .await
        .with_context(|| {
            format!(
                "Could not copy {} into sync blob cache.",
                entry.path.display()
            )
        })?;

    let copied_path = temp_path.clone();
    let copied_hash = tokio::task::spawn_blocking(move || hash_file(&copied_path)).await??;
    if copied_hash != hash {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Ok(None);
    }

    match tokio::fs::rename(&temp_path, &path).await {
        Ok(()) => {}
        Err(_error) if path.is_file() => {
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Could not install sync blob at {}.", path.display()))
        }
    }
    Ok(Some(path))
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryPayload {
    id: String,
    library_id: String,
    title: Option<String>,
    author: Option<String>,
    #[serde(default)]
    display_title: Option<String>,
    #[serde(default)]
    display_author: Option<String>,
    #[serde(default)]
    metadata_locked: bool,
    #[serde(default)]
    page_count: Option<u16>,
    #[serde(default)]
    last_page: u16,
    #[serde(default)]
    opened_at: Option<i64>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    missing: bool,
    updated_at: i64,
    deleted_at: Option<i64>,
    #[serde(default)]
    purged: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct FolderPayload {
    id: String,
    library_id: String,
    name: String,
    parent_id: Option<String>,
    updated_at: i64,
    deleted_at: Option<i64>,
    #[serde(default)]
    purged: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntryFolderPayload {
    entry_id: String,
    folder_id: String,
    updated_at: i64,
    deleted_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryPayload {
    id: String,
    name: String,
    updated_at: i64,
    deleted_at: Option<i64>,
}

fn prepare_local_crdt_operations(
    db: &Db,
    library_id: &str,
    device_id: &str,
) -> Result<PreparedCrdtOperations> {
    let mut logical_time = db.sync_crdt_max_logical_time(library_id)?.map_or_else(
        || Utc::now().timestamp_millis(),
        |time| Utc::now().timestamp_millis().max(time + 1),
    );
    let mut generated = 0;

    let sync_entry_deleted_at = db
        .sync_entries_updated_since(library_id, i64::MIN)?
        .into_iter()
        .map(|row| (row.id.as_str().to_owned(), row.deleted_at))
        .collect::<BTreeMap<_, _>>();
    let local_entries = db
        .get_all_entries()?
        .into_iter()
        .chain(db.get_trashed_entries()?);
    let mut local_entry_ids = BTreeSet::new();
    for entry in local_entries {
        local_entry_ids.insert(entry.id.as_str().to_owned());
        let mut tags = entry.tags.clone();
        tags.sort();
        tags.dedup();
        let updated_at = entry
            .opened_at
            .unwrap_or(entry.added_at)
            .timestamp()
            .max(entry.added_at.timestamp());
        let payload = EntryPayload {
            id: entry.id.as_str().to_owned(),
            library_id: library_id.to_owned(),
            title: entry.title.clone(),
            author: entry.author.clone(),
            display_title: entry.display_title.clone(),
            display_author: entry.display_author.clone(),
            metadata_locked: entry.metadata_locked,
            page_count: entry.page_count,
            last_page: entry.last_page,
            opened_at: entry.opened_at.map(|timestamp| timestamp.timestamp()),
            tags,
            missing: entry.missing || !entry.path.is_file(),
            updated_at,
            deleted_at: sync_entry_deleted_at
                .get(entry.id.as_str())
                .copied()
                .flatten(),
            purged: false,
        };
        if record_payload_operation(
            db,
            library_id,
            device_id,
            ENTITY_ENTRY,
            entry.id.as_str(),
            &payload,
            logical_time,
        )? {
            generated += 1;
            logical_time += 1;
        }
    }

    for row in db.sync_entries_for_library(library_id)? {
        if row.deleted_at.is_none() || local_entry_ids.contains(row.id.as_str()) {
            continue;
        }
        let payload = EntryPayload {
            id: row.id.as_str().to_owned(),
            library_id: row.library_id.clone(),
            title: row.title.clone(),
            author: row.author.clone(),
            display_title: row.title.clone(),
            display_author: row.author.clone(),
            metadata_locked: false,
            page_count: None,
            last_page: 0,
            opened_at: None,
            tags: Vec::new(),
            missing: true,
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
            purged: true,
        };
        if record_payload_operation(
            db,
            library_id,
            device_id,
            ENTITY_ENTRY,
            row.id.as_str(),
            &payload,
            logical_time,
        )? {
            generated += 1;
            logical_time += 1;
        }
    }

    let local_folder_ids = db
        .library_organization_snapshot()?
        .folders
        .into_iter()
        .map(|folder| folder.id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    for row in db.sync_folders_updated_since(library_id, i64::MIN)? {
        let payload = FolderPayload {
            id: row.id.as_str().to_owned(),
            library_id: row.library_id.clone(),
            name: row.name.clone(),
            parent_id: row.parent_id.as_ref().map(|id| id.as_str().to_owned()),
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
            purged: row.deleted_at.is_some() && !local_folder_ids.contains(row.id.as_str()),
        };
        if record_payload_operation(
            db,
            library_id,
            device_id,
            ENTITY_FOLDER,
            row.id.as_str(),
            &payload,
            logical_time,
        )? {
            generated += 1;
            logical_time += 1;
        }
    }

    for row in db.sync_entry_folders_updated_since(library_id, i64::MIN)? {
        let entity_id = entry_folder_entity_id(&row.entry_id, &row.folder_id);
        let payload = EntryFolderPayload {
            entry_id: row.entry_id.as_str().to_owned(),
            folder_id: row.folder_id.as_str().to_owned(),
            updated_at: row.updated_at,
            deleted_at: row.deleted_at,
        };
        if record_payload_operation(
            db,
            library_id,
            device_id,
            ENTITY_ENTRY_FOLDER,
            &entity_id,
            &payload,
            logical_time,
        )? {
            generated += 1;
            logical_time += 1;
        }
    }

    let pending_operations = db.pending_sync_crdt_operations(library_id, device_id)?;
    Ok(PreparedCrdtOperations {
        summary: SyncCrdtPrepareSummary {
            generated,
            pending_push: pending_operations.len(),
        },
        pending_operations,
    })
}

fn record_payload_operation<T: Serialize>(
    db: &Db,
    library_id: &str,
    device_id: &str,
    entity_kind: &str,
    entity_id: &str,
    payload: &T,
    logical_time: i64,
) -> Result<bool> {
    let payload = serde_json::to_string(payload).context("Could not encode CRDT payload.")?;
    let payload_hash = blake3::hash(payload.as_bytes()).to_hex().to_string();
    let op_id = crdt_operation_id(
        device_id,
        entity_kind,
        entity_id,
        &payload_hash,
        logical_time,
    );
    let operation = SyncCrdtOperation {
        op_id,
        library_id: library_id.to_owned(),
        device_id: device_id.to_owned(),
        logical_time,
        entity_kind: entity_kind.to_owned(),
        entity_id: entity_id.to_owned(),
        payload,
        created_at: Utc::now().timestamp(),
        remote_sequence: None,
        pushed_at: None,
    };
    db.record_sync_crdt_operation_if_changed(&operation, &payload_hash)
}

fn crdt_operation_id(
    device_id: &str,
    entity_kind: &str,
    entity_id: &str,
    payload_hash: &str,
    logical_time: i64,
) -> String {
    let hash = blake3::Hasher::new()
        .update(device_id.as_bytes())
        .update(b"\0")
        .update(entity_kind.as_bytes())
        .update(b"\0")
        .update(entity_id.as_bytes())
        .update(b"\0")
        .update(payload_hash.as_bytes())
        .update(b"\0")
        .update(logical_time.to_string().as_bytes())
        .finalize()
        .to_hex()
        .to_string();
    format!("crdt-{hash}")
}

fn entry_folder_entity_id(
    entry_id: &pdf_folio_db::EntryId,
    folder_id: &pdf_folio_db::FolderId,
) -> String {
    format!("{}\x1f{}", entry_id.as_str(), folder_id.as_str())
}

fn affected_entities_for_operations(
    operations: &[SyncCrdtOperation],
) -> BTreeSet<(String, String)> {
    operations
        .iter()
        .map(|operation| (operation.entity_kind.clone(), operation.entity_id.clone()))
        .collect()
}

fn is_blob_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn winning_entry_payload(
    db: &Db,
    library_id: &str,
    entry_id: &str,
) -> Result<Option<EntryPayload>> {
    let winner = winning_operation_for_entity(db, library_id, ENTITY_ENTRY, entry_id.to_owned())?;
    winner
        .map(|operation| {
            serde_json::from_str::<EntryPayload>(&operation.payload)
                .context("Could not decode winning entry CRDT payload.")
        })
        .transpose()
}

fn apply_entry_payload_to_local(db: &Db, payload: &EntryPayload) -> Result<()> {
    let entry_id = pdf_folio_db::EntryId::new(payload.id.clone());
    if payload.purged {
        db.delete_entry(&entry_id)?;
        return Ok(());
    }
    let Some(entry) = db.entry_by_id(&entry_id)? else {
        return Ok(());
    };
    db.apply_synced_entry_state(
        &entry_id,
        payload.title.as_deref(),
        payload.author.as_deref(),
        payload.display_title.as_deref(),
        payload.display_author.as_deref(),
        payload.metadata_locked,
        payload.page_count,
        payload.last_page,
        payload.opened_at,
        &payload.tags,
    )?;
    db.apply_synced_entry_trash_state(&entry_id, payload.deleted_at)?;
    if payload.missing {
        db.set_missing(&entry_id, true)?;
    } else if entry.path.is_file() {
        db.set_missing(&entry_id, false)?;
    }
    Ok(())
}

fn materialize_crdt_entities(
    db: &Db,
    library_id: &str,
    entities: impl IntoIterator<Item = (String, String)>,
) -> Result<SyncPlan> {
    let mut winners = BTreeMap::new();
    for (entity_kind, entity_id) in entities {
        if let Some(operation) =
            winning_operation_for_entity(db, library_id, &entity_kind, entity_id.clone())?
        {
            winners.insert((entity_kind, entity_id), operation);
        }
    }
    materialize_crdt_winners(db, library_id, winners)
}

fn materialize_crdt_winners(
    db: &Db,
    library_id: &str,
    winners: BTreeMap<(String, String), SyncCrdtOperation>,
) -> Result<SyncPlan> {
    let mut plan = SyncPlan::default();
    for ((entity_kind, entity_id), operation) in winners {
        materialize_crdt_winner(db, library_id, &mut plan, entity_kind, entity_id, operation)?;
    }

    Ok(plan)
}

fn winners_for_operations(
    operations: Vec<SyncCrdtOperation>,
) -> BTreeMap<(String, String), SyncCrdtOperation> {
    let mut winners = BTreeMap::new();
    for operation in operations {
        let key = (operation.entity_kind.clone(), operation.entity_id.clone());
        match winners.get(&key) {
            Some(current) if !operation_wins(&operation, current) => {}
            _ => {
                winners.insert(key, operation);
            }
        }
    }
    winners
}

fn winning_operation_for_entity(
    db: &Db,
    library_id: &str,
    entity_kind: &str,
    entity_id: String,
) -> Result<Option<SyncCrdtOperation>> {
    Ok(winners_for_operations(db.sync_crdt_operations_for_entity(
        library_id,
        entity_kind,
        &entity_id,
    )?)
    .remove(&(entity_kind.to_owned(), entity_id)))
}

fn materialize_crdt_winner(
    db: &Db,
    library_id: &str,
    plan: &mut SyncPlan,
    entity_kind: String,
    entity_id: String,
    operation: SyncCrdtOperation,
) -> Result<()> {
    match entity_kind.as_str() {
        ENTITY_ENTRY => {
            let payload = serde_json::from_str::<EntryPayload>(&operation.payload)
                .context("Could not decode entry CRDT payload.")?;
            db.upsert_sync_entry(&SyncEntryRow {
                id: pdf_folio_db::EntryId::new(payload.id.clone()),
                library_id: payload.library_id.clone(),
                title: payload
                    .display_title
                    .clone()
                    .or_else(|| payload.title.clone()),
                author: payload
                    .display_author
                    .clone()
                    .or_else(|| payload.author.clone()),
                updated_at: payload.updated_at,
                deleted_at: payload.deleted_at,
            })?;
            apply_entry_payload_to_local(db, &payload)?;
            remember_materialized_payload(db, library_id, &operation, &entity_kind, &entity_id)?;
            plan.entries_to_push += 1;
        }
        ENTITY_FOLDER => {
            let payload = serde_json::from_str::<FolderPayload>(&operation.payload)
                .context("Could not decode folder CRDT payload.")?;
            if payload.purged {
                db.delete_folder(&pdf_folio_db::FolderId::new(payload.id.clone()))?;
            }
            db.upsert_sync_folder(&SyncFolderRow {
                id: pdf_folio_db::FolderId::new(payload.id.clone()),
                library_id: payload.library_id.clone(),
                name: payload.name.clone(),
                parent_id: payload.parent_id.clone().map(pdf_folio_db::FolderId::new),
                updated_at: payload.updated_at,
                deleted_at: payload.deleted_at,
            })?;
            db.apply_synced_folder_state(&SyncFolderRow {
                id: pdf_folio_db::FolderId::new(payload.id),
                library_id: payload.library_id,
                name: payload.name,
                parent_id: payload.parent_id.map(pdf_folio_db::FolderId::new),
                updated_at: payload.updated_at,
                deleted_at: payload.deleted_at,
            })?;
            remember_materialized_payload(db, library_id, &operation, &entity_kind, &entity_id)?;
            plan.folders_to_push += 1;
        }
        ENTITY_ENTRY_FOLDER => {
            let payload = serde_json::from_str::<EntryFolderPayload>(&operation.payload)
                .context("Could not decode entry-folder CRDT payload.")?;
            let row = SyncEntryFolderRow {
                entry_id: pdf_folio_db::EntryId::new(payload.entry_id),
                folder_id: pdf_folio_db::FolderId::new(payload.folder_id),
                updated_at: payload.updated_at,
                deleted_at: payload.deleted_at,
            };
            db.upsert_sync_entry_folder(&row)?;
            db.apply_synced_entry_folder_state(&row)?;
            remember_materialized_payload(db, library_id, &operation, &entity_kind, &entity_id)?;
            plan.memberships_to_push += 1;
        }
        _ => {}
    }
    Ok(())
}

fn operation_wins(candidate: &SyncCrdtOperation, current: &SyncCrdtOperation) -> bool {
    (
        candidate.logical_time,
        candidate.device_id.as_str(),
        candidate.op_id.as_str(),
    ) > (
        current.logical_time,
        current.device_id.as_str(),
        current.op_id.as_str(),
    )
}

fn remember_materialized_payload(
    db: &Db,
    library_id: &str,
    operation: &SyncCrdtOperation,
    entity_kind: &str,
    entity_id: &str,
) -> Result<()> {
    let payload_hash = blake3::hash(operation.payload.as_bytes())
        .to_hex()
        .to_string();
    db.remember_sync_crdt_entity_payload(
        library_id,
        entity_kind,
        entity_id,
        &payload_hash,
        operation.logical_time,
        &operation.device_id,
    )
}

fn apply_pulled_crdt_operations(
    db: &Db,
    library_id: &str,
    device_id: &str,
    operations: &[SyncCrdtOperation],
) -> Result<()> {
    let max_remote_sequence = operations
        .iter()
        .filter_map(|operation| operation.remote_sequence)
        .max();
    for operation in operations {
        db.upsert_sync_crdt_operation(operation)?;
    }
    if let Some(sequence) = max_remote_sequence {
        db.set_sync_crdt_remote_cursor(library_id, device_id, sequence)?;
    }
    Ok(())
}

async fn upsert_remote_sync_operations(
    remote: &TursoRemote,
    operations: &[SyncCrdtOperation],
) -> Result<()> {
    for operation in operations {
        remote
            .execute(
                "INSERT INTO sync_operations
                    (op_id, library_id, device_id, logical_time, entity_kind, entity_id,
                     payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(op_id) DO NOTHING",
                vec![
                    TursoValue::text(operation.op_id.as_str()),
                    TursoValue::text(operation.library_id.as_str()),
                    TursoValue::text(operation.device_id.as_str()),
                    TursoValue::integer(operation.logical_time),
                    TursoValue::text(operation.entity_kind.as_str()),
                    TursoValue::text(operation.entity_id.as_str()),
                    TursoValue::text(operation.payload.as_str()),
                    TursoValue::integer(operation.created_at),
                ],
            )
            .await
            .context("Could not append remote CRDT sync operation.")?;
    }
    Ok(())
}

async fn remote_sync_operations_since(
    remote: &TursoRemote,
    library_id: &str,
    since_sequence: i64,
) -> Result<Vec<SyncCrdtOperation>> {
    let rows = remote
        .query(
            "SELECT remote_sequence, op_id, library_id, device_id, logical_time,
                    entity_kind, entity_id, payload, created_at
             FROM sync_operations
             WHERE library_id = ?1 AND remote_sequence > ?2
             ORDER BY remote_sequence ASC",
            vec![
                TursoValue::text(library_id),
                TursoValue::integer(since_sequence),
            ],
        )
        .await
        .context("Could not query remote CRDT sync operations.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncCrdtOperation {
            remote_sequence: Some(row[0].as_i64()?),
            op_id: row[1].as_string()?,
            library_id: row[2].as_string()?,
            device_id: row[3].as_string()?,
            logical_time: row[4].as_i64()?,
            entity_kind: row[5].as_string()?,
            entity_id: row[6].as_string()?,
            payload: row[7].as_string()?,
            created_at: row[8].as_i64()?,
            pushed_at: Some(Utc::now().timestamp()),
        });
    }
    Ok(output)
}

async fn remote_sync_head_sequence(remote: &TursoRemote, library_id: &str) -> Result<i64> {
    let rows = remote
        .query(
            "SELECT COALESCE(MAX(remote_sequence), 0)
             FROM sync_operations
             WHERE library_id = ?1",
            vec![TursoValue::text(library_id)],
        )
        .await
        .context("Could not query remote CRDT sync head.")?;
    rows.first()
        .and_then(|row| row.first())
        .map(TursoValue::as_i64)
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("Remote CRDT sync head query returned no rows."))
}

async fn upsert_remote_libraries(remote: &TursoRemote, rows: &[SyncLibraryRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO libraries
                    (id, name, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                    name = CASE
                        WHEN excluded.updated_at >= libraries.updated_at
                            THEN excluded.name
                        ELSE libraries.name
                    END,
                    deleted_at = CASE
                        WHEN excluded.deleted_at IS NOT NULL
                            THEN COALESCE(libraries.deleted_at, excluded.deleted_at)
                        ELSE libraries.deleted_at
                    END,
                    updated_at = MAX(libraries.updated_at, excluded.updated_at)",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.name.as_str()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote library metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_entries(remote: &TursoRemote, rows: &[SyncEntryRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_entries
                    (id, library_id, title, author, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(library_id, id) DO UPDATE SET
                    title = excluded.title,
                    author = excluded.author,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.library_id.as_str()),
                    TursoValue::nullable_text(row.title.as_deref()),
                    TursoValue::nullable_text(row.author.as_deref()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote entry metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_folders(remote: &TursoRemote, rows: &[SyncFolderRow]) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_folders
                    (id, library_id, name, parent_id, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(library_id, id) DO UPDATE SET
                    name = excluded.name,
                    parent_id = excluded.parent_id,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(row.id.as_str()),
                    TursoValue::text(row.library_id.as_str()),
                    TursoValue::text(row.name.as_str()),
                    TursoValue::nullable_text(row.parent_id.as_ref().map(|id| id.as_str())),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote folder metadata.")?;
    }
    Ok(())
}

async fn upsert_remote_entry_folders(
    remote: &TursoRemote,
    library_id: &str,
    rows: &[SyncEntryFolderRow],
) -> Result<()> {
    for row in rows {
        remote
            .execute(
                "INSERT INTO library_entry_folders
                    (library_id, entry_id, folder_id, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(library_id, entry_id, folder_id) DO UPDATE SET
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
                vec![
                    TursoValue::text(library_id),
                    TursoValue::text(row.entry_id.as_str()),
                    TursoValue::text(row.folder_id.as_str()),
                    TursoValue::integer(row.updated_at),
                    TursoValue::nullable_integer(row.deleted_at),
                ],
            )
            .await
            .context("Could not upsert remote entry-folder metadata.")?;
    }
    Ok(())
}

async fn remote_libraries(remote: &TursoRemote) -> Result<Vec<SyncLibraryRow>> {
    let rows = remote
        .query(
            "SELECT id, name, updated_at, deleted_at
             FROM libraries
             ORDER BY lower(name) ASC, id ASC",
            vec![],
        )
        .await
        .context("Could not query remote libraries.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncLibraryRow {
            id: row[0].as_string()?,
            name: row[1].as_string()?,
            updated_at: row[2].as_i64()?,
            deleted_at: row[3].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_entries_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncEntryRow>> {
    let rows = remote
        .query(
            "SELECT id, library_id, title, author, updated_at, deleted_at
             FROM library_entries
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote entry metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncEntryRow {
            id: pdf_folio_db::EntryId::new(row[0].as_string()?),
            library_id: row[1].as_string()?,
            title: row[2].as_optional_string()?,
            author: row[3].as_optional_string()?,
            updated_at: row[4].as_i64()?,
            deleted_at: row[5].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_folders_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncFolderRow>> {
    let rows = remote
        .query(
            "SELECT id, library_id, name, parent_id, updated_at, deleted_at
             FROM library_folders
             WHERE library_id = ?1 AND updated_at > ?2
             ORDER BY updated_at ASC, id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote folder metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncFolderRow {
            id: pdf_folio_db::FolderId::new(row[0].as_string()?),
            library_id: row[1].as_string()?,
            name: row[2].as_string()?,
            parent_id: row[3]
                .as_optional_string()?
                .map(pdf_folio_db::FolderId::new),
            updated_at: row[4].as_i64()?,
            deleted_at: row[5].as_optional_i64()?,
        });
    }
    Ok(output)
}

async fn remote_entry_folders_updated_since(
    remote: &TursoRemote,
    library_id: &str,
    since: i64,
) -> Result<Vec<SyncEntryFolderRow>> {
    let rows = remote
        .query(
            "SELECT ef.entry_id, ef.folder_id, ef.updated_at, ef.deleted_at
             FROM library_entry_folders ef
             WHERE ef.library_id = ?1 AND ef.updated_at > ?2
             ORDER BY ef.updated_at ASC, ef.entry_id ASC, ef.folder_id ASC",
            vec![TursoValue::text(library_id), TursoValue::integer(since)],
        )
        .await
        .context("Could not query remote entry-folder metadata.")?;
    let mut output = Vec::new();
    for row in rows {
        output.push(SyncEntryFolderRow {
            entry_id: pdf_folio_db::EntryId::new(row[0].as_string()?),
            folder_id: pdf_folio_db::FolderId::new(row[1].as_string()?),
            updated_at: row[2].as_i64()?,
            deleted_at: row[3].as_optional_i64()?,
        });
    }
    Ok(output)
}
