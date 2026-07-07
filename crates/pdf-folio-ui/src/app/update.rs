use super::*;
use crate::app_libraries::{
    create_library_profile, delete_library_profile, load_library_preview, rename_library_profile,
    sync_library_registry_profiles, sync_library_rows_for_registry, LibraryProfile,
};
use anyhow::Context;
use directories::ProjectDirs;
use iced::futures::SinkExt;
use pdf_folio_db::ImportSummary;

#[path = "update/shortcuts.rs"]
mod shortcuts;

fn mark_entry_opened_task(app: &PDFolioApp) -> Task<Message> {
    let Some(entry_id) = app.viewer.current_entry_id.clone() else {
        return Task::none();
    };
    let db = Arc::clone(&app.db);
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.mark_entry_opened(&entry_id)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::ProgressSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

enum RaindropImportTaskEvent {
    CreatedFolder(FolderId),
    Progress(RaindropImportProgress),
    Finished(anyhow::Result<pdf_folio_raindrop::RaindropImportSummary>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PendingRaindropRollback {
    entries: Vec<PendingRaindropRollbackEntry>,
    folders: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PendingRaindropRollbackEntry {
    id: String,
    path: PathBuf,
    inserted: bool,
}

impl PendingRaindropRollback {
    fn from_progress(imported_entries: Vec<ImportedEntry>, created_folders: Vec<FolderId>) -> Self {
        Self {
            entries: imported_entries
                .into_iter()
                .map(|entry| PendingRaindropRollbackEntry {
                    id: entry.id.as_str().to_owned(),
                    path: entry.path,
                    inserted: entry.inserted,
                })
                .collect(),
            folders: created_folders
                .into_iter()
                .map(|folder_id| folder_id.as_str().to_owned())
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.folders.is_empty()
    }
}

fn pending_raindrop_rollback_path() -> anyhow::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .ok_or_else(|| anyhow::anyhow!("Could not find a data directory for PDF-Folio."))?;
    Ok(project_dirs
        .data_dir()
        .join("raindrop")
        .join("pending-rollback.json"))
}

fn load_pending_raindrop_rollback() -> anyhow::Result<Option<PendingRaindropRollback>> {
    let path = pending_raindrop_rollback_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|error| anyhow::anyhow!("Could not read {}: {error}", path.display()))?;
    let rollback = serde_json::from_str::<PendingRaindropRollback>(&json)
        .map_err(|error| anyhow::anyhow!("Could not parse {}: {error}", path.display()))?;
    Ok(Some(rollback))
}

fn save_pending_raindrop_rollback(rollback: &PendingRaindropRollback) -> anyhow::Result<()> {
    let path = pending_raindrop_rollback_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow::anyhow!("Could not create {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(rollback)?)
        .map_err(|error| anyhow::anyhow!("Could not write {}: {error}", path.display()))?;
    Ok(())
}

fn clear_pending_raindrop_rollback() -> anyhow::Result<()> {
    let path = pending_raindrop_rollback_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "Could not remove {}: {error}",
            path.display()
        )),
    }
}

fn cleanup_raindrop_import_files(
    entries: Vec<PendingRaindropRollbackEntry>,
    clear_when_done: bool,
) {
    if entries.is_empty() {
        if clear_when_done {
            if let Err(error) = clear_pending_raindrop_rollback() {
                tracing::debug!(error = %error, "Could not clear pending Raindrop rollback");
            }
        }
        return;
    }

    std::thread::spawn(move || {
        for entry in entries {
            if let Err(error) = std::fs::remove_file(&entry.path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::debug!(
                        path = %entry.path.display(),
                        error = %error,
                        "Could not remove canceled Raindrop import file"
                    );
                }
            }
        }
        if clear_when_done {
            if let Err(error) = clear_pending_raindrop_rollback() {
                tracing::debug!(error = %error, "Could not clear pending Raindrop rollback");
            }
        }
    });
}

fn raindrop_thumbnail_task(pdfs: Vec<RaindropPdfCandidate>) -> Task<Message> {
    let candidates = pdfs
        .into_iter()
        .filter_map(|pdf| pdf.thumbnail_url.map(|url| (pdf.id, url)))
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Task::none();
    }

    Task::perform(
        async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .build()?;
            let limiter = Arc::new(tokio::sync::Semaphore::new(12));
            let mut tasks = Vec::with_capacity(candidates.len());

            for (id, url) in candidates {
                let client = client.clone();
                let limiter = Arc::clone(&limiter);
                tasks.push(tokio::spawn(async move {
                    let _permit = limiter.acquire_owned().await.ok()?;
                    let bytes = client
                        .get(url)
                        .send()
                        .await
                        .ok()?
                        .error_for_status()
                        .ok()?
                        .bytes()
                        .await
                        .ok()?;
                    Some((id, bytes.to_vec()))
                }));
            }

            let mut loaded = Vec::new();
            for task in tasks {
                if let Ok(Some(thumbnail)) = task.await {
                    loaded.push(thumbnail);
                }
            }

            Ok::<_, anyhow::Error>(loaded)
        },
        |result| match result {
            Ok(thumbnails) => Message::RaindropPdfThumbnailsLoaded(thumbnails),
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn raindrop_import_preserves_structure(destination: &RaindropImportDestination) -> bool {
    matches!(
        destination,
        RaindropImportDestination::PreserveRaindropFolders
            | RaindropImportDestination::PreserveRaindropFoldersUnder(_)
    )
}

fn raindrop_import_root_folder(destination: &RaindropImportDestination) -> Option<FolderId> {
    match destination {
        RaindropImportDestination::PreserveRaindropFoldersUnder(folder_id) => folder_id.clone(),
        RaindropImportDestination::LocalFolder(folder_id) => Some(folder_id.clone()),
        RaindropImportDestination::PreserveRaindropFolders
        | RaindropImportDestination::LibraryRoot => None,
    }
}

fn raindrop_import_destination(
    preserve_structure: bool,
    root_folder: Option<FolderId>,
) -> RaindropImportDestination {
    if preserve_structure {
        match root_folder {
            Some(folder_id) => {
                RaindropImportDestination::PreserveRaindropFoldersUnder(Some(folder_id))
            }
            None => RaindropImportDestination::PreserveRaindropFolders,
        }
    } else {
        match root_folder {
            Some(folder_id) => RaindropImportDestination::LocalFolder(folder_id),
            None => RaindropImportDestination::LibraryRoot,
        }
    }
}

fn raindrop_import_task(
    db: Arc<Db>,
    preview: pdf_folio_raindrop::RaindropImportPreview,
    preserve_structure: bool,
    root_folder: Option<FolderId>,
    new_folder_name: Option<String>,
) -> (Task<Message>, iced::task::Handle) {
    Task::run(
        iced::stream::channel(100, async move |mut output| {
            let root_folder = if let Some(name) = new_folder_name {
                match db.create_folder(&name, root_folder.as_ref()) {
                    Ok(folder_id) => {
                        let _ = output
                            .send(RaindropImportTaskEvent::CreatedFolder(folder_id.clone()))
                            .await;
                        Some(folder_id)
                    }
                    Err(error) => {
                        let _ = output
                            .send(RaindropImportTaskEvent::Finished(Err(error)))
                            .await;
                        return;
                    }
                }
            } else {
                root_folder
            };
            let (progress_sender, mut progress_receiver) =
                tokio::sync::mpsc::unbounded_channel::<RaindropImportProgress>();
            let import_db = Arc::clone(&db);
            let mut import_task = tokio::spawn(async move {
                let destination = raindrop_import_destination(preserve_structure, root_folder);
                pdf_folio_raindrop::import_preview_pdfs_with_progress(
                    &import_db,
                    preview,
                    destination,
                    move |progress| {
                        let _ = progress_sender.send(progress);
                    },
                )
                .await
            });

            loop {
                tokio::select! {
                    Some(progress) = progress_receiver.recv() => {
                        if output
                            .send(RaindropImportTaskEvent::Progress(progress))
                            .await
                            .is_err()
                        {
                            import_task.abort();
                            break;
                        }
                    }
                    result = &mut import_task => {
                        let result = match result {
                            Ok(result) => result,
                            Err(error) => Err(anyhow::anyhow!(error)),
                        };
                        if result.is_ok() {
                            let _ = clear_pending_raindrop_rollback();
                        }
                        let _ = output.send(RaindropImportTaskEvent::Finished(result)).await;
                        break;
                    }
                }
            }
        }),
        |event| match event {
            RaindropImportTaskEvent::CreatedFolder(folder_id) => {
                Message::RaindropImportCreatedFolder(folder_id)
            }
            RaindropImportTaskEvent::Progress(progress) => {
                Message::RaindropImportProgressUpdated(progress)
            }
            RaindropImportTaskEvent::Finished(Ok(summary)) => {
                Message::RaindropImportFinished(summary)
            }
            RaindropImportTaskEvent::Finished(Err(error)) => {
                Message::LibraryError(error.to_string())
            }
        },
    )
    .abortable()
}

fn rollback_pending_raindrop_import_task(
    db: Arc<Db>,
    rollback: PendingRaindropRollback,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut removed = 0;
                let mut errors = Vec::new();
                let search_index = pdf_folio_db::SearchIndex::open_default();
                let mut rollback = rollback;
                if let Err(error) = save_pending_raindrop_rollback(&rollback) {
                    errors.push(error.to_string());
                }

                let entries = std::mem::take(&mut rollback.entries);
                let mut inserted_entry_ids = entries
                    .iter()
                    .filter(|entry| entry.inserted)
                    .map(|entry| EntryId::new(entry.id.clone()))
                    .collect::<Vec<_>>();
                inserted_entry_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
                inserted_entry_ids.dedup_by(|left, right| left.as_str() == right.as_str());
                let index_entry_ids = inserted_entry_ids
                    .iter()
                    .map(|entry_id| entry_id.as_str().to_owned())
                    .collect::<Vec<_>>();
                let mut entries_to_cleanup = Vec::new();

                if !inserted_entry_ids.is_empty() {
                    tracing::info!(
                        entry_count = inserted_entry_ids.len(),
                        "Deleting canceled Raindrop import entries in one batch"
                    );
                    if let Err(error) = db.delete_entries(inserted_entry_ids.iter()) {
                        errors.push(error.to_string());
                        rollback.entries = entries;
                    } else {
                        removed += inserted_entry_ids.len();
                        if let Ok(search_index) = search_index.as_ref() {
                            if let Err(error) = search_index
                                .delete_entries(index_entry_ids.iter().map(String::as_str))
                            {
                                errors.push(format!("search index: {error}"));
                            }
                        }
                        entries_to_cleanup = entries;
                    }
                } else {
                    entries_to_cleanup = entries;
                }

                let mut remaining_folders = Vec::new();
                while let Some(folder_id) = rollback.folders.pop() {
                    let folder_id = FolderId::new(folder_id);
                    if let Err(error) = db.delete_folder(&folder_id) {
                        errors.push(format!("{}: {error}", folder_id.as_str()));
                        remaining_folders.push(folder_id.as_str().to_owned());
                    }
                }
                rollback.folders = remaining_folders;

                if rollback.is_empty() {
                    if entries_to_cleanup.is_empty() {
                        if let Err(error) = clear_pending_raindrop_rollback() {
                            errors.push(error.to_string());
                        }
                    } else {
                        cleanup_raindrop_import_files(entries_to_cleanup, true);
                    }
                } else {
                    if !entries_to_cleanup.is_empty() {
                        cleanup_raindrop_import_files(entries_to_cleanup, false);
                    }
                    if let Err(error) = save_pending_raindrop_rollback(&rollback) {
                        errors.push(error.to_string());
                    }
                }

                (removed, errors)
            })
            .await
        },
        |result| match result {
            Ok((removed, errors)) => Message::RaindropImportRollbackFinished { removed, errors },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn sync_library_registry_task(
    registry: LibraryRegistryRuntime,
    db_path: PathBuf,
    sync_all_after: bool,
    push_local: bool,
) -> Task<Message> {
    Task::perform(
        async move {
            let session =
                pdf_folio_sync::cached_session().context("No cached sync session is available.")?;
            let client = pdf_folio_sync::SyncClient::new(session);
            client.ensure_remote_schema().await?;
            let db = Db::open(&db_path)?;
            let rows = push_local
                .then(|| sync_library_rows_for_registry(&registry))
                .unwrap_or_default();
            let remote_libraries = client
                .sync_library_registry(&db, &rows, &default_sync_device_id())
                .await?;
            let (registry, added_library_ids) = tokio::task::spawn_blocking(move || {
                sync_library_registry_profiles(registry, remote_libraries)
            })
            .await??;
            Ok::<_, anyhow::Error>((registry, added_library_ids))
        },
        move |result| Message::LibraryRegistrySyncFinished {
            sync_all_after,
            result: result.map_err(|error| error.to_string()),
        },
    )
}

pub(super) fn sync_library_registry_for_app_task(
    app: &PDFolioApp,
    sync_all_after: bool,
    push_local: bool,
) -> Task<Message> {
    app.libraries
        .active_profile()
        .map(|profile| {
            sync_library_registry_task(
                app.libraries.clone(),
                profile.db_path.clone(),
                sync_all_after,
                push_local,
            )
        })
        .unwrap_or_else(Task::none)
}

pub(super) fn pending_raindrop_rollback_check_task() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(move || {
                load_pending_raindrop_rollback().map(|rollback| {
                    rollback.map(|_| {
                        String::from("Finishing cleanup from an interrupted Raindrop import...")
                    })
                })
            })
            .await
        },
        |result| match result {
            Ok(Ok(status)) => Message::PendingRaindropRollbackChecked(status),
            Ok(Err(error)) => Message::LibraryError(error.to_string()),
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn auto_sync_task(library: LibraryProfile) -> Task<Message> {
    let library_id = library.id.clone();
    Task::perform(
        async move {
            let session =
                pdf_folio_sync::cached_session().context("No cached sync session is available.")?;
            let client = pdf_folio_sync::SyncClient::new(session);
            client.ensure_remote_schema().await?;
            let cache = pdf_folio_sync::BlobCache::open_default()?;
            let device_id = default_sync_device_id();
            let db = Db::open(&library.db_path)
                .with_context(|| format!("Could not open {} for sync.", library.name))?;
            let uploads = client
                .upload_local_blobs(&db, &cache)
                .await
                .with_context(|| format!("Could not upload PDF blobs for {}.", library.name))?;
            let crdt = client
                .sync_crdt_metadata(&db, &library.id, &device_id)
                .await
                .with_context(|| format!("Could not sync metadata for {}.", library.name))?;
            let hydration = client
                .hydrate_remote_library(&db, &library.id, &cache)
                .await
                .with_context(|| format!("Could not hydrate {}.", library.name))?;
            Ok::<_, anyhow::Error>((uploads, crdt, hydration))
        },
        move |result| Message::AutoSyncFinished {
            library_id: library_id.clone(),
            result: result.map_err(|error| error.to_string()),
        },
    )
}

fn refresh_library_preview_task(profile: LibraryProfile) -> Task<Message> {
    Task::perform(
        async move {
            let library_id = profile.id.clone();
            let preview =
                tokio::task::spawn_blocking(move || load_library_preview(&profile)).await?;
            Ok::<_, tokio::task::JoinError>((library_id, preview))
        },
        |result| match result {
            Ok((library_id, preview)) => Message::LibraryPreviewRefreshed {
                library_id,
                preview,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn refresh_library_preview_by_id_task(app: &PDFolioApp, library_id: &str) -> Task<Message> {
    app.libraries
        .profiles
        .iter()
        .find(|profile| profile.id == library_id)
        .cloned()
        .map(refresh_library_preview_task)
        .unwrap_or_else(Task::none)
}

fn default_sync_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

pub(super) fn start_auto_sync_now(app: &mut PDFolioApp) -> Task<Message> {
    auto_sync_library_task(app, app.libraries.active_library_id.clone())
}

pub(super) fn start_auto_sync_for_all_libraries(app: &mut PDFolioApp) -> Task<Message> {
    let library_ids = app
        .libraries
        .profiles
        .iter()
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();
    let mut task = Task::none();
    for library_id in library_ids {
        task = Task::batch([task, auto_sync_library_task(app, library_id)]);
    }
    task
}

fn auto_sync_library_task(app: &mut PDFolioApp, library_id: String) -> Task<Message> {
    if !app.sync_auth.is_signed_in() {
        return Task::none();
    }
    if app.sync_in_progress.is_some() {
        app.sync_queued_libraries.insert(library_id);
        return Task::none();
    }
    let Some(profile) = app
        .libraries
        .profiles
        .iter()
        .find(|profile| profile.id == library_id)
        .cloned()
    else {
        return Task::none();
    };
    app.sync_queued_libraries.remove(&profile.id);
    app.sync_in_progress = Some(profile.id.clone());
    app.last_sync_started_at = Some(Instant::now());
    auto_sync_task(profile)
}

fn start_next_queued_sync(app: &mut PDFolioApp) -> Task<Message> {
    let Some(library_id) = app.sync_queued_libraries.iter().next().cloned() else {
        return Task::none();
    };
    auto_sync_library_task(app, library_id)
}

fn import_review_from_summary(
    title: String,
    summary: &ImportSummary,
    destination_label: String,
    suggested_tags: Vec<String>,
) -> ImportReviewState {
    let imported_entry_ids = summary
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();
    let duplicate_count = summary
        .entries
        .iter()
        .filter(|entry| !entry.inserted)
        .count();
    ImportReviewState {
        title,
        imported_entry_ids,
        imported_count: summary.entries.len().saturating_sub(duplicate_count),
        duplicate_count,
        failed_count: summary.errors.len(),
        destination_label,
        suggested_tags,
        errors: summary.errors.clone(),
    }
}

fn export_entries_for_source(app: &PDFolioApp, source: &ExportSource) -> Vec<LibraryEntry> {
    let all_entries = app
        .library
        .library_entries
        .iter()
        .chain(app.library.library_trash_entries.iter());
    match source {
        ExportSource::SelectedEntries => app.selected_entries(),
        ExportSource::SingleEntry(entry_id) => all_entries
            .filter(|entry| &entry.id == entry_id)
            .cloned()
            .collect(),
        ExportSource::Folder(folder_id) => all_entries
            .filter(|entry| entry.folders.iter().any(|folder| &folder.id == folder_id))
            .cloned()
            .collect(),
        ExportSource::Tag(tag) => all_entries
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == tag))
            .cloned()
            .collect(),
    }
}

pub(super) fn update(app: &mut PDFolioApp, message: Message) -> Task<Message> {
    match message {
        Message::SyncSignInRequested => {
            app.sync_auth.state = SyncAuthState::SigningIn;
            app.sync_auth.error = None;
            return app_sync_auth::sync_sign_in_task(
                app.sync_auth.expected_email.clone(),
                app.sync_auth.server_base_url.clone(),
            );
        }
        Message::SyncSignInFinished(result) => match result {
            Ok(session) => match app.sync_auth.apply_signed_in_session(session) {
                Ok(()) => {
                    app.mode = AppMode::Library;
                    app.library.library_startup_loading = true;
                    return Task::batch([
                        app.refresh_folders(),
                        app.refresh_library(),
                        attribute_pending_metadata_task(Arc::clone(&app.db)),
                        pending_raindrop_rollback_check_task(),
                        sync_library_registry_for_app_task(app, true, true),
                    ]);
                }
                Err(error) => {
                    app.sync_auth.error = Some(error.to_string());
                }
            },
            Err(error) => {
                app.sync_auth.state = SyncAuthState::SignedOut;
                app.sync_auth.error = Some(error);
            }
        },
        Message::AutoSyncTick(_tick) => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            if app.sync_queued_libraries.is_empty() {
                return sync_library_registry_for_app_task(app, false, false);
            }
            return start_next_queued_sync(app);
        }
        Message::RemoteSyncAvailable {
            library_id,
            noticed_at,
            remote_sequence,
        } => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            tracing::debug!(
                remote_sequence,
                library_id = %library_id,
                "Live sync watcher detected remote CRDT updates"
            );
            app.last_sync_started_at = Some(noticed_at);
            app.library.library_status =
                Some(String::from("Syncing updates from another device..."));
            return auto_sync_library_task(app, library_id);
        }
        Message::LibraryRegistryRemoteAvailable {
            noticed_at,
            remote_sequence,
        } => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            tracing::debug!(
                remote_sequence,
                "Live sync watcher detected remote library registry updates"
            );
            app.last_sync_started_at = Some(noticed_at);
            return sync_library_registry_for_app_task(app, false, false);
        }
        Message::AutoSyncFinished { library_id, result } => {
            if app.sync_in_progress.as_deref() == Some(library_id.as_str()) {
                app.sync_in_progress = None;
            }
            let mut follow_up_tasks = Vec::new();
            let library_is_active = app.libraries.active_library_id == library_id;
            let library_name = app
                .libraries
                .profiles
                .iter()
                .find(|profile| profile.id == library_id)
                .map_or(library_id.as_str(), |profile| profile.name.as_str());
            match result {
                Ok((uploads, crdt, hydration)) => {
                    let library_changed = uploads.uploaded_blobs > 0
                        || uploads.failed_blobs > 0
                        || crdt.generated_operations > 0
                        || crdt.pushed_operations > 0
                        || crdt.pulled_operations > 0
                        || hydration.hydrated_entries > 0
                        || hydration.relinked_entries > 0
                        || hydration.hydrated_folders > 0
                        || hydration.hydrated_memberships > 0
                        || hydration.missing_blobs > 0;
                    if uploads.uploaded_blobs > 0
                        || uploads.failed_blobs > 0
                        || crdt.generated_operations > 0
                        || crdt.pushed_operations > 0
                        || crdt.pulled_operations > 0
                        || hydration.hydrated_entries > 0
                        || hydration.relinked_entries > 0
                        || hydration.hydrated_folders > 0
                        || hydration.hydrated_memberships > 0
                        || hydration.missing_blobs > 0
                    {
                        app.library.library_status = Some(format!(
                            "Synced {library_name}: {} PDFs, {} new, {} pushed, {} pulled, {} entries hydrated, {} PDFs healed, {} folders, {} memberships hydrated, {} PDFs missing.",
                            uploads.uploaded_blobs,
                            crdt.generated_operations,
                            crdt.pushed_operations,
                            crdt.pulled_operations,
                            hydration.hydrated_entries,
                            hydration.relinked_entries,
                            hydration.hydrated_folders,
                            hydration.hydrated_memberships,
                            hydration.missing_blobs
                        ));
                    }
                    if library_changed {
                        follow_up_tasks.push(refresh_library_preview_by_id_task(app, &library_id));
                    }
                    if library_is_active
                        && (crdt.pulled_operations > 0
                            || hydration.hydrated_entries > 0
                            || hydration.relinked_entries > 0
                            || hydration.hydrated_folders > 0
                            || hydration.hydrated_memberships > 0
                            || hydration.missing_blobs > 0)
                    {
                        follow_up_tasks.push(Task::batch([
                            app.refresh_folders(),
                            app.refresh_library(),
                            app.request_visible_thumbnails(),
                        ]));
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Automatic PDF-Folio sync failed");
                    app.library.library_status = Some(format!("Sync paused: {error}"));
                }
            }
            if !app.sync_queued_libraries.is_empty() {
                follow_up_tasks.push(start_next_queued_sync(app));
            }
            if !follow_up_tasks.is_empty() {
                return Task::batch(follow_up_tasks);
            }
        }
        Message::LibraryRegistrySyncFinished {
            sync_all_after,
            result,
        } => match result {
            Ok((registry, added_library_ids)) => {
                let registry_task = match app.apply_library_registry(registry) {
                    Ok(task) => task,
                    Err(error) => return Task::done(Message::LibraryError(error.to_string())),
                };
                for library_id in added_library_ids {
                    app.sync_queued_libraries.insert(library_id);
                }
                let sync_task = if sync_all_after {
                    start_auto_sync_for_all_libraries(app)
                } else {
                    start_next_queued_sync(app)
                };
                return Task::batch([registry_task, sync_task]);
            }
            Err(error) => {
                tracing::warn!(%error, "Automatic PDF-Folio library registry sync failed");
                app.library.library_status = Some(format!("Library sync paused: {error}"));
            }
        },
        Message::LibraryPreviewRefreshed {
            library_id,
            preview,
        } => {
            app.libraries.previews.insert(library_id, preview);
        }
        Message::CursorMoved(position) => {
            app.chrome.cursor_position = position;
        }
        Message::ContextMenuOpened(target) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.open_context_menu(target);
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::ContextMenuOpenedAt { target, position } => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.chrome.cursor_position = position;
            app.open_context_menu(target);
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::ContextMenuClosed => {
            app.chrome.open_context_menu = None;
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::OpenCommandPalette => {
            if app.mode == AppMode::Library || app.mode == AppMode::Viewer {
                app.chrome.command_palette_open = true;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
                app.chrome.open_context_menu = None;
                if app.mode == AppMode::Library {
                    return scroll_library_to_offset_task(app.library.library_scroll_offset);
                }
            }
        }
        Message::CloseCommandPalette => {
            app.chrome.command_palette_open = false;
            app.chrome.command_palette_query.clear();
            app.chrome.command_palette_selected_index = 0;
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::CommandPaletteQueryChanged(query) => {
            app.chrome.command_palette_query = query;
            app.chrome.command_palette_selected_index = 0;
        }
        Message::CommandPaletteMoveSelection(delta) => {
            let visible_count = crate::app_commands::library_commands(app)
                .into_iter()
                .filter(|command| {
                    command.visible
                        && crate::app_commands::command_matches(
                            command.spec,
                            &app.chrome.command_palette_query,
                        )
                })
                .count();
            if visible_count > 0 {
                let current = app.chrome.command_palette_selected_index as i32;
                let next = (current + delta).rem_euclid(visible_count as i32) as usize;
                app.chrome.command_palette_selected_index = next;
            }
        }
        Message::CommandPaletteRunSelected => {
            let selected = crate::app_commands::library_commands(app)
                .into_iter()
                .filter(|command| {
                    command.visible
                        && crate::app_commands::command_matches(
                            command.spec,
                            &app.chrome.command_palette_query,
                        )
                })
                .nth(app.chrome.command_palette_selected_index)
                .map(|command| command.spec.id);
            if let Some(command_id) = selected {
                return Task::done(Message::CommandPaletteRun(command_id));
            }
        }
        Message::CommandPaletteRun(command_id) => {
            app.chrome.command_palette_open = false;
            app.chrome.command_palette_query.clear();
            app.chrome.command_palette_selected_index = 0;
            if let Some(message) = crate::app_commands::command_message(app, command_id) {
                return Task::done(message);
            }
        }
        Message::CloseImportReview => {
            app.library.import_review = None;
        }
        Message::SelectImportReviewEntries => {
            if let Some(review) = app.library.import_review.as_ref() {
                let imported_entry_ids = review.imported_entry_ids.clone();
                app.clear_library_selection();
                for entry_id in imported_entry_ids {
                    app.select_library_entry(entry_id);
                }
            }
        }
        Message::ContextMenuActionSelected(action) => {
            if action == ContextMenuAction::SelectOnly {
                if let Some(ContextMenuTarget::LibraryEntry(entry_id)) = app
                    .chrome
                    .open_context_menu
                    .as_ref()
                    .map(|menu| menu.target.clone())
                {
                    app.clear_library_selection();
                    app.select_library_entry(entry_id);
                }
                app.chrome.open_context_menu = None;
                if app.mode == AppMode::Library {
                    return scroll_library_to_offset_task(app.library.library_scroll_offset);
                }
                return Task::none();
            }
            let message = app.context_menu_action_message(action);
            app.chrome.open_context_menu = None;
            if let Some(message) = message {
                return Task::done(message);
            }
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::OpenFileDialog => return open_file_dialog_task(),
        Message::FileDialogCanceled => {}
        Message::FileSelected(path) => {
            app.viewer.pending_document_open = true;
            app.viewer.document_open_started_at = Some(Instant::now());
            return open_document_task(path);
        }
        Message::DocumentOpened { path, doc } => {
            let task = app.open_document_with_path(doc, Some(path));
            return with_session_save(task, app);
        }
        Message::LibraryDocumentOpened { entry_id, doc } => {
            let task = app.open_library_document(entry_id, doc);
            return with_session_save(Task::batch([task, mark_entry_opened_task(app)]), app);
        }
        Message::BackToLibrary => return with_session_save(app.return_to_library(), app),
        Message::BackToViewer => return with_session_save(app.return_to_viewer(), app),
        Message::DocumentError(error) => {
            app.viewer.pending_document_open = false;
            app.viewer.document_open_started_at = None;
            if !app.viewer.dismissed_document_errors.contains(&error) {
                app.viewer.document_error = Some(error);
            }
            app.viewer.pending_renders.clear();
            app.viewer.page_fade_started.clear();
        }
        Message::DismissDocumentError => {
            if let Some(error) = app.viewer.document_error.take() {
                app.viewer.dismissed_document_errors.insert(error);
            }
            app.viewer.document_error = None;
            return app.request_visible_pages();
        }
        Message::PageRendered {
            key,
            data,
            width,
            height,
            generation,
        } => {
            if app.viewer.pending_renders.get(&key) == Some(&generation) {
                app.viewer.pending_renders.remove(&key);
            }
            if generation.is_some_and(|generation| generation != app.viewer.zoom_generation) {
                return Task::none();
            }

            let had_fallback = generation.is_some()
                && key.width_px == app.render_width_px()
                && app.fallback_rendered_page_for_draw(key).is_some();
            app.viewer.cache.insert(key, data.clone());
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.viewer.rendered_pages.insert(
                key,
                RenderedPageView {
                    width,
                    height,
                    handle,
                },
            );
            if had_fallback {
                app.viewer.page_fade_started.insert(key, Instant::now());
            }

            if key.width_px == app.render_width_px()
                && app.all_visible_pages_rendered_at_current_zoom()
            {
                app.viewer.zoom_preview_width_px = None;
            }
        }
        Message::ThemeToggled => {
            app.appearance.theme = app.appearance.theme.toggled();
            return save_app_session_task(app);
        }
        Message::ReloadStyles => {
            return Task::perform(async { StyleBook::load() }, Message::StylesReloaded);
        }
        Message::StylesReloaded(result) => match result {
            Ok(style_book) => {
                app.appearance.style_book = style_book;
                app.appearance.style_load_error = None;
                app.library.library_status = Some(String::from("Styles reloaded."));
            }
            Err(error) => {
                tracing::warn!(%error, "Failed to reload PDF-Folio styles");
                app.appearance.style_load_error = Some(error.clone());
                app.library.library_status = Some(format!("Style reload failed: {error}"));
            }
        },
        Message::ToggleSidebar | Message::ToggleTocPanel => {
            app.viewer.toc_open = !app.viewer.toc_open;
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
            return with_session_save(app.apply_active_dimension_zoom(), app);
        }
        Message::ViewerSidebarTabSelected(tab) => {
            app.viewer.viewer_sidebar_tab = tab;
            return with_session_save(app.request_viewer_thumbnail_pages(), app);
        }
        Message::ToggleViewMode => {
            app.library.compact_view_mode = !app.library.compact_view_mode;
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]);
        }
        Message::LibrarySortChanged(sort_mode) => {
            app.library.library_sort_mode = sort_mode;
            app.library.library_scroll_offset = 0.0;
            app.library.library_drag = None;
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
                app.refresh_library(),
            ]);
        }
        Message::LibraryGridZoomChanged(zoom) => {
            app.library.library_grid_zoom =
                zoom.clamp(app.library_grid_zoom_min(), app.library_grid_zoom_limit());
            app.library.library_scroll_offset = app
                .library
                .library_scroll_offset
                .min(app.max_library_scroll_offset());
            app.update_library_drag_target_from_cursor();
            return Task::batch([save_app_session_task(app), app.request_visible_thumbnails()]);
        }
        Message::LibraryMetadataDensityChanged(density) => {
            app.library.library_metadata_density = density;
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]);
        }
        Message::LibraryLoaded {
            entries,
            trash_entries,
        } => {
            app.library.library_entries = entries;
            app.library.library_trash_entries = trash_entries;
            app.library.library_history_restore_started_at = None;
            app.set_active_library_preview_from_entries();
            app.library.library_startup_loading = false;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_error = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            app.sync_details_editor_to_selection();
            app.library.library_status = Some(format!(
                "{} PDFs in {}",
                app.active_library_entries().len(),
                if app.library.trash_view_active {
                    "trash"
                } else {
                    "library"
                }
            ));
            let restore_task = app.apply_pending_session_to_loaded_library();
            if !app.library.search_query.trim().is_empty() {
                return Task::batch([
                    restore_task,
                    Task::done(Message::SearchDebounced(app.library.search_query.clone())),
                ]);
            }
            return Task::batch([
                restore_task,
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(app.library.library_scroll_offset),
            ]);
        }
        Message::OpenLibrarySwitcher => {
            app.open_library_switcher();
            return save_app_session_task(app);
        }
        Message::CloseLibrarySwitcher => {
            app.libraries.open_menu_library_id = None;
            app.mode = AppMode::Library;
            return save_app_session_task(app);
        }
        Message::SelectLibrary(library_id) => {
            app.libraries.open_menu_library_id = None;
            return match app.select_library(library_id) {
                Ok(task) => task,
                Err(error) => Task::done(Message::LibraryError(error.to_string())),
            };
        }
        Message::ToggleLibraryCardMenu(library_id) => {
            app.libraries.open_menu_library_id = (app.libraries.open_menu_library_id.as_ref()
                != Some(&library_id))
            .then_some(library_id);
        }
        Message::CloseLibraryCardMenu => {
            app.libraries.open_menu_library_id = None;
        }
        Message::OpenCreateLibraryDialog => {
            app.libraries.open_menu_library_id = None;
            app.libraries.new_library_name.clear();
            app.libraries.name_dialog = Some(LibraryNameDialog::Create);
            return operation::focus(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID));
        }
        Message::OpenRenameLibraryDialog(library_id) => {
            let Some(profile) = app
                .libraries
                .profiles
                .iter()
                .find(|profile| profile.id == library_id)
            else {
                return Task::none();
            };
            app.libraries.open_menu_library_id = None;
            app.libraries.new_library_name = profile.name.clone();
            app.libraries.name_dialog = Some(LibraryNameDialog::Rename(library_id));
            return operation::focus(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID));
        }
        Message::CancelLibraryNameDialog => {
            app.libraries.name_dialog = None;
            app.libraries.new_library_name.clear();
        }
        Message::ConfirmLibraryNameDialog => {
            let Some(dialog) = app.libraries.name_dialog.clone() else {
                return Task::none();
            };
            let name = app.libraries.new_library_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            return Task::done(match dialog {
                LibraryNameDialog::Create => Message::CreateLibrary,
                LibraryNameDialog::Rename(library_id) => {
                    app.libraries.rename_inputs.insert(library_id.clone(), name);
                    Message::RenameLibrary(library_id)
                }
            });
        }
        Message::NewLibraryNameChanged(value) => {
            app.libraries.new_library_name = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::CreateLibrary => {
            let name = app.libraries.new_library_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            let registry = app.libraries.clone();
            app.library.library_status = Some(format!("Creating library {name}..."));
            app.libraries.name_dialog = None;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || create_library_profile(registry, name))
                        .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::LibraryRegistryUpdated(registry) => {
            return match app.apply_library_registry(registry) {
                Ok(task) => Task::batch([
                    task,
                    sync_library_registry_for_app_task(app, false, true),
                    start_auto_sync_now(app),
                ]),
                Err(error) => Task::done(Message::LibraryError(error.to_string())),
            };
        }
        Message::LibraryRenameInputChanged { library_id, value } => {
            app.libraries.rename_inputs.insert(
                library_id,
                value
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(80)
                    .collect(),
            );
        }
        Message::RenameLibrary(library_id) => {
            let name = app
                .libraries
                .rename_inputs
                .get(&library_id)
                .cloned()
                .unwrap_or_default();
            if name.trim().is_empty() {
                return Task::none();
            }
            let registry = app.libraries.clone();
            app.libraries.name_dialog = None;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        rename_library_profile(registry, library_id, name)
                    })
                    .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::RequestDeleteLibrary(library_id) => {
            app.libraries.open_menu_library_id = None;
            app.chrome.pending_confirmation = Some(ConfirmationAction::DeleteLibrary(library_id));
        }
        Message::DeleteLibrary(library_id) => {
            let registry = app.libraries.clone();
            app.library.library_status = Some(String::from("Deleting library..."));
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        delete_library_profile(registry, library_id)
                    })
                    .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::LibraryFoldersLoaded(folders) => {
            app.library.library_folders = folders;
            if !app.library.trash_view_active
                && app
                    .library
                    .selected_folder
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.selected_folder = None;
                app.sync_folder_rename_input();
                return save_library_preferences_task(app);
            }
            if !app.library.trash_view_active
                && app
                    .library
                    .details_folder_id
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.details_folder_id = None;
            }
            app.sync_folder_rename_input();
        }
        Message::LibraryTrashFoldersLoaded(folders) => {
            app.library.library_trash_folders = folders;
            if app.library.trash_view_active
                && app
                    .library
                    .selected_folder
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_trash_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.selected_folder = None;
                app.sync_folder_rename_input();
            }
            if app.library.trash_view_active
                && app
                    .library
                    .details_folder_id
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_trash_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.details_folder_id = None;
            }
            app.sync_folder_rename_input();
        }
        Message::PendingRaindropRollbackChecked(status) => {
            if let Some(status) = status {
                app.library.library_startup_loading = true;
                app.library.raindrop_rollback_recovery_active = true;
                app.library.raindrop_rollback_recovery_status = Some(status);
                match load_pending_raindrop_rollback() {
                    Ok(Some(rollback)) => {
                        return rollback_pending_raindrop_import_task(
                            Arc::clone(&app.db),
                            rollback,
                        );
                    }
                    Ok(None) => {
                        app.library.raindrop_rollback_recovery_active = false;
                        return Task::batch([
                            app.refresh_folders(),
                            app.refresh_library(),
                            attribute_pending_metadata_task(Arc::clone(&app.db)),
                        ]);
                    }
                    Err(error) => return Task::done(Message::LibraryError(error.to_string())),
                }
            }
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                attribute_pending_metadata_task(Arc::clone(&app.db)),
            ]);
        }
        Message::PendingRaindropRollbackFinished { removed, errors } => {
            app.library.raindrop_rollback_recovery_status = Some(format!(
                "Finished interrupted Raindrop cleanup and removed {}.",
                format_count(removed, "PDF")
            ));
            if errors.is_empty() {
                app.library.library_error = None;
            } else {
                app.library.library_error = Some(errors.join("\n"));
            }
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                attribute_pending_metadata_task(Arc::clone(&app.db)),
            ]);
        }
        Message::LibraryRefresh => return app.refresh_library(),
        Message::LibraryError(error) => {
            app.library.library_startup_loading = false;
            app.library.library_history_restore_started_at = None;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_status = Some(String::from("Library operation failed."));
            if !app.library.dismissed_library_errors.contains(&error) {
                app.library.library_error = Some(error);
            }
            app.library.raindrop_import_progress = None;
            app.library.bulk_operation_progress = None;
            app.library.pending_thumbnails.clear();
        }
        Message::DismissLibraryError => {
            if let Some(error) = app.library.library_error.take() {
                app.library.dismissed_library_errors.insert(error);
            }
            return scroll_library_to_offset_task(app.library.library_scroll_offset);
        }
        Message::LibraryStatus(status) => {
            app.library.library_status = Some(status);
            app.library.library_error = None;
        }
        Message::OpenImportMenu => {
            app.library.import_menu_open = true;
            app.chrome.open_context_menu = None;
        }
        Message::CloseImportMenu => {
            app.library.import_menu_open = false;
        }
        Message::ImportFolderDialog => {
            app.library.import_menu_open = false;
            return import_folder_dialog_task();
        }
        Message::ImportFolderSelected(path) => {
            app.library.library_status = Some(format!("Importing {}...", path.display()));
            let db = Arc::clone(&app.db);
            app.settings.watch_directories.push(path.clone());
            app.settings.watch_directories.sort();
            app.settings.watch_directories.dedup();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || import_folder_with_index(&db, &path))
                        .await?
                },
                |result| match result {
                    Ok(summary) => Message::ImportFinished(summary),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::ImportPdfDialog => {
            app.library.import_menu_open = false;
            return import_pdf_dialog_task();
        }
        Message::ImportPdfSelected(path) => {
            app.library.library_status = Some(format!("Importing {}...", path.display()));
            let db = Arc::clone(&app.db);
            let destination_folder = app.library.selected_folder.clone();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        import_pdf_with_index(&db, path).and_then(|entry| {
                            if let Some(folder_id) = destination_folder.as_ref() {
                                db.add_entry_to_folder(&entry.id, folder_id)?;
                            }
                            Ok(pdf_folio_db::ImportSummary {
                                entries: vec![entry],
                                errors: Vec::new(),
                            })
                        })
                    })
                    .await?
                },
                |result| match result {
                    Ok(summary) => Message::ImportFinished(summary),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::ImportRaindrop => {
            app.library.import_menu_open = false;
            app.library.library_error = None;
            app.library.raindrop_import_preview = None;
            app.library.raindrop_pdf_thumbnails.clear();
            app.library.selected_raindrop_pdf_ids.clear();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
            if !pdf_folio_raindrop::can_import_without_prompt() {
                app.library.raindrop_connect_dialog_open = true;
                app.library.raindrop_callback_copied = false;
                app.library.library_status =
                    Some(String::from("Connect Raindrop.io to import PDFs."));
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
            app.library.raindrop_import_dialog_open = true;
            app.library.library_status = Some(String::from("Loading Raindrop PDFs..."));
            return Task::perform(
                async move { pdf_folio_raindrop::import_preview().await },
                |result| match result {
                    Ok(preview) => Message::RaindropImportPreviewLoaded(preview),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::RaindropImportPreviewLoaded(preview) => {
            let thumbnail_pdfs = preview.pdfs.clone();
            app.library.selected_raindrop_pdf_ids = preview
                .pdfs
                .iter()
                .map(|pdf| pdf.id)
                .collect::<HashSet<_>>();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_preview = Some(preview);
            app.library.raindrop_import_dialog_open = true;
            app.library.raindrop_connect_dialog_open = false;
            app.library.library_error = None;
            app.library.library_status = Some(String::from("Choose Raindrop PDFs to import."));
            return Task::batch([
                scroll_library_to_offset_task(app.library.library_scroll_offset),
                raindrop_thumbnail_task(thumbnail_pdfs),
            ]);
        }
        Message::RaindropPdfThumbnailsLoaded(thumbnails) => {
            for (id, bytes) in thumbnails {
                app.library
                    .raindrop_pdf_thumbnails
                    .insert(id, image::Handle::from_bytes(bytes));
            }
        }
        Message::RaindropPdfToggled(id, selected) => {
            if selected {
                app.library.selected_raindrop_pdf_ids.insert(id);
            } else {
                app.library.selected_raindrop_pdf_ids.remove(&id);
            }
        }
        Message::SelectAllRaindropPdfs => {
            if let Some(preview) = app.library.raindrop_import_preview.as_ref() {
                app.library.selected_raindrop_pdf_ids = preview
                    .pdfs
                    .iter()
                    .map(|pdf| pdf.id)
                    .collect::<HashSet<_>>();
            }
        }
        Message::ClearAllRaindropPdfs => {
            app.library.selected_raindrop_pdf_ids.clear();
        }
        Message::RaindropDestinationChanged(destination) => {
            app.library.raindrop_import_destination = destination;
        }
        Message::RaindropPreserveFolderStructureToggled(preserve_structure) => {
            let root_folder = raindrop_import_root_folder(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(preserve_structure, root_folder);
        }
        Message::ToggleRaindropImportLocationMenu => {
            app.library.raindrop_import_location_menu_open =
                !app.library.raindrop_import_location_menu_open;
        }
        Message::RaindropImportRootChanged(folder_id) => {
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(preserve_structure, folder_id);
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
        }
        Message::ToggleRaindropImportLocationFolder(folder_id) => {
            if !app
                .library
                .expanded_raindrop_import_location_folders
                .insert(folder_id.clone())
            {
                app.library
                    .expanded_raindrop_import_location_folders
                    .remove(&folder_id);
            }
        }
        Message::StartNewRaindropImportFolder => {
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(preserve_structure, None);
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = true;
            app.library.raindrop_import_new_folder_name.clear();
        }
        Message::RaindropImportNewFolderNameChanged(value) => {
            app.library.raindrop_import_new_folder_name = value;
        }
        Message::ImportSelectedRaindropPdfs => {
            let selected_ids = app.library.selected_raindrop_pdf_ids.clone();
            if selected_ids.is_empty() {
                return Task::none();
            }
            let Some(preview) = app.library.raindrop_import_preview.as_ref() else {
                app.library.library_error = Some(String::from(
                    "Raindrop import metadata is still loading. Try again once the list appears.",
                ));
                return Task::none();
            };
            let selected_pdfs = preview
                .pdfs
                .iter()
                .filter(|pdf| selected_ids.contains(&pdf.id))
                .cloned()
                .collect::<Vec<_>>();
            if selected_pdfs.is_empty() {
                return Task::none();
            }
            let selected_preview = pdf_folio_raindrop::RaindropImportPreview {
                account_id: preview.account_id.clone(),
                account_label: preview.account_label.clone(),
                pdfs: selected_pdfs,
            };
            if app.library.raindrop_import_new_folder_active
                && app
                    .library
                    .raindrop_import_new_folder_name
                    .trim()
                    .is_empty()
            {
                app.library.library_error = Some(String::from(
                    "Enter a new folder name before importing to a new folder.",
                ));
                return Task::none();
            }
            app.library.raindrop_import_dialog_open = false;
            app.library.raindrop_import_progress = Some(RaindropImportProgressView {
                completed: 0,
                total: selected_preview.pdfs.len(),
                current_title: String::from("Preparing import..."),
                phase: pdf_folio_raindrop::RaindropImportPhase::PreparingImports,
                progress_basis_points: None,
                failed: false,
                started_at: Instant::now(),
                imported_entries: Vec::new(),
                created_folders: Vec::new(),
                task_handle: None,
            });
            app.library.library_error = None;
            app.library.library_status = Some(format!(
                "Importing {} Raindrop PDFs...",
                selected_preview.pdfs.len()
            ));
            let db = Arc::clone(&app.db);
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            let root_folder = raindrop_import_root_folder(&app.library.raindrop_import_destination);
            let new_folder_name = app
                .library
                .raindrop_import_new_folder_active
                .then(|| {
                    app.library
                        .raindrop_import_new_folder_name
                        .trim()
                        .to_owned()
                })
                .filter(|name| !name.is_empty());
            let (task, handle) = raindrop_import_task(
                db,
                selected_preview,
                preserve_structure,
                root_folder,
                new_folder_name,
            );
            if let Some(progress) = app.library.raindrop_import_progress.as_mut() {
                progress.task_handle = Some(handle);
            }
            return task;
        }
        Message::RaindropImportProgressUpdated(progress) => {
            let mut imported_entries = app
                .library
                .raindrop_import_progress
                .as_ref()
                .map_or_else(Vec::new, |progress| progress.imported_entries.clone());
            let mut created_folders = app
                .library
                .raindrop_import_progress
                .as_ref()
                .map_or_else(Vec::new, |progress| progress.created_folders.clone());
            if let Some(entry) = progress.entry.clone() {
                if !imported_entries
                    .iter()
                    .any(|existing| existing.path == entry.path)
                {
                    imported_entries.push(entry);
                }
            }
            for folder_id in progress.created_folders {
                if !created_folders.contains(&folder_id) {
                    created_folders.push(folder_id);
                }
            }
            let pending_rollback = PendingRaindropRollback::from_progress(
                imported_entries.clone(),
                created_folders.clone(),
            );
            if !pending_rollback.is_empty() {
                if let Err(error) = save_pending_raindrop_rollback(&pending_rollback) {
                    app.library.library_error = Some(error.to_string());
                }
            }
            let task_handle = app
                .library
                .raindrop_import_progress
                .as_ref()
                .and_then(|progress| progress.task_handle.clone());
            app.library.raindrop_import_progress = Some(RaindropImportProgressView {
                completed: progress.completed,
                total: progress.total,
                current_title: progress.current_title,
                phase: progress.phase,
                progress_basis_points: progress.progress_basis_points,
                failed: progress.failed,
                started_at: app
                    .library
                    .raindrop_import_progress
                    .as_ref()
                    .map_or_else(Instant::now, |progress| progress.started_at),
                imported_entries,
                created_folders,
                task_handle,
            });
        }
        Message::RaindropImportCreatedFolder(folder_id) => {
            if let Some(progress) = app.library.raindrop_import_progress.as_mut() {
                if !progress.created_folders.contains(&folder_id) {
                    progress.created_folders.push(folder_id);
                }
            }
        }
        Message::CancelRaindropImport => {
            let Some(progress) = app.library.raindrop_import_progress.take() else {
                return Task::none();
            };
            if let Some(handle) = progress.task_handle {
                handle.abort();
            }
            let pending_rollback = PendingRaindropRollback::from_progress(
                progress.imported_entries,
                progress.created_folders,
            );
            if pending_rollback.is_empty() {
                app.library.library_status = Some(String::from("Cancelled Raindrop import."));
                return Task::none();
            }
            if let Err(error) = save_pending_raindrop_rollback(&pending_rollback) {
                app.library.library_error = Some(error.to_string());
            }

            app.library.library_startup_loading = false;
            app.library.raindrop_rollback_recovery_active = true;
            app.library.raindrop_rollback_recovery_status =
                Some(String::from("Undoing imported Raindrop PDFs..."));
            app.library.library_status = Some(String::from("Undoing cancelled Raindrop import..."));
            return rollback_pending_raindrop_import_task(Arc::clone(&app.db), pending_rollback);
        }
        Message::RaindropImportRollbackFinished { removed, errors } => {
            app.library.raindrop_import_progress = None;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_status = Some(format!(
                "Cancelled Raindrop import and removed {}.",
                format_count(removed, "PDF")
            ));
            if errors.is_empty() {
                app.library.library_error = None;
            } else {
                app.library.library_error = Some(errors.join("\n"));
            }
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                attribute_pending_metadata_task(Arc::clone(&app.db)),
            ]);
        }
        Message::OpenRaindropIntegrations => {
            app.library.library_status = Some(String::from("Opening Raindrop.io integrations..."));
            return Task::perform(
                async {
                    webbrowser::open("https://app.raindrop.io/settings/integrations")?;
                    Ok::<_, anyhow::Error>(())
                },
                |result| match result {
                    Ok(()) => Message::LibraryStatus(String::from(
                        "Raindrop.io integrations opened in your browser.",
                    )),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::CopyRaindropCallbackUrl => {
            app.library.raindrop_callback_copied = true;
            app.library.library_status = Some(String::from("Callback url copied to clipboard!"));
            return clipboard::write(String::from(pdf_folio_raindrop::OAUTH_CALLBACK_URL));
        }
        Message::RaindropClientIdChanged(value) => {
            app.library.raindrop_callback_copied = false;
            app.library.raindrop_client_id_input = value;
        }
        Message::RaindropClientSecretChanged(value) => {
            app.library.raindrop_callback_copied = false;
            app.library.raindrop_client_secret_input = value;
        }
        Message::SubmitRaindropSignIn => {
            let client_id = app.library.raindrop_client_id_input.trim().to_owned();
            let client_secret = app.library.raindrop_client_secret_input.trim().to_owned();
            if client_id.is_empty() || client_secret.is_empty() {
                app.library.library_error = Some(String::from(
                    "Enter a Raindrop OAuth client ID and client secret before signing in.",
                ));
                return Task::none();
            }
            app.library.raindrop_connect_dialog_open = false;
            app.library.raindrop_import_dialog_open = true;
            app.library.raindrop_import_preview = None;
            app.library.raindrop_pdf_thumbnails.clear();
            app.library.selected_raindrop_pdf_ids.clear();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
            app.library.library_error = None;
            app.library.library_status = Some(String::from(
                "Opening Raindrop.io in your browser for sign-in...",
            ));
            let oauth_config = pdf_folio_raindrop::RaindropOAuthConfig {
                client_id,
                client_secret,
            };
            return Task::perform(
                async move { pdf_folio_raindrop::import_preview_with_auth(Some(oauth_config)).await },
                |result| match result {
                    Ok(preview) => Message::RaindropImportPreviewLoaded(preview),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::RaindropImportFinished(summary) => {
            if let Err(error) = clear_pending_raindrop_rollback() {
                app.library.library_error = Some(error.to_string());
            }
            app.library.raindrop_import_progress = None;
            app.library.import_review = Some(import_review_from_summary(
                format!("Raindrop import from {}", summary.account_label),
                &summary.import,
                String::from("Raindrop destination"),
                Vec::new(),
            ));
            app.library.library_status = Some(format!(
                "Imported {} Raindrop PDFs from {}{}",
                summary.import.entries.len(),
                summary.account_label,
                if summary.import.errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({} skipped)", summary.import.errors.len())
                }
            ));
            if !summary.import.errors.is_empty() {
                app.library.library_error = Some(summary.import.errors.join("\n"));
            }
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::ImportFinished(summary) => {
            let destination_label = app
                .library
                .selected_folder
                .as_ref()
                .and_then(|folder_id| {
                    app.library
                        .library_folders
                        .iter()
                        .find(|folder| &folder.id == folder_id)
                        .map(|folder| folder.name.clone())
                })
                .unwrap_or_else(|| String::from("Library root"));
            app.library.import_review = Some(import_review_from_summary(
                String::from("Import review"),
                &summary,
                destination_label,
                Vec::new(),
            ));
            app.library.library_status = Some(format!(
                "Imported {} PDFs{}",
                summary.entries.len(),
                if summary.errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({} skipped)", summary.errors.len())
                }
            ));
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::AuthorAttributionFinished => return app.refresh_library(),
        Message::OpenLibraryEntry(entry_id) => {
            if let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            {
                app.viewer.pending_document_open = true;
                app.viewer.document_open_started_at = Some(Instant::now());
                return open_library_document_task(entry.id, entry.path);
            }
        }
        Message::LibraryEntryClicked(entry_id) => {
            if app.library.library_drag.is_some() {
                return Task::none();
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.details_folder_id = None;
            app.select_library_entry(entry_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_library_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == &entry_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });

            app.library.last_library_click = Some((entry_id.clone(), now));

            if is_double_click {
                return Task::done(Message::OpenLibraryEntry(entry_id));
            }
            return save_app_session_task(app);
        }
        Message::FolderClicked(folder_id) => {
            if app.library.folder_drag.is_some() {
                return Task::none();
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.select_folder_for_details(folder_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_folder_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == &folder_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });

            app.library.last_folder_click = Some((folder_id.clone(), now));

            if is_double_click {
                return Task::done(Message::FolderSelected(folder_id));
            }
        }
        Message::FolderTreeClicked(folder_id) => {
            if app.library.folder_drag.is_some() {
                return Task::none();
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.select_folder_in_tree(folder_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_folder_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == &folder_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });

            app.library.last_folder_click = Some((folder_id.clone(), now));

            if is_double_click {
                return Task::done(Message::FolderTreeFolderOpened(folder_id));
            }
        }
        Message::FolderTreeFolderOpened(folder_id) => {
            app.open_folder_from_tree(folder_id);
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::OpenTrashCan => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.trash_view_active = true;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.folder_details_sidebar_open = false;
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = None;
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            app.clear_library_selection();
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]);
        }
        Message::EntryCheckboxToggled(entry_id) => {
            app.toggle_library_entry_selection(entry_id);
        }
        Message::MasterCheckboxClicked => match app.master_checkbox_state() {
            MasterCheckboxState::All => app.clear_library_selection(),
            MasterCheckboxState::None | MasterCheckboxState::Partial => {
                app.select_all_visible_library_entries();
            }
        },
        Message::LibraryEntryHoverChanged(entry_id, hovered) => {
            app.set_library_card_hover(entry_id, hovered);
        }
        Message::AnimationFrame(now) => {
            app.tick_animations(now);
        }
        Message::BeginLibraryEntryDrag(entry_id) => {
            app.begin_library_drag(entry_id);
            return scroll_library_to_offset_task(app.library.library_scroll_offset);
        }
        Message::BeginFolderDrag(folder_id) => {
            app.begin_folder_drag(folder_id);
            return scroll_library_to_offset_task(app.library.library_scroll_offset);
        }
        Message::BeginFolderTreeDrag(folder_id) => {
            app.begin_folder_tree_drag(folder_id);
            return scroll_library_to_offset_task(app.library.library_scroll_offset);
        }
        Message::ClearLibrarySelection => {
            app.clear_library_selection();
        }
        Message::ClearLibrarySidebarDetails => {
            app.clear_library_sidebar_details();
        }
        Message::SelectAllVisibleLibraryEntries => {
            app.select_all_visible_library_entries();
        }
        Message::CutLibrarySelection => {
            if app.set_library_clipboard(LibraryClipboardMode::Cut) {
                app.library.library_status = app
                    .library
                    .clipboard
                    .as_ref()
                    .map(|clipboard| format!("{} ready to paste.", clipboard.label()));
            }
        }
        Message::CopyLibrarySelection => {
            if app.set_library_clipboard(LibraryClipboardMode::Copy) {
                app.library.library_status = app
                    .library
                    .clipboard
                    .as_ref()
                    .map(|clipboard| format!("{} ready to paste.", clipboard.label()));
            }
        }
        Message::PasteLibraryClipboard => {
            let Some(clipboard) = app.library.clipboard.clone() else {
                app.library.library_status = Some(String::from("Nothing to paste."));
                return Task::none();
            };
            if !app.can_paste_library_clipboard() {
                app.library.library_status = Some(String::from(
                    "Choose a valid destination before pasting library items.",
                ));
                return Task::none();
            }
            app.library.library_status = Some(format!("{}...", clipboard.paste_label()));
            return paste_library_clipboard_task(
                Arc::clone(&app.db),
                clipboard,
                app.library.selected_folder.clone(),
            );
        }
        Message::LibraryClipboardPasteFinished {
            action,
            clipboard,
            updated,
            errors,
        } => {
            if action.before != action.after {
                app.library.history.push(action);
            }
            if clipboard.mode == LibraryClipboardMode::Cut && errors.is_empty() {
                app.library.clipboard = None;
            }
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "{} {} item{}.",
                    clipboard.paste_label(),
                    updated,
                    if updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "{} {} item{}; {} failed.",
                    clipboard.paste_label(),
                    updated,
                    if updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::LibraryHistoryActionFinished {
            action,
            label,
            updated,
            errors,
        } => {
            app.library.bulk_operation_progress = None;
            if action.before != action.after {
                app.library.history.push(action);
            }
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "{label} {updated} item{}.",
                    if updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "{label} {updated} item{}; {} failed.",
                    if updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::UndoLibraryAction => {
            if app.library.library_history_restore_started_at.is_some() {
                return Task::none();
            }
            let Some((target_index, action)) = app.library.history.undo_target() else {
                app.library.library_status = Some(String::from("Nothing to undo."));
                return Task::none();
            };
            let search_changed_entry_ids = action.after.search_changed_entry_ids(&action.before);
            app.library.library_history_restore_started_at = Some(Instant::now());
            app.library.library_status = Some(format!("Undoing {}...", action.label));
            return restore_library_history_snapshot_task(
                Arc::clone(&app.db),
                action.before,
                target_index,
                format!("Undid {}.", action.label),
                search_changed_entry_ids,
            );
        }
        Message::RedoLibraryAction => {
            if app.library.library_history_restore_started_at.is_some() {
                return Task::none();
            }
            let Some((target_index, action)) = app.library.history.redo_target() else {
                app.library.library_status = Some(String::from("Nothing to redo."));
                return Task::none();
            };
            let search_changed_entry_ids = action.before.search_changed_entry_ids(&action.after);
            app.library.library_history_restore_started_at = Some(Instant::now());
            app.library.library_status = Some(format!("Redoing {}...", action.label));
            return restore_library_history_snapshot_task(
                Arc::clone(&app.db),
                action.after,
                target_index,
                format!("Redid {}.", action.label),
                search_changed_entry_ids,
            );
        }
        Message::LibraryHistoryRestoreFinished {
            target_index,
            status,
        } => {
            app.library.history.set_current(target_index);
            app.library.library_status = Some(status);
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::LibraryEntryDragMoved(position) => {
            app.update_library_drag_target(position);
        }
        Message::FolderDragMoved(position) => {
            app.update_folder_drag_target(position);
        }
        Message::FolderDropTargetChanged(folder_id) => {
            app.set_folder_drop_hover_target(folder_id, Instant::now());
        }
        Message::ParentDirectoryDropTargetChanged(active) => {
            app.set_parent_directory_drop_hover_target(active);
        }
        Message::LibraryAutoScrollTick(tick) => {
            return app.auto_scroll_library_drag(tick);
        }
        Message::EndLibraryEntryDrag => {
            return app.finish_library_drag();
        }
        Message::EndFolderDrag => {
            return app.finish_folder_drag();
        }
        Message::ManualEntryOrderSaved => {
            app.library.library_status = Some(String::from("Manual PDF order saved."));
            return Task::batch([
                app.refresh_library(),
                scroll_library_to_offset_task(app.library.library_scroll_offset),
                start_auto_sync_now(app),
            ]);
        }
        Message::SearchQueryChanged(query) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.search_query = query;
            app.library.library_drag = None;
            app.library.search_generation = app.library.search_generation.wrapping_add(1);
            let query = app.library.search_query.clone();
            if query.trim().is_empty() {
                app.library.search_results = None;
                app.library.search_hit_pages.clear();
                return with_session_save(app.request_visible_thumbnails(), app);
            }
            return with_session_save(schedule_search(query), app);
        }
        Message::SearchDebounced(query) => {
            if query == app.library.search_query {
                let db = Arc::clone(&app.db);
                let sort_mode = app.library.library_sort_mode;
                let trash_view_active = app.library.trash_view_active;
                return Task::perform(
                    search_library_task(db, query, sort_mode, trash_view_active),
                    |result| match result {
                        Ok((entries, hit_pages)) => Message::SearchResults { entries, hit_pages },
                        Err(error) => Message::LibraryError(error.to_string()),
                    },
                );
            }
        }
        Message::SearchResults { entries, hit_pages } => {
            app.library.search_results = Some(entries);
            app.library.search_hit_pages = hit_pages;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return with_session_save(app.request_visible_thumbnails(), app);
        }
        Message::LibraryScrolled {
            offset_y,
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
        } => {
            app.library.library_scroll_offset = offset_y.max(0.0);
            app.library.library_viewport_x = viewport_x;
            app.library.library_viewport_y = viewport_y;
            app.library.library_viewport_width = viewport_width.max(1.0);
            app.library.library_viewport_height = viewport_height.max(1.0);
            app.update_library_drag_target_from_cursor();
            return with_session_save(app.request_visible_thumbnails(), app);
        }
        Message::CollapseLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library.library_tag_sidebar_open = false;
            app.library.resizing_library_tag_sidebar = false;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            return with_session_save(app.request_visible_thumbnails(), app);
        }
        Message::ExpandLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library.library_tag_sidebar_open = true;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            return with_session_save(app.request_visible_thumbnails(), app);
        }
        Message::ToggleLibrarySidebar => {
            if app.mode == AppMode::Library {
                let columns = app.library_entries_per_row();
                app.library.library_tag_sidebar_open = !app.library.library_tag_sidebar_open;
                app.library.resizing_library_tag_sidebar = false;
                app.recalculate_library_viewport_width();
                app.fit_library_grid_zoom_to_columns(columns);
                return with_session_save(app.request_visible_thumbnails(), app);
            }
        }
        Message::BeginTagSidebarResize => {
            app.library.resizing_library_tag_sidebar = true;
        }
        Message::TagSidebarResizeDragged(width) => {
            if app.library.resizing_library_tag_sidebar {
                app.library.library_tag_sidebar_width = width.clamp(
                    app.layout().library_sidebar_min_width,
                    app.layout().library_sidebar_max_width,
                );
                app.recalculate_library_viewport_width();
            }
        }
        Message::EndTagSidebarResize => {
            app.library.resizing_library_tag_sidebar = false;
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]);
        }
        Message::ToggleLibraryInspector => {
            if app.mode == AppMode::Library {
                let columns = app.library_entries_per_row();
                app.library.library_inspector_open = !app.library.library_inspector_open;
                app.library.resizing_library_inspector = false;
                app.recalculate_library_viewport_width();
                app.fit_library_grid_zoom_to_columns(columns);
                return with_session_save(app.request_visible_thumbnails(), app);
            }
        }
        Message::BeginLibraryInspectorResize => {
            app.library.resizing_library_inspector = true;
            app.library.library_inspector_open = true;
        }
        Message::LibraryInspectorResizeDragged(cursor_x) => {
            if app.library.resizing_library_inspector {
                let width = (app.viewer.viewport_width - cursor_x).max(1.0);
                app.library.library_inspector_width = width.clamp(
                    app.layout().metric("LibraryInspector", "min_width", 260.0),
                    app.layout().metric("LibraryInspector", "max_width", 520.0),
                );
                app.recalculate_library_viewport_width();
            }
        }
        Message::EndLibraryInspectorResize => {
            app.library.resizing_library_inspector = false;
            return save_app_session_task(app);
        }
        Message::LibrarySidebarTabChanged(tab) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.library_sidebar_tab = tab;
            return save_app_session_task(app);
        }
        Message::ToggleLibraryTreeRoot => {
            app.library.library_tree_root_expanded = !app.library.library_tree_root_expanded;
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]);
        }
        Message::ToggleLibraryTags => {
            app.library.library_tags_expanded = !app.library.library_tags_expanded;
            return save_app_session_task(app);
        }
        Message::ToggleLibraryTreeFolder(folder_id) => {
            if !app
                .library
                .collapsed_library_tree_folders
                .insert(folder_id.clone())
            {
                app.library
                    .collapsed_library_tree_folders
                    .remove(&folder_id);
            }
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]);
        }
        Message::LibraryWatchEvent(event) => {
            let db = Arc::clone(&app.db);
            app.library.library_status = Some(match &event {
                LibraryWatchEvent::PdfCreated(path) => format!("Importing {}...", path.display()),
                LibraryWatchEvent::PdfRemoved(path) => {
                    format!("Marking missing: {}", path.display())
                }
            });
            return Task::perform(
                async move { tokio::task::spawn_blocking(move || apply_watch_event(&db, event)).await? },
                |result| match result {
                    Ok(()) => Message::LibraryWatchEventApplied(Ok(())),
                    Err(error) => Message::LibraryWatchEventApplied(Err(error.to_string())),
                },
            );
        }
        Message::LibraryWatchEventApplied(result) => match result {
            Ok(()) => {
                return Task::batch([app.refresh_library(), start_auto_sync_now(app)]);
            }
            Err(error) => {
                return Task::batch([
                    Task::done(Message::LibraryError(error)),
                    start_auto_sync_now(app),
                ]);
            }
        },
        Message::TagFilterChanged(tag) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.active_tag_filter = tag;
            app.library.active_recently_opened_filter = false;
            app.library.previous_tag_pill_view = None;
            if app.library.active_tag_filter.is_some() {
                app.library.selected_folder = None;
                app.library.details_folder_id = None;
            }
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]);
        }
        Message::TagTreeClicked(tag) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_tag_click
                    .as_ref()
                    .is_some_and(|(last_tag, last_click)| {
                        last_tag == &tag
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });
            app.library.last_tag_click = Some((tag.clone(), now));
            if is_double_click {
                return Task::done(Message::StartTagRename(tag));
            }
            return Task::done(Message::TagFilterChanged(Some(tag)));
        }
        Message::TagPillClicked(tag) => {
            app.library.previous_tag_pill_view = Some(LibraryViewSnapshot {
                search_query: app.library.search_query.clone(),
                search_results: app.library.search_results.clone(),
                search_hit_pages: app.library.search_hit_pages.clone(),
                active_tag_filter: app.library.active_tag_filter.clone(),
                active_reading_filter: app.library.active_reading_filter,
                active_recently_opened_filter: app.library.active_recently_opened_filter,
                missing_filter_active: app.library.missing_filter_active,
                selected_folder: app.library.selected_folder.clone(),
                details_folder_id: app.library.details_folder_id.clone(),
                library_scroll_offset: app.library.library_scroll_offset,
            });
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = Some(tag);
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]);
        }
        Message::RestoreLibraryViewBeforeTag => {
            if let Some(snapshot) = app.library.previous_tag_pill_view.take() {
                app.library.search_query = snapshot.search_query;
                app.library.search_results = snapshot.search_results;
                app.library.search_hit_pages = snapshot.search_hit_pages;
                app.library.active_tag_filter = snapshot.active_tag_filter;
                app.library.active_reading_filter = snapshot.active_reading_filter;
                app.library.active_recently_opened_filter = snapshot.active_recently_opened_filter;
                app.library.missing_filter_active = snapshot.missing_filter_active;
                app.library.selected_folder = snapshot.selected_folder;
                app.library.details_folder_id = snapshot.details_folder_id;
                app.library.library_drag = None;
                app.library.library_scroll_offset = snapshot.library_scroll_offset.max(0.0);
                app.sync_folder_rename_input();
                let visible_entries = app.visible_library_entries();
                app.prune_selection_to_visible_entries(&visible_entries);
                return Task::batch([
                    app.request_visible_thumbnails(),
                    scroll_library_to_offset_task(app.library.library_scroll_offset),
                    save_app_session_task(app),
                ]);
            }
        }
        Message::ReadingFilterChanged(filter) => {
            app.library.active_reading_filter = filter;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.active_tag_filter = None;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]);
        }
        Message::RecentlyOpenedFilterChanged(active) => {
            app.library.active_recently_opened_filter = active;
            if active {
                app.library.active_reading_filter = None;
                app.library.missing_filter_active = false;
                app.library.active_tag_filter = None;
                app.library.selected_folder = None;
                app.library.details_folder_id = None;
            }
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]);
        }
        Message::MissingFilterChanged(active) => {
            app.library.missing_filter_active = active;
            if active {
                app.library.active_recently_opened_filter = false;
            }
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return with_session_save(app.request_visible_thumbnails(), app);
        }
        Message::FolderSelected(folder_id) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.selected_folder = folder_id.clone();
            app.library.active_recently_opened_filter = false;
            app.library.previous_tag_pill_view = None;
            app.select_folder_for_details(folder_id);
            app.sync_folder_rename_input();
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::ClearLibraryFilters => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = None;
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            return Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]);
        }
        Message::NewFolderNameChanged(value) => {
            app.library.new_folder_name = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::FolderRenameInputChanged(value) => {
            app.library.folder_rename_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::OpenCreateFolderDialog => {
            app.library.create_folder_dialog_open = true;
            return operation::focus(Id::new(LIBRARY_CREATE_FOLDER_INPUT_ID));
        }
        Message::CreateFolder => {
            let name = app.library.new_folder_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            let db = Arc::clone(&app.db);
            let parent_id = app.library.selected_folder.clone();
            app.library.library_status = Some(format!("Creating folder {name}..."));
            app.library.new_folder_name.clear();
            app.library.create_folder_dialog_open = false;
            return create_folder_task(db, name, parent_id);
        }
        Message::RenameSelectedFolder => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Task::none();
            };
            let name = app.library.folder_rename_input.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            app.library.library_status = Some(format!("Renaming folder to {name}..."));
            return rename_folder_task(Arc::clone(&app.db), folder_id, name);
        }
        Message::MoveSelectedFolderToRoot => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Task::none();
            };
            app.library.library_status = Some(String::from("Moving folder to library root..."));
            return move_folder_task(Arc::clone(&app.db), folder_id, None);
        }
        Message::MoveSelectedFolderUp => {
            let Some(folder) = app.details_folder().cloned() else {
                return Task::none();
            };
            let Some(parent_id) = folder.parent_id.as_ref() else {
                return Task::none();
            };
            let grandparent_id = app
                .library
                .library_folders
                .iter()
                .find(|candidate| &candidate.id == parent_id)
                .and_then(|parent| parent.parent_id.clone());
            app.library.library_status = Some(String::from("Moving folder up one level..."));
            return move_folder_task(Arc::clone(&app.db), folder.id, grandparent_id);
        }
        Message::MoveSelectedFolderEarlier => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(-1) else {
                return Task::none();
            };
            app.library.library_status = Some(String::from("Moving folder earlier..."));
            return persist_manual_folder_order_task(Arc::clone(&app.db), parent_id, folder_ids);
        }
        Message::MoveSelectedFolderLater => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(1) else {
                return Task::none();
            };
            app.library.library_status = Some(String::from("Moving folder later..."));
            return persist_manual_folder_order_task(Arc::clone(&app.db), parent_id, folder_ids);
        }
        Message::OpenMoveSelectionDialog => {
            if app.library.selected_library_entries.is_empty() {
                return Task::none();
            }
            app.chrome.open_context_menu = None;
            app.library.move_picker = Some(LibraryMovePicker {
                target: LibraryMoveTarget::SelectedEntries,
                selected_destination: app.library.selected_folder.clone(),
                expanded_folders: app.move_picker_expanded_folders(),
            });
        }
        Message::OpenMoveSelectedFolderDialog => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Task::none();
            };
            let selected_destination = app
                .library
                .library_folders
                .iter()
                .find(|folder| folder.id == folder_id)
                .and_then(|folder| folder.parent_id.clone());
            app.chrome.open_context_menu = None;
            app.library.move_picker = Some(LibraryMovePicker {
                target: LibraryMoveTarget::Folder(folder_id),
                selected_destination,
                expanded_folders: app.move_picker_expanded_folders(),
            });
        }
        Message::MovePickerDestinationSelected(destination) => {
            let Some(picker) = app.library.move_picker.as_mut() else {
                return Task::none();
            };
            if let LibraryMoveTarget::Folder(folder_id) = &picker.target {
                if destination.as_ref() == Some(folder_id)
                    || destination.as_ref().is_some_and(|destination| {
                        !folder_can_move_into(&app.library.library_folders, folder_id, destination)
                    })
                {
                    return Task::none();
                }
            }
            picker.selected_destination = destination;
        }
        Message::ToggleMovePickerFolder(folder_id) => {
            let Some(picker) = app.library.move_picker.as_mut() else {
                return Task::none();
            };
            if !picker.expanded_folders.insert(folder_id.clone()) {
                picker.expanded_folders.remove(&folder_id);
            }
        }
        Message::CancelMovePicker => {
            app.library.move_picker = None;
        }
        Message::ConfirmMovePicker => {
            let Some(picker) = app.library.move_picker.take() else {
                return Task::none();
            };
            match picker.target {
                LibraryMoveTarget::SelectedEntries => {
                    let entry_ids = app
                        .library
                        .selected_library_entries
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    if entry_ids.is_empty() {
                        return Task::none();
                    }
                    app.start_bulk_operation_progress("Moving", entry_ids.len());
                    return move_entries_to_folder_task(
                        Arc::clone(&app.db),
                        entry_ids,
                        picker.selected_destination,
                    );
                }
                LibraryMoveTarget::Folder(folder_id) => {
                    app.library.library_status = Some(String::from("Moving folder..."));
                    return move_folder_task(
                        Arc::clone(&app.db),
                        folder_id,
                        picker.selected_destination,
                    );
                }
            }
        }
        Message::RequestDeleteSelectedFolder => {
            if let Some(folder_id) = app.library.details_folder_id.clone() {
                if app.chrome.folder_delete_warning_suppressed {
                    return Task::done(Message::DeleteFolder(folder_id));
                }
                app.chrome.folder_delete_skip_warning_checked = false;
                app.chrome.pending_confirmation = Some(ConfirmationAction::DeleteFolder(folder_id));
            }
        }
        Message::DeleteFolder(folder_id) => {
            app.library.library_status = Some(String::from("Moving folder to trash..."));
            return delete_folder_task(Arc::clone(&app.db), folder_id);
        }
        Message::FolderUpdated => {
            app.library.library_status = Some(String::from("Folder updated."));
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]);
        }
        Message::FolderCreated { folder_id, action } => {
            if action.before != action.after {
                app.library.history.push(action);
            }
            app.library.library_status = Some(String::from("Folder created."));
            app.library.selected_folder = Some(folder_id);
            app.library.details_folder_id = app.library.selected_folder.clone();
            app.sync_folder_rename_input();
            app.library.library_scroll_offset = 0.0;
            return Task::batch([
                save_library_preferences_task(app),
                app.refresh_folders(),
                app.refresh_library(),
                scroll_library_to_offset_task(0.0),
                start_auto_sync_now(app),
            ]);
        }
        Message::StartTagEntry(entry_id) => {
            app.library.tag_entry_id = Some(entry_id);
            app.library.tag_input.clear();
        }
        Message::TagInputChanged(value) => {
            app.library.tag_input = value;
        }
        Message::SubmitTag => {
            if let Some(entry_id) = app.library.tag_entry_id.clone() {
                let tag = app.library.tag_input.trim().to_owned();
                app.library.tag_entry_id = None;
                app.library.tag_input.clear();
                if !tag.is_empty() {
                    let db = Arc::clone(&app.db);
                    return Task::perform(
                        async move {
                            let saved_entry_id = entry_id.clone();
                            let saved_tag = tag.clone();
                            tokio::task::spawn_blocking(move || {
                                db.add_tag(&saved_entry_id, &saved_tag)
                            })
                            .await??;
                            Ok::<_, anyhow::Error>((entry_id, tag))
                        },
                        |result| match result {
                            Ok((id, tag)) => Message::EntryTagged { id, tag },
                            Err(error) => Message::LibraryError(error.to_string()),
                        },
                    );
                }
            }
        }
        Message::StartTagRename(tag) => {
            app.library.tag_manager_open = false;
            app.library.library_sidebar_tab = LibrarySidebarTab::Tags;
            app.library.renaming_tag = Some(tag.clone());
            app.library.tag_rename_input = tag;
            return operation::focus(Id::new(LIBRARY_TAG_RENAME_INPUT_ID));
        }
        Message::TagRenameInputChanged(value) => {
            app.library.tag_rename_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::SubmitTagRename => {
            let Some(old_tag) = app.library.renaming_tag.take() else {
                return Task::none();
            };
            let new_tag = app.library.tag_rename_input.trim().to_owned();
            app.library.tag_rename_input.clear();
            if new_tag.is_empty() || new_tag == old_tag {
                return Task::none();
            }
            if app.all_tags().iter().any(|tag| tag == &new_tag) {
                app.library.library_error = Some(format!("The tag \"{new_tag}\" already exists."));
                return Task::none();
            }
            if app.library.active_tag_filter.as_ref() == Some(&old_tag) {
                app.library.active_tag_filter = Some(new_tag.clone());
            }
            app.library.library_status = Some(format!("Renaming tag \"{old_tag}\"..."));
            return rename_tag_task(Arc::clone(&app.db), old_tag, new_tag);
        }
        Message::CancelTagRename => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
        }
        Message::DeleteTag(tag) => {
            if app.library.active_tag_filter.as_ref() == Some(&tag) {
                app.library.active_tag_filter = None;
            }
            if app.library.renaming_tag.as_ref() == Some(&tag) {
                app.library.renaming_tag = None;
                app.library.tag_rename_input.clear();
            }
            app.library.library_status = Some(format!("Deleting tag \"{tag}\"..."));
            return delete_tag_task(Arc::clone(&app.db), tag);
        }
        Message::EntryTagged { .. } | Message::EntryUntagged { .. } | Message::EntryDeleted(_) => {
            return Task::batch([app.refresh_library(), start_auto_sync_now(app)]);
        }
        Message::RequestConfirmation(action) => {
            if let ConfirmationAction::DeleteFolder(folder_id) = &action {
                if app.chrome.folder_delete_warning_suppressed {
                    return Task::done(Message::DeleteFolder(folder_id.clone()));
                }
                app.chrome.folder_delete_skip_warning_checked = false;
            }
            app.chrome.pending_confirmation = Some(action);
        }
        Message::CancelConfirmation => {
            app.chrome.pending_confirmation = None;
            app.chrome.folder_delete_skip_warning_checked = false;
        }
        Message::FolderDeleteWarningSuppressionToggled(checked) => {
            app.chrome.folder_delete_skip_warning_checked = checked;
        }
        Message::ConfirmPendingAction => {
            let Some(action) = app.chrome.pending_confirmation.take() else {
                return Task::none();
            };
            if matches!(action, ConfirmationAction::DeleteFolder(_))
                && app.chrome.folder_delete_skip_warning_checked
            {
                app.chrome.folder_delete_warning_suppressed = true;
            }
            app.chrome.folder_delete_skip_warning_checked = false;
            return Task::done(match action {
                ConfirmationAction::BulkResetDisplayMetadata => Message::BulkResetDisplayMetadata,
                ConfirmationAction::BulkDeleteFromLibrary => Message::BulkDeleteFromLibrary,
                ConfirmationAction::PermanentlyDeleteFromTrash => {
                    Message::PermanentlyDeleteSelectedFromTrash
                }
                ConfirmationAction::PermanentlyDeleteFolderFromTrash(folder_id) => {
                    Message::PermanentlyDeleteSelectedFolderFromTrash(folder_id)
                }
                ConfirmationAction::ResetDetailsMetadata(entry_id) => {
                    Message::ResetDetailsMetadata(entry_id)
                }
                ConfirmationAction::DeleteFolder(folder_id) => Message::DeleteFolder(folder_id),
                ConfirmationAction::DeleteTag(tag) => Message::DeleteTag(tag),
                ConfirmationAction::DeleteLibrary(library_id) => Message::DeleteLibrary(library_id),
            });
        }
        Message::DetailsTitleChanged(value) => {
            app.library.details_title_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
        }
        Message::DetailsAuthorChanged(value) => {
            app.library.details_author_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
        }
        Message::SaveDetailsMetadata => {
            let Some(entry_id) = app.library.details_entry_id.clone() else {
                return Task::none();
            };
            let Some(mut entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            entry.display_title = clean_metadata_input(&app.library.details_title_input);
            entry.display_author = clean_metadata_input(&app.library.details_author_input);
            entry.metadata_locked = true;
            app.library.library_status =
                Some(format!("Saving metadata for {}...", entry_title(&entry)));
            return edit_metadata_task(
                Arc::clone(&app.db),
                entry,
                app.library.details_title_input.clone(),
                app.library.details_author_input.clone(),
            );
        }
        Message::ResetDetailsMetadata(entry_id) => {
            let Some(mut entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            entry.display_title = None;
            entry.display_author = None;
            entry.metadata_locked = false;
            app.library.library_status =
                Some(format!("Resetting metadata for {}...", entry_title(&entry)));
            return reset_metadata_task(Arc::clone(&app.db), entry);
        }
        Message::RevealEntryInFileManager(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library.library_status = Some(format!("Revealing {}...", entry_title(&entry)));
            return open_file_manager_task(entry.path, true);
        }
        Message::OpenEntryContainingFolder(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library.library_status =
                Some(format!("Opening folder for {}...", entry_title(&entry)));
            return open_file_manager_task(entry.path, false);
        }
        Message::CopyEntryFilePath(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library.library_status = Some(String::from("File path copied."));
            return clipboard::write(entry.path.display().to_string());
        }
        Message::RelinkMissingEntry(entry_id) => {
            return relink_file_dialog_task(entry_id);
        }
        Message::RelinkFileSelected { entry_id, path } => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Task::none();
            };
            app.library.library_status = Some(format!("Relinking {}...", entry_title(&entry)));
            return relink_entry_task(Arc::clone(&app.db), entry_id, path);
        }
        Message::RelinkFinished { entry_id: _, path } => {
            app.library.library_status = Some(format!("Relinked PDF to {}.", path.display()));
            app.library.library_error = None;
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]);
        }
        Message::MetadataEditFinished {
            entry_id: _,
            action,
            label,
            errors,
        } => {
            if action.before != action.after {
                app.library.history.push(action);
            }
            app.library.library_status = Some(if errors.is_empty() {
                label
            } else {
                format!("{label}; {} indexing errors.", errors.len())
            });
            app.library.details_entry_id = None;
            return app.refresh_library();
        }
        Message::BulkTagInputChanged(value) => {
            app.library.bulk_tag_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::InspectorTagInputChanged(value) => {
            app.library.inspector_tag_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
            app.library.inspector_tag_suggestions_open =
                !app.library.inspector_tag_input.trim().is_empty();
            app.library.inspector_tag_highlighted_index = 0;
        }
        Message::InspectorApplyTag(tag) => {
            let tag = tag.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Task::none();
            }
            app.library.inspector_tag_input.clear();
            app.library.inspector_tag_suggestions_open = false;
            app.library.inspector_tag_highlighted_index = 0;
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tag to", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tag"),
                move |db, entry_id| db.add_tag(entry_id, &tag),
            );
        }
        Message::InspectorAddTag => {
            let tags = app
                .library
                .inspector_tag_input
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if tags.is_empty() || app.library.selected_library_entries.is_empty() {
                return Task::none();
            }
            app.library.inspector_tag_input.clear();
            app.library.inspector_tag_suggestions_open = false;
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tags to", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tags"),
                move |db, entry_id| {
                    for tag in &tags {
                        db.add_tag(entry_id, tag)?;
                    }
                    Ok(())
                },
            );
        }
        Message::InspectorRemoveTag { entry_id, tag } => {
            app.start_bulk_operation_progress("Removing tag from", 1);
            return bulk_operation_task(
                Arc::clone(&app.db),
                vec![entry_id],
                String::from("Removed tag from"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            );
        }
        Message::InspectorRemoveTagFromSelection(tag) => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Removing tag from", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Removed tag from"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            );
        }
        Message::OpenTagManager => {
            app.library.tag_manager_open = true;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
        }
        Message::CloseTagManager => {
            app.library.tag_manager_open = false;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
        }
        Message::TagManagerFilterChanged(value) => {
            app.library.tag_manager_filter = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
        }
        Message::TagManagerMergeDestinationChanged(value) => {
            app.library.tag_manager_merge_destination = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
        }
        Message::MergeTag {
            source,
            destination,
        } => {
            let destination = destination.trim().to_owned();
            if source.trim().is_empty() || destination.is_empty() || source == destination {
                return Task::none();
            }
            app.library.tag_manager_open = false;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            return rename_tag_task(Arc::clone(&app.db), source, destination);
        }
        Message::BulkAddTag => {
            let tag = app.library.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Task::none();
            }
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tag to", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tag"),
                move |db, entry_id| db.add_tag(entry_id, &tag),
            );
        }
        Message::BulkRemoveTag => {
            let tag = app.library.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Task::none();
            }
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Removing tag from", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Untagged"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            );
        }
        Message::BulkAddToCurrentFolder => {
            let Some(folder_id) = app.library.selected_folder.clone() else {
                app.library.library_status =
                    Some(String::from("Open a folder before adding PDFs to it."));
                return Task::none();
            };
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Adding to folder", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Added to folder"),
                String::from("Add PDFs to Folder"),
                move |db, entry_id| db.add_entry_to_folder(entry_id, &folder_id),
            );
        }
        Message::BulkRemoveFromCurrentFolder => {
            let Some(folder_id) = app.library.selected_folder.clone() else {
                app.library.library_status =
                    Some(String::from("Open a folder before removing PDFs from it."));
                return Task::none();
            };
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Removing from folder", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Removed from folder"),
                String::from("Remove PDFs from Folder"),
                move |db, entry_id| db.remove_entry_from_folder(entry_id, &folder_id),
            );
        }
        Message::BulkResetDisplayMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Resetting metadata for", entries.len());
            return bulk_reset_metadata_task(Arc::clone(&app.db), entries);
        }
        Message::BulkApplyTitleSortCleanup => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Cleaning title sort keys for", entry_ids.len());
            return bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Cleaned title sort for"),
                String::from("Clean Title Sort"),
                |db, entry_id| db.apply_title_sort_cleanup(entry_id),
            );
        }
        Message::BulkRefreshPdfMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Refreshing metadata for", entries.len());
            return bulk_refresh_metadata_task(Arc::clone(&app.db), entries);
        }
        Message::BulkRebuildThumbnails => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            for entry in &entries {
                app.library
                    .thumbnails
                    .retain(|key, _| key.entry_id != entry.id);
                app.library
                    .pending_thumbnails
                    .retain(|key| key.entry_id != entry.id);
            }
            app.start_bulk_operation_progress("Rebuilding thumbnails for", entries.len());
            return bulk_thumbnail_task(entries);
        }
        Message::BulkReindex => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Reindexing", entries.len());
            return bulk_reindex_task(entries);
        }
        Message::BulkDeleteFromLibrary => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Moving to trash", entry_ids.len());
            return bulk_delete_metadata_task(Arc::clone(&app.db), entry_ids);
        }
        Message::RestoreSelectedFromTrash => {
            let entries = app.selected_entries();
            let folder_id = app
                .library
                .trash_view_active
                .then(|| app.library.details_folder_id.clone())
                .flatten();
            if entries.is_empty() && folder_id.is_none() {
                return Task::none();
            }
            app.start_bulk_operation_progress(
                "Restoring",
                entries.len() + usize::from(folder_id.is_some()),
            );
            return bulk_restore_trash_items_task(Arc::clone(&app.db), entries, folder_id);
        }
        Message::PermanentlyDeleteSelectedFromTrash => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Task::none();
            }
            app.start_bulk_operation_progress("Permanently deleting", entry_ids.len());
            return bulk_permanently_delete_entries_task(Arc::clone(&app.db), entry_ids);
        }
        Message::PermanentlyDeleteSelectedFolderFromTrash(folder_id) => {
            app.start_bulk_operation_progress("Permanently deleting", 1);
            return permanently_delete_folder_from_trash_task(Arc::clone(&app.db), folder_id);
        }
        Message::TrashFolderPermanentlyDeleted { updated, errors } => {
            app.library.bulk_operation_progress = None;
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "Permanently deleted {updated} item{}.",
                    if updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "Permanently deleted {updated} item{}; {} failed.",
                    if updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]);
        }
        Message::BulkOperationFinished {
            label,
            updated,
            errors,
        } => {
            app.library.bulk_operation_progress = None;
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!("{label} {updated} PDFs.")
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]);
        }
        Message::OpenExportDialog(source) => {
            app.library.export_dialog = Some(LibraryExportDialog::new(source));
            app.library.last_export_summary = None;
        }
        Message::CloseExportDialog => {
            app.library.export_dialog = None;
            app.library.export_progress = None;
            app.library.last_export_summary = None;
        }
        Message::ExportDestinationSelected(path) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.destination = Some(path);
            }
        }
        Message::ChooseExportDestination => return export_destination_dialog_task(),
        Message::ExportModeChanged(mode) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.mode = mode;
            }
        }
        Message::ExportFilenameTemplateChanged(template) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.filename_template = template;
            }
        }
        Message::ExportMetadataCsvToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_metadata_csv = value;
            }
        }
        Message::ExportMetadataJsonToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_metadata_json = value;
            }
        }
        Message::ExportTagsToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_tags = value;
            }
        }
        Message::ExportReadingProgressToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_reading_progress = value;
            }
        }
        Message::ExportConflictBehaviorChanged(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.conflict_behavior = value;
            }
        }
        Message::StartExport => {
            let Some(dialog) = app.library.export_dialog.clone() else {
                return Task::none();
            };
            if dialog.destination.is_none() {
                return export_destination_dialog_task();
            }
            let entries = export_entries_for_source(app, &dialog.source);
            if entries.is_empty() {
                app.library.library_error =
                    Some(String::from("There are no PDFs available for this export."));
                return Task::none();
            }
            app.library.export_progress = Some(LibraryExportProgress {
                label: String::from("Exporting PDFs"),
                total: entries.len(),
                started_at: Instant::now(),
            });
            return export_library_entries_task(entries, dialog);
        }
        Message::ExportFinished(result) => {
            app.library.export_progress = None;
            match result {
                Ok(summary) => {
                    app.library.library_status = Some(format!(
                        "Exported {} PDFs to {}{}",
                        summary.exported,
                        summary.destination.display(),
                        if summary.skipped == 0 {
                            String::new()
                        } else {
                            format!(" ({} skipped)", summary.skipped)
                        }
                    ));
                    if summary.errors.is_empty() {
                        app.library.library_error = None;
                    } else {
                        app.library.library_error = Some(summary.errors.join("\n"));
                    }
                    app.library.last_export_summary = Some(summary);
                }
                Err(error) => {
                    app.library.library_error = Some(error);
                    app.library.library_status = Some(String::from("Export failed."));
                }
            }
        }
        Message::RevealExportedFolder => {
            if let Some(summary) = app.library.last_export_summary.as_ref() {
                return open_file_manager_task(summary.destination.clone(), false);
            }
        }
        Message::CopyExportPath => {
            if let Some(summary) = app.library.last_export_summary.as_ref() {
                app.library.library_status = Some(String::from("Export path copied."));
                return clipboard::write(summary.destination.display().to_string());
            }
        }
        Message::FolderAssignmentFinished {
            folder_id,
            label,
            updated,
            errors,
        } => {
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                if updated > 0 {
                    if let Some(folder_id) = folder_id {
                        app.start_folder_drop_flash(folder_id, Instant::now());
                    }
                }
                format!("{label} {updated} PDFs.")
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            return Task::batch([
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]);
        }
        Message::ThumbnailReady {
            entry_id,
            size,
            data,
            width,
            height,
        } => {
            let key = ThumbnailCacheKey {
                entry_id: entry_id.clone(),
                size,
            };
            app.library.pending_thumbnails.remove(&key);
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.library.thumbnails.insert(
                key,
                ThumbnailView {
                    width,
                    height,
                    handle,
                },
            );
        }
        Message::ProgressUpdated { entry_id, page } => {
            let db = Arc::clone(&app.db);
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || db.update_last_page(&entry_id, page))
                        .await??;
                    Ok::<_, anyhow::Error>(())
                },
                |result| match result {
                    Ok(()) => Message::ProgressSaved,
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::ProgressSaved => return start_auto_sync_now(app),
        Message::LibraryPreferencesSaved | Message::SessionSaved => {}
        Message::OpenJumpDialog => {
            app.viewer.page_input_editing = false;
            app.viewer.jump_dialog_open = true;
            app.viewer.jump_input = app
                .viewer
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
        }
        Message::OpenViewerFind => {
            return app.open_viewer_find();
        }
        Message::CloseViewerFind => {
            app.viewer.viewer_find.open = false;
            return save_app_session_task(app);
        }
        Message::ViewerFindQueryChanged(query) => {
            return with_session_save(app.set_viewer_find_query(query), app);
        }
        Message::ViewerFindPrevious => {
            app.viewer.viewer_find.select_previous();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::ViewerFindNext => {
            app.viewer.viewer_find.select_next();
            return app.scroll_to_selected_viewer_find_match();
        }
        Message::ViewerFindHighlightAllToggled(value) => {
            app.viewer.viewer_find.highlight_all = value;
            return save_app_session_task(app);
        }
        Message::ViewerFindMatchCaseToggled(value) => {
            app.viewer.viewer_find.match_case = value;
            app.refresh_viewer_find_matches();
            return with_session_save(app.scroll_to_selected_viewer_find_match(), app);
        }
        Message::ViewerFindMatchDiacriticsToggled(value) => {
            app.viewer.viewer_find.match_diacritics = value;
            app.refresh_viewer_find_matches();
            return with_session_save(app.scroll_to_selected_viewer_find_match(), app);
        }
        Message::CloseOverlay => {
            if app.chrome.command_palette_open {
                app.chrome.command_palette_open = false;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
            } else if app.libraries.name_dialog.is_some() {
                app.libraries.name_dialog = None;
                app.libraries.new_library_name.clear();
            } else if app.libraries.open_menu_library_id.is_some() {
                app.libraries.open_menu_library_id = None;
            } else if app.mode == AppMode::LibrarySwitcher {
                app.mode = AppMode::Library;
                return save_app_session_task(app);
            } else if app.viewer.jump_dialog_open {
                app.viewer.jump_dialog_open = false;
                app.viewer.jump_input.clear();
            } else if app.viewer.page_input_editing {
                app.viewer.page_input_editing = false;
                app.viewer.jump_input.clear();
            } else if app.viewer.viewer_find.open {
                app.viewer.viewer_find.open = false;
            } else if app.library.create_folder_dialog_open {
                app.library.create_folder_dialog_open = false;
            } else if app.library.move_picker.is_some() {
                app.library.move_picker = None;
            } else if app.library.import_menu_open {
                app.library.import_menu_open = false;
            } else if app.library.raindrop_connect_dialog_open {
                app.library.raindrop_connect_dialog_open = false;
            } else if app.library.raindrop_import_dialog_open {
                app.library.raindrop_import_dialog_open = false;
            } else if app.library.import_review.is_some() {
                app.library.import_review = None;
            } else if app.library.tag_manager_open {
                app.library.tag_manager_open = false;
                app.library.tag_manager_filter.clear();
                app.library.tag_manager_merge_destination.clear();
            } else if app.library.export_dialog.is_some()
                || app.library.export_progress.is_some()
                || app.library.last_export_summary.is_some()
            {
                app.library.export_dialog = None;
                app.library.export_progress = None;
                app.library.last_export_summary = None;
            } else if app.chrome.pending_confirmation.is_some() {
                app.chrome.pending_confirmation = None;
            } else if app.chrome.open_context_menu.is_some() {
                app.chrome.open_context_menu = None;
            } else {
                app.viewer.toc_open = false;
            }
        }
        Message::JumpInputChanged(value) => {
            app.viewer.jump_input = value.chars().filter(char::is_ascii_digit).take(5).collect();
        }
        Message::StartPageInputEdit => {
            app.viewer.jump_dialog_open = false;
            app.viewer.page_input_editing = true;
            app.viewer.jump_input = app
                .viewer
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
            return operation::focus(Id::new(PAGE_INPUT_ID));
        }
        Message::SubmitJump => {
            if let Ok(page) = app.viewer.jump_input.parse::<u16>() {
                return app.jump_to_page(page.saturating_sub(1));
            }
            app.viewer.page_input_editing = false;
            app.viewer.jump_input.clear();
        }
        Message::JumpToPage(page) => return with_session_save(app.jump_to_page(page), app),
        Message::PreviousPage => {
            let page = app.current_page().saturating_sub(1);
            return with_session_save(app.jump_to_page(page), app);
        }
        Message::NextPage => {
            if let Some(doc) = &app.viewer.doc {
                let page = app
                    .current_page()
                    .saturating_add(1)
                    .min(doc.page_count().saturating_sub(1));
                return with_session_save(app.jump_to_page(page), app);
            }
        }
        Message::ToggleOutlineNode(path) => {
            if !app.viewer.expanded_outline_paths.insert(path.clone()) {
                app.viewer.expanded_outline_paths.remove(&path);
            }
            return save_app_session_task(app);
        }
        Message::ViewerTextLayerLoaded { page, layer } => {
            app.viewer.pending_text_layers.remove(&page);
            app.viewer.viewer_text_layers.insert(page, layer);
            let mut tasks = Vec::new();
            if app.viewer.viewer_find.open {
                let previous_match = app.viewer.viewer_find.selected_match();
                app.refresh_viewer_find_matches();
                if !app.viewer.viewer_find.query.is_empty()
                    && previous_match != app.viewer.viewer_find.selected_match()
                    && app.viewer.viewer_find.selected_match().is_some()
                {
                    tasks.push(app.scroll_to_selected_viewer_find_match());
                }
            }
            if app.viewer.viewer_copy_pending && app.selected_text_layers_ready() {
                tasks.push(app.copy_selected_viewer_text());
            }
            if !tasks.is_empty() {
                return Task::batch(tasks);
            }
        }
        Message::ViewerTextLayerError { page, error } => {
            app.viewer.pending_text_layers.remove(&page);
            app.viewer.document_error = Some(error);
        }
        Message::ViewerTextSelectionStarted { page, char_index } => {
            app.start_viewer_text_selection(page, char_index);
        }
        Message::ViewerTextSelectionChanged { page, char_index } => {
            app.update_viewer_text_selection(page, char_index);
        }
        Message::ViewerTextSelectionEnded => {
            app.finish_viewer_text_selection();
        }
        Message::ViewerCanvasClicked => {
            app.clear_viewer_text_selection();
        }
        Message::ClearViewerTextSelection => {
            app.clear_viewer_text_selection();
        }
        Message::CopyViewerTextSelection => {
            return app.copy_selected_viewer_text();
        }
        Message::ScrollChanged(offset) => {
            app.viewer.last_scroll_offset = app.viewer.scroll_offset;
            app.viewer.scroll_offset = offset;
            app.clamp_scroll_offset();
            let render_task = app.request_visible_pages();
            let progress_task =
                app.viewer
                    .current_entry_id
                    .clone()
                    .map_or_else(Task::none, |entry_id| {
                        Task::done(Message::ProgressUpdated {
                            entry_id,
                            page: app.current_page(),
                        })
                    });
            return Task::batch([render_task, progress_task, save_app_session_task(app)]);
        }
        Message::ViewportChanged {
            horizontal_offset,
            scroll_offset,
            width,
            height,
        } => {
            app.viewer.last_scroll_offset = app.viewer.scroll_offset;
            app.viewer.horizontal_offset = horizontal_offset;
            app.viewer.scroll_offset = scroll_offset;
            app.viewer.viewer_viewport_width = width.max(1.0);
            app.viewer.viewer_viewport_height = height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();
            return Task::batch([
                app.apply_active_dimension_zoom(),
                app.request_visible_pages(),
                save_app_session_task(app),
            ]);
        }
        Message::WindowResized { width, height } => {
            app.viewer.viewport_width = width.max(1.0);
            app.viewer.viewport_height = height.max(1.0);
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
            if app.mode == AppMode::Library {
                app.recalculate_library_viewport_width();
                app.library.library_viewport_height =
                    (app.viewer.viewport_height - Spacing::LG * 2.0).max(1.0);
                return with_session_save(app.request_visible_thumbnails(), app);
            }
            return with_session_save(app.apply_active_dimension_zoom(), app);
        }
        Message::ViewportWheelScrolled {
            delta_x,
            delta_y,
            cursor,
            viewport_width,
            viewport_height,
        } => {
            app.viewer.viewer_viewport_width = viewport_width.max(1.0);
            app.viewer.viewer_viewport_height = viewport_height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();

            if app.viewer.modifiers.control() {
                app.viewer.active_zoom_preset = None;
                let direction = if delta_y.abs() >= delta_x.abs() {
                    delta_y
                } else {
                    -delta_x
                };
                let step = if direction > 0.0 { 100 } else { -100 };
                let width = (i32::from(app.viewer.zoom_width) + step)
                    .clamp(i32::from(MIN_ZOOM_WIDTH), i32::from(MAX_ZOOM_WIDTH))
                    as u16;
                let task = app.zoom_to_width(width, Some(cursor), ZoomRenderPolicy::Debounced);
                return with_session_save(task, app);
            }

            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
                let direction = if delta_y < 0.0 || delta_x > 0.0 {
                    1
                } else {
                    -1
                };
                let task = app.scroll_page_mode_by(direction);
                return with_session_save(task, app);
            }

            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                let delta = if delta_x != 0.0 { delta_x } else { delta_y };
                app.viewer.horizontal_offset =
                    (app.viewer.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
                return Task::batch([
                    app.request_visible_pages(),
                    app.scroll_viewer_to_offsets_task(),
                    save_app_session_task(app),
                ]);
            }

            if app.viewer.modifiers.shift() || delta_x != 0.0 {
                let delta = if delta_x != 0.0 { delta_x } else { delta_y };
                app.viewer.horizontal_offset =
                    (app.viewer.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
                return Task::batch([
                    app.request_visible_pages(),
                    app.scroll_viewer_to_offsets_task(),
                    save_app_session_task(app),
                ]);
            } else {
                app.viewer.last_scroll_offset = app.viewer.scroll_offset;
                app.viewer.scroll_offset =
                    (app.viewer.scroll_offset - delta_y).clamp(0.0, app.max_scroll_offset());
                return with_session_save(app.request_visible_pages(), app);
            }
        }
        Message::ModifiersChanged(modifiers) => {
            app.viewer.modifiers = modifiers;
        }
        Message::ZoomRenderSettled(generation) => {
            if generation == app.viewer.zoom_generation {
                return app.request_visible_pages();
            }
        }
        Message::ZoomIn => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(
                app.viewer.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
            return with_session_save(task, app);
        }
        Message::ZoomOut => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(
                app.viewer.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            );
            return with_session_save(task, app);
        }
        Message::ShortcutPressed(shortcut) => return shortcuts::handle_shortcut(app, shortcut),
        Message::ZoomSet(width) => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
            return with_session_save(task, app);
        }
        Message::StartZoomInputEdit => {
            app.viewer.zoom_editing = true;
            app.viewer.zoom_menu_open = false;
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            return operation::focus(Id::new(ZOOM_INPUT_ID));
        }
        Message::ZoomInputChanged(value) => {
            app.viewer.zoom_input = value;
        }
        Message::SubmitZoomInput => {
            let width = width_from_percent_input(&app.viewer.zoom_input);
            app.viewer.zoom_editing = false;
            if let Some(width) = width {
                app.viewer.active_zoom_preset = None;
                let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
                return with_session_save(task, app);
            }
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
        }
        Message::ToggleZoomMenu => {
            app.chrome.open_context_menu = None;
            app.viewer.zoom_menu_open = !app.viewer.zoom_menu_open;
            app.viewer.zoom_editing = false;
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
        }
        Message::CloseZoomMenu => {
            app.viewer.zoom_menu_open = false;
        }
        Message::ZoomPresetSelected(preset) => {
            app.viewer.zoom_menu_open = false;
            app.viewer.zoom_editing = false;
            app.viewer.active_zoom_preset = Some(preset);
            let width = preset.width_for(app);
            app.viewer.zoom_input = zoom_percent_label(width);
            let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
            if matches!(preset, ZoomPreset::PageWidth) {
                app.viewer.horizontal_offset = 0.0;
            }
            return with_session_save(task, app);
        }
        Message::ViewerScrollModeSelected(mode) => {
            let task = app.set_viewer_scroll_mode(mode);
            return with_session_save(task, app);
        }
        Message::ViewerSpreadModeSelected(mode) => {
            let task = app.set_viewer_spread_mode(mode);
            return with_session_save(task, app);
        }
        _ => {}
    }

    Task::none()
}
