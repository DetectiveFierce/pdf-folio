use crate::library::registry::{
    load_library_preview, sync_library_registry_profiles, sync_library_rows_for_registry,
    LibraryProfile,
};
use crate::*;
use anyhow::Context;
use directories::ProjectDirs;
use iced::futures::SinkExt;
use pdf_folio_db::ImportSummary;

pub(crate) fn mark_entry_opened_task(app: &PDFolioApp) -> Task<Message> {
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
    Finished(anyhow::Result<pdf_folio_cloud::raindrop::RaindropImportSummary>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PendingRaindropRollback {
    entries: Vec<PendingRaindropRollbackEntry>,
    folders: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PendingRaindropRollbackEntry {
    id: String,
    path: PathBuf,
    inserted: bool,
}

impl PendingRaindropRollback {
    pub(crate) fn from_progress(
        imported_entries: Vec<ImportedEntry>,
        created_folders: Vec<FolderId>,
    ) -> Self {
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

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.folders.is_empty()
    }
}

pub(crate) fn pending_raindrop_rollback_path() -> anyhow::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .ok_or_else(|| anyhow::anyhow!("Could not find a data directory for PDF-Folio."))?;
    Ok(project_dirs
        .data_dir()
        .join("raindrop")
        .join("pending-rollback.json"))
}

pub(crate) fn load_pending_raindrop_rollback() -> anyhow::Result<Option<PendingRaindropRollback>> {
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

pub(crate) fn save_pending_raindrop_rollback(
    rollback: &PendingRaindropRollback,
) -> anyhow::Result<()> {
    let path = pending_raindrop_rollback_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| anyhow::anyhow!("Could not create {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(rollback)?)
        .map_err(|error| anyhow::anyhow!("Could not write {}: {error}", path.display()))?;
    Ok(())
}

pub(crate) fn clear_pending_raindrop_rollback() -> anyhow::Result<()> {
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

pub(crate) fn cleanup_raindrop_import_files(
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

pub(crate) fn raindrop_thumbnail_task(pdfs: Vec<RaindropPdfCandidate>) -> Task<Message> {
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

pub(crate) fn raindrop_import_preserves_structure(destination: &RaindropImportDestination) -> bool {
    matches!(
        destination,
        RaindropImportDestination::PreserveRaindropFolders
            | RaindropImportDestination::PreserveRaindropFoldersUnder(_)
    )
}

pub(crate) fn raindrop_import_root_folder(
    destination: &RaindropImportDestination,
) -> Option<FolderId> {
    match destination {
        RaindropImportDestination::PreserveRaindropFoldersUnder(folder_id) => folder_id.clone(),
        RaindropImportDestination::LocalFolder(folder_id) => Some(folder_id.clone()),
        RaindropImportDestination::PreserveRaindropFolders
        | RaindropImportDestination::LibraryRoot => None,
    }
}

pub(crate) fn raindrop_import_destination(
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

pub(crate) fn raindrop_import_task(
    db: Arc<Db>,
    preview: pdf_folio_cloud::raindrop::RaindropImportPreview,
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
                pdf_folio_cloud::raindrop::import_preview_pdfs_with_progress(
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

pub(crate) fn rollback_pending_raindrop_import_task(
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

pub(crate) fn sync_library_registry_task(
    registry: LibraryRegistryRuntime,
    db_path: PathBuf,
    sync_all_after: bool,
    push_local: bool,
) -> Task<Message> {
    Task::perform(
        async move {
            let session = pdf_folio_cloud::sync::cached_session()
                .context("No cached sync session is available.")?;
            let client = pdf_folio_cloud::sync::SyncClient::new(session);
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

pub(crate) fn sync_library_registry_for_app_task(
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

pub(crate) fn pending_raindrop_rollback_check_task() -> Task<Message> {
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

pub(crate) fn auto_sync_task(library: LibraryProfile) -> Task<Message> {
    let library_id = library.id.clone();
    Task::perform(
        async move {
            let session = pdf_folio_cloud::sync::cached_session()
                .context("No cached sync session is available.")?;
            let client = pdf_folio_cloud::sync::SyncClient::new(session);
            client.ensure_remote_schema().await?;
            let cache = pdf_folio_cloud::sync::BlobCache::open_default()?;
            let device_id = default_sync_device_id();
            let db = Db::open(&library.db_path)
                .with_context(|| format!("Could not open {} for sync.", library.name))?;
            client
                .sync_library_if_needed(&db, &library.id, &device_id, &cache)
                .await
                .with_context(|| format!("Could not sync {}.", library.name))
        },
        move |result| Message::AutoSyncFinished {
            library_id: library_id.clone(),
            result: result.map_err(|error| error.to_string()),
        },
    )
}

pub(crate) fn refresh_library_preview_task(profile: LibraryProfile) -> Task<Message> {
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

pub(crate) fn refresh_library_preview_by_id_task(
    app: &PDFolioApp,
    library_id: &str,
) -> Task<Message> {
    app.libraries
        .profiles
        .iter()
        .find(|profile| profile.id == library_id)
        .cloned()
        .map(refresh_library_preview_task)
        .unwrap_or_else(Task::none)
}

pub(crate) fn default_sync_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

pub(crate) fn start_auto_sync_now(app: &mut PDFolioApp) -> Task<Message> {
    if app.sync_auth.is_signed_in() {
        app.sync_queued_libraries
            .insert(app.libraries.active_library_id.clone());
    }
    Task::none()
}

pub(crate) fn start_auto_sync_for_all_libraries(app: &mut PDFolioApp) -> Task<Message> {
    if app.sync_auth.is_signed_in() {
        for library_id in app
            .libraries
            .profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect::<Vec<_>>()
        {
            app.sync_queued_libraries.insert(library_id);
        }
    }
    Task::none()
}

pub(crate) fn auto_sync_library_task(app: &mut PDFolioApp, library_id: String) -> Task<Message> {
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

pub(crate) fn start_next_queued_sync(app: &mut PDFolioApp) -> Task<Message> {
    let Some(library_id) = app.sync_queued_libraries.iter().next().cloned() else {
        return Task::none();
    };
    auto_sync_library_task(app, library_id)
}

pub(crate) fn import_review_from_summary(
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

pub(crate) fn export_entries_for_source(
    app: &PDFolioApp,
    source: &ExportSource,
) -> Vec<LibraryEntry> {
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
