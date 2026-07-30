//! Async task constructors and blocking helpers for library operations.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use directories::ProjectDirs;
use iced::futures::SinkExt;
use iced::Task;
use pdf_folio_core::PdfDoc;
use pdf_folio_core::{
    hash_file, scan_pdf_files, Db, EntryId, Folder, FolderId, ImportSummary, ImportedEntry,
    IndexDocument, LibraryEntry, LibraryOrganizationSnapshot, LibrarySortMode, LibraryWatchEvent,
    NewLibraryEntry, SearchIndex,
};

use crate::library::filters::entry_matches_query;
use crate::library::metadata::{entry_author, entry_title, file_size};
use crate::library::thumbnails::cache_thumbnail_variants;
use crate::messages::Message;
use crate::{
    ExportConflictBehavior, ExportFilenameTemplate, ExportMode, ExportSource, ImportReviewState,
    LibraryClipboard, LibraryClipboardMode, LibraryClipboardTarget, LibraryExportDialog,
    LibraryExportSummary, LibraryHistoryAction, PDFolioApp, RaindropImportDestination,
    RaindropImportProgress, RaindropPdfCandidate,
};

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

pub(crate) fn persist_manual_entry_order_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.set_manual_entry_order(&entry_ids)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Manual PDF Order"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Manual PDF order saved"),
                    entry_ids.len(),
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
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
                .timeout(std::time::Duration::from_secs(12))
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
                let search_index = SearchIndex::open_default();
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

pub(crate) fn persist_manual_folder_entry_order_task(
    db: Arc<Db>,
    folder_id: FolderId,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.set_manual_folder_entry_order(&folder_id, &entry_ids)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Manual Folder PDF Order"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Manual folder PDF order saved"),
                    entry_ids.len(),
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn persist_manual_folder_order_task(
    db: Arc<Db>,
    parent_id: Option<FolderId>,
    folder_ids: Vec<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.set_manual_folder_order(parent_id.as_ref(), &folder_ids)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Manual Folder Order"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Manual folder order saved"),
                    folder_ids.len(),
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_operation_task<F>(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    label: String,
    history_label: String,
    operation: F,
) -> Task<Message>
where
    F: Fn(&Db, &EntryId) -> anyhow::Result<()> + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match operation(&db, &entry_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: history_label,
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    label,
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn rename_tag_task(db: Arc<Db>, old_tag: String, new_tag: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let updated = db.rename_tag(&old_tag, &new_tag)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Rename Tag"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Renamed tag"),
                    updated,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn delete_tag_task(db: Arc<Db>, tag: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let updated = db.delete_tag(&tag)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Delete Tag"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Deleted tag from"),
                    updated,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn rename_folder_task(db: Arc<Db>, folder_id: FolderId, name: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.rename_folder(&folder_id, &name)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Rename Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Renamed folder"),
                    1,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn move_folder_task(
    db: Arc<Db>,
    folder_id: FolderId,
    new_parent_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.move_folder(&folder_id, new_parent_id.as_ref())?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved folder"),
                    1,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn delete_folder_task(db: Arc<Db>, folder_id: FolderId) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let folders = db.get_folders()?;
                let folder_ids = folder_subtree_ids(&folders, &folder_id);
                let mut entry_ids = HashSet::new();
                for folder_id in &folder_ids {
                    for entry in db.entries_in_folder(folder_id)? {
                        entry_ids.insert(entry.id);
                    }
                }
                db.trash_folder_tree(&folder_id)?;
                if !entry_ids.is_empty() {
                    let search_index = SearchIndex::open_default()?;
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))?;
                }
                let after = db.library_organization_snapshot()?;
                let updated = folder_ids.len() + entry_ids.len();
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move Folder to Trash"),
                        refresh_search_on_restore: before.trash_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved to trash"),
                    updated,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn folder_subtree_ids(folders: &[Folder], folder_id: &FolderId) -> HashSet<FolderId> {
    let mut folder_ids = HashSet::new();
    collect_folder_subtree_ids(folders, folder_id, &mut folder_ids);
    folder_ids
}

fn collect_folder_subtree_ids(
    folders: &[Folder],
    folder_id: &FolderId,
    folder_ids: &mut HashSet<FolderId>,
) {
    if !folder_ids.insert(folder_id.clone()) {
        return;
    }
    for child in folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
    {
        collect_folder_subtree_ids(folders, &child.id, folder_ids);
    }
}

pub(crate) fn move_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    let result = if let Some(folder_id) = folder_id.as_ref() {
                        db.move_entry_to_folder(&entry_id, folder_id)
                    } else {
                        db.move_entry_to_root(&entry_id)
                    };
                    match result {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move PDFs"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn add_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: FolderId,
) -> Task<Message> {
    bulk_operation_task(
        db,
        entry_ids,
        String::from("Added to folder"),
        String::from("Add PDFs to Folder"),
        move |db, entry_id| db.add_entry_to_folder(entry_id, &folder_id),
    )
}

pub(crate) fn create_folder_task(
    db: Arc<Db>,
    name: String,
    parent_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let folder_id = db.create_folder(&name, parent_id.as_ref())?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    folder_id,
                    LibraryHistoryAction {
                        label: String::from("Create Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                ))
            })
            .await?
        },
        |result| match result {
            Ok((folder_id, action)) => Message::FolderCreated { folder_id, action },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn paste_library_clipboard_task(
    db: Arc<Db>,
    clipboard: LibraryClipboard,
    destination_folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();

                match &clipboard.target {
                    LibraryClipboardTarget::Entries(entry_ids) => {
                        for entry_id in entry_ids {
                            let result = match clipboard.mode {
                                LibraryClipboardMode::Cut => {
                                    if let Some(folder_id) = destination_folder_id.as_ref() {
                                        db.move_entry_to_folder(entry_id, folder_id)
                                    } else {
                                        db.move_entry_to_root(entry_id)
                                    }
                                }
                                LibraryClipboardMode::Copy => {
                                    if let Some(folder_id) = destination_folder_id.as_ref() {
                                        db.add_entry_to_folder(entry_id, folder_id)
                                    } else {
                                        anyhow::bail!(
                                            "Copied PDFs can only be pasted into a folder."
                                        );
                                    }
                                }
                            };

                            match result {
                                Ok(()) => updated += 1,
                                Err(error) => {
                                    errors.push(format!("{}: {error}", entry_id.as_str()));
                                }
                            }
                        }
                    }
                    LibraryClipboardTarget::Folder(folder_id) => match clipboard.mode {
                        LibraryClipboardMode::Cut => {
                            db.move_folder(folder_id, destination_folder_id.as_ref())?;
                            updated = 1;
                        }
                        LibraryClipboardMode::Copy => {
                            db.copy_folder_subtree(folder_id, destination_folder_id.as_ref())?;
                            updated = 1;
                        }
                    },
                }

                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: clipboard.paste_label().to_owned(),
                        refresh_search_on_restore: before.trash_state_differs_from(&after),
                        before,
                        after,
                    },
                    clipboard,
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, clipboard, updated, errors)) => Message::LibraryClipboardPasteFinished {
                action,
                clipboard,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn restore_library_history_snapshot_task(
    db: Arc<Db>,
    snapshot: LibraryOrganizationSnapshot,
    target_index: usize,
    status: String,
    search_changed_entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.restore_library_organization_snapshot(&snapshot)?;
                if !search_changed_entry_ids.is_empty() {
                    let search_index = SearchIndex::open_default()?;
                    let changed_ids = search_changed_entry_ids
                        .iter()
                        .map(|entry_id| entry_id.as_str())
                        .collect::<std::collections::HashSet<_>>();
                    let active_entries = db
                        .get_all_entries()?
                        .into_iter()
                        .filter(|entry| changed_ids.contains(entry.id.as_str()))
                        .collect::<Vec<_>>();
                    reindex_entries(&search_index, &active_entries)?;

                    let active_ids = active_entries
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .collect::<std::collections::HashSet<_>>();
                    let deleted_ids = search_changed_entry_ids
                        .iter()
                        .map(EntryId::as_str)
                        .filter(|entry_id| !active_ids.contains(entry_id))
                        .collect::<Vec<_>>();
                    if !deleted_ids.is_empty() {
                        search_index.delete_entries(deleted_ids)?;
                    }
                }
                Ok::<_, anyhow::Error>((target_index, status))
            })
            .await?
        },
        |result| match result {
            Ok((target_index, status)) => Message::LibraryHistoryRestoreFinished {
                target_index,
                status,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn edit_metadata_task(
    db: Arc<Db>,
    entry: LibraryEntry,
    display_title: String,
    display_author: String,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.update_display_metadata(&entry.id, Some(&display_title), Some(&display_author))?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Edit Metadata"),
                    refresh_search_on_restore: before.search_state_differs_from(&after),
                    before,
                    after,
                };
                let label = format!("Saved metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), action, label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, action, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                action,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn reset_metadata_task(db: Arc<Db>, entry: LibraryEntry) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.reset_display_metadata(&entry.id)?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Reset Metadata"),
                    refresh_search_on_restore: before.search_state_differs_from(&after),
                    before,
                    after,
                };
                let label = format!("Reset metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), action, label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, action, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                action,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn relink_entry_task(db: Arc<Db>, entry_id: EntryId, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking({
                let entry_id = entry_id.clone();
                let path = path.clone();
                move || {
                    db.relink_entry_path(&entry_id, &path)?;
                    Ok::<_, anyhow::Error>((entry_id, path))
                }
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, path)) => Message::RelinkFinished { entry_id, path },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_reset_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let mut reset_entries = Vec::new();
                for mut entry in entries {
                    entry.display_title = None;
                    entry.display_author = None;
                    entry.metadata_locked = false;
                    match db.reset_display_metadata(&entry.id) {
                        Ok(()) => reset_entries.push(entry),
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                let (updated, reindex_errors) =
                    reindex_entries_collecting_errors(&search_index, &reset_entries);
                errors.extend(reindex_errors);
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Reset Metadata"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Reset metadata for"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_refresh_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let mut refreshed_entries = Vec::new();
                for mut entry in entries {
                    match refresh_entry_metadata(&db, &mut entry) {
                        Ok(()) => refreshed_entries.push(entry),
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                let (updated, reindex_errors) =
                    reindex_entries_collecting_errors(&search_index, &refreshed_entries);
                errors.extend(reindex_errors);
                Ok::<_, anyhow::Error>((String::from("Refreshed metadata for"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_delete_metadata_task(db: Arc<Db>, entry_ids: Vec<EntryId>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let updated = entry_ids.len();
                if let Err(error) = db.trash_entries(entry_ids.iter()) {
                    errors.push(error.to_string());
                } else if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }
                if !errors.is_empty() {
                    for entry_id in &entry_ids {
                        tracing::debug!(
                            entry_id = entry_id.as_str(),
                            "Bulk delete entry was part of a failed batch"
                        );
                    }
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Move to Trash"),
                    refresh_search_on_restore: before.trash_state_differs_from(&after),
                    before,
                    after,
                };
                Ok::<_, anyhow::Error>((action, String::from("Moved to trash"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_restore_trash_items_task(
    db: Arc<Db>,
    entries: Vec<LibraryEntry>,
    folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut entries_to_reindex = entries;
                let mut updated = entries_to_reindex.len();
                let restoring_folder = folder_id.is_some();

                if let Some(folder_id) = folder_id {
                    let folders = db.get_trashed_folders()?;
                    let folder_ids = folder_subtree_ids(&folders, &folder_id);
                    let trashed_entries = db.get_trashed_entries()?;
                    for entry in trashed_entries {
                        let in_restored_folder = entry
                            .folders
                            .iter()
                            .any(|folder| folder_ids.contains(&folder.id));
                        let already_selected = entries_to_reindex
                            .iter()
                            .any(|selected| selected.id == entry.id);
                        if in_restored_folder && !already_selected {
                            entries_to_reindex.push(entry);
                        }
                    }
                    updated += db.restore_folder_tree(&folder_id)?;
                }

                let entry_ids = entries_to_reindex
                    .iter()
                    .map(|entry| &entry.id)
                    .collect::<Vec<_>>();
                db.restore_entries(entry_ids.iter().copied())?;

                let (reindexed, errors) =
                    reindex_entries_collecting_errors(&search_index, &entries_to_reindex);
                if !restoring_folder {
                    updated = reindexed;
                }

                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Restore from Trash"),
                    refresh_search_on_restore: before.trash_state_differs_from(&after),
                    before,
                    after,
                };
                Ok::<_, anyhow::Error>((action, String::from("Restored"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_permanently_delete_entries_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let updated = entry_ids.len();
                if let Err(error) = db.delete_entries(entry_ids.iter()) {
                    errors.push(error.to_string());
                } else if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }

                Ok::<_, anyhow::Error>((String::from("Permanently deleted"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn permanently_delete_folder_from_trash_task(
    db: Arc<Db>,
    folder_id: FolderId,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let (updated, entry_ids) = db.permanently_delete_trashed_folder_tree(&folder_id)?;
                if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }
                Ok::<_, anyhow::Error>((updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((updated, errors)) => Message::TrashFolderPermanentlyDeleted { updated, errors },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_reindex_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let (updated, errors) = reindex_entries_collecting_errors(&search_index, &entries);
                Ok::<_, anyhow::Error>((String::from("Reindexed"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn export_library_entries_task(
    entries: Vec<LibraryEntry>,
    dialog: LibraryExportDialog,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || export_library_entries(entries, dialog)).await?
        },
        |result| Message::ExportFinished(result.map_err(|error| error.to_string())),
    )
}

fn export_library_entries(
    entries: Vec<LibraryEntry>,
    dialog: LibraryExportDialog,
) -> anyhow::Result<LibraryExportSummary> {
    let destination = dialog
        .destination
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Choose an export destination folder."))?;
    std::fs::create_dir_all(&destination)?;

    if dialog.mode == ExportMode::Zip {
        return export_library_entries_zip(entries, dialog, destination);
    }

    let mut exported = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();
    let mut metadata_rows = Vec::new();

    for entry in &entries {
        let target_dir = match dialog.mode {
            ExportMode::CopyFlat | ExportMode::Zip => destination.clone(),
            ExportMode::PreserveFolders => {
                let mut folder_dir = destination.clone();
                if entry.folders.is_empty() {
                    folder_dir.push("Unfiled");
                } else {
                    for folder in &entry.folders {
                        folder_dir.push(sanitize_filename(&folder.name));
                    }
                }
                folder_dir
            }
        };
        if let Err(error) = std::fs::create_dir_all(&target_dir) {
            errors.push(format!("{}: {error}", entry_title(entry)));
            continue;
        }
        let filename = export_filename(entry, dialog.filename_template);
        let target_path = match export_target_path(&target_dir, &filename, dialog.conflict_behavior)
        {
            Ok(Some(path)) => path,
            Ok(None) => {
                skipped += 1;
                continue;
            }
            Err(error) => {
                errors.push(format!("{}: {error}", entry_title(entry)));
                continue;
            }
        };
        match std::fs::copy(&entry.path, &target_path) {
            Ok(_) => {
                exported += 1;
                metadata_rows.push((entry.clone(), target_path));
            }
            Err(error) => errors.push(format!("{}: {error}", entry.path.display())),
        }
    }

    if dialog.include_metadata_csv {
        if let Err(error) = write_export_metadata_csv(
            &destination.join("metadata.csv"),
            &metadata_rows,
            dialog.include_tags,
            dialog.include_reading_progress,
        ) {
            errors.push(format!("metadata.csv: {error}"));
        }
    }
    if dialog.include_metadata_json {
        if let Err(error) = write_export_metadata_json(
            &destination.join("metadata.json"),
            &metadata_rows,
            dialog.include_tags,
            dialog.include_reading_progress,
        ) {
            errors.push(format!("metadata.json: {error}"));
        }
    }

    Ok(LibraryExportSummary {
        destination,
        exported,
        skipped,
        errors,
    })
}

fn export_library_entries_zip(
    entries: Vec<LibraryEntry>,
    dialog: LibraryExportDialog,
    destination: PathBuf,
) -> anyhow::Result<LibraryExportSummary> {
    let zip_path = export_target_path(
        &destination,
        "pdf-folio-export.zip",
        dialog.conflict_behavior,
    )?
    .ok_or_else(|| anyhow::anyhow!("Export ZIP already exists."))?;
    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut exported = 0;
    let skipped = 0;
    let mut errors = Vec::new();
    let mut metadata_rows = Vec::new();
    let mut used_names = HashSet::new();

    for entry in &entries {
        let filename = export_filename(entry, dialog.filename_template);
        let mut archive_name = archive_path_for_entry(entry, &filename);
        archive_name = unique_archive_name(archive_name, &mut used_names);
        match std::fs::read(&entry.path) {
            Ok(bytes) => {
                if let Err(error) = zip.start_file(&archive_name, options) {
                    errors.push(format!("{}: {error}", entry_title(entry)));
                    continue;
                }
                if let Err(error) = zip.write_all(&bytes) {
                    errors.push(format!("{}: {error}", entry_title(entry)));
                    continue;
                }
                exported += 1;
                metadata_rows.push((entry.clone(), PathBuf::from(archive_name)));
            }
            Err(error) => errors.push(format!("{}: {error}", entry.path.display())),
        }
    }

    if dialog.include_metadata_csv {
        let csv = export_metadata_csv_string(
            &metadata_rows,
            dialog.include_tags,
            dialog.include_reading_progress,
        );
        zip.start_file("metadata.csv", options)?;
        zip.write_all(csv.as_bytes())?;
    }
    if dialog.include_metadata_json {
        let json = export_metadata_json_bytes(
            &metadata_rows,
            dialog.include_tags,
            dialog.include_reading_progress,
        )?;
        zip.start_file("metadata.json", options)?;
        zip.write_all(&json)?;
    }
    zip.finish()?;

    Ok(LibraryExportSummary {
        destination: zip_path,
        exported,
        skipped,
        errors,
    })
}

fn export_target_path(
    target_dir: &Path,
    filename: &str,
    conflict_behavior: ExportConflictBehavior,
) -> anyhow::Result<Option<PathBuf>> {
    let mut path = target_dir.join(filename);
    if !path.exists() {
        return Ok(Some(path));
    }
    match conflict_behavior {
        ExportConflictBehavior::Skip => Ok(None),
        ExportConflictBehavior::Overwrite => Ok(Some(path)),
        ExportConflictBehavior::KeepBoth => {
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("document")
                .to_owned();
            let extension = path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(ToOwned::to_owned);
            for index in 2..10_000 {
                let candidate = match extension.as_deref() {
                    Some(extension) => format!("{stem} ({index}).{extension}"),
                    None => format!("{stem} ({index})"),
                };
                path = target_dir.join(candidate);
                if !path.exists() {
                    return Ok(Some(path));
                }
            }
            anyhow::bail!("Could not find an available filename for {filename}.")
        }
    }
}

fn export_filename(entry: &LibraryEntry, template: ExportFilenameTemplate) -> String {
    let original = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("document.pdf");
    if template == ExportFilenameTemplate::OriginalFilename {
        return sanitize_pdf_filename(original);
    }
    let title = entry_title(entry);
    let author = entry_author(entry);
    let year = entry.added_at.format("%Y").to_string();
    let raw = match template {
        ExportFilenameTemplate::OriginalFilename => original.to_owned(),
        ExportFilenameTemplate::Title => title,
        ExportFilenameTemplate::AuthorTitle => format!("{author} - {title}"),
        ExportFilenameTemplate::YearAuthorTitle => format!("{year} - {author} - {title}"),
    };
    sanitize_pdf_filename(&raw)
}

fn sanitize_pdf_filename(value: &str) -> String {
    let stem = value.strip_suffix(".pdf").unwrap_or(value);
    let stem = sanitize_filename(stem);
    if stem.is_empty() {
        String::from("document.pdf")
    } else {
        format!("{stem}.pdf")
    }
}

fn sanitize_filename(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches('.')
        .trim()
        .to_owned()
}

fn archive_path_for_entry(entry: &LibraryEntry, filename: &str) -> String {
    let mut parts = if entry.folders.is_empty() {
        vec![String::from("Unfiled")]
    } else {
        entry
            .folders
            .iter()
            .map(|folder| sanitize_filename(&folder.name))
            .collect::<Vec<_>>()
    };
    parts.push(filename.to_owned());
    parts.join("/")
}

fn unique_archive_name(mut name: String, used_names: &mut HashSet<String>) -> String {
    if used_names.insert(name.clone()) {
        return name;
    }
    let path = Path::new(&name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document")
        .to_owned();
    let parent = path
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    let parent = parent.to_owned();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("pdf")
        .to_owned();
    for index in 2..10_000 {
        name = if parent.is_empty() {
            format!("{stem} ({index}).{extension}")
        } else {
            format!("{parent}/{stem} ({index}).{extension}")
        };
        if used_names.insert(name.clone()) {
            return name;
        }
    }
    name
}

fn write_export_metadata_csv(
    path: &Path,
    rows: &[(LibraryEntry, PathBuf)],
    include_tags: bool,
    include_reading_progress: bool,
) -> anyhow::Result<()> {
    let csv = export_metadata_csv_string(rows, include_tags, include_reading_progress);
    std::fs::write(path, csv)?;
    Ok(())
}

fn export_metadata_csv_string(
    rows: &[(LibraryEntry, PathBuf)],
    include_tags: bool,
    include_reading_progress: bool,
) -> String {
    let mut csv = String::from("file,title,author,pages,source_path");
    if include_tags {
        csv.push_str(",tags");
    }
    if include_reading_progress {
        csv.push_str(",last_page,progress_percent");
    }
    csv.push('\n');
    for (entry, exported_path) in rows {
        csv.push_str(&csv_cell(&exported_path.display().to_string()));
        csv.push(',');
        csv.push_str(&csv_cell(&entry_title(entry)));
        csv.push(',');
        csv.push_str(&csv_cell(&entry_author(entry)));
        csv.push(',');
        csv.push_str(
            &entry
                .page_count
                .map_or(String::new(), |pages| pages.to_string()),
        );
        csv.push(',');
        csv.push_str(&csv_cell(&entry.path.display().to_string()));
        if include_tags {
            csv.push(',');
            csv.push_str(&csv_cell(&entry.tags.join("; ")));
        }
        if include_reading_progress {
            csv.push(',');
            csv.push_str(&(u32::from(entry.last_page) + 1).to_string());
            csv.push(',');
            let progress = entry.page_count.map_or(0.0, |page_count| {
                (f32::from(entry.last_page.saturating_add(1)) / f32::from(page_count.max(1)))
                    * 100.0
            });
            csv.push_str(&format!("{progress:.0}"));
        }
        csv.push('\n');
    }
    csv
}

fn csv_cell(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn write_export_metadata_json(
    path: &Path,
    rows: &[(LibraryEntry, PathBuf)],
    include_tags: bool,
    include_reading_progress: bool,
) -> anyhow::Result<()> {
    std::fs::write(
        path,
        export_metadata_json_bytes(rows, include_tags, include_reading_progress)?,
    )?;
    Ok(())
}

fn export_metadata_json_bytes(
    rows: &[(LibraryEntry, PathBuf)],
    include_tags: bool,
    include_reading_progress: bool,
) -> anyhow::Result<Vec<u8>> {
    let items = rows
        .iter()
        .map(|(entry, exported_path)| {
            let mut item = serde_json::json!({
                "file": exported_path,
                "title": entry_title(entry),
                "author": entry_author(entry),
                "pages": entry.page_count,
                "source_path": entry.path,
            });
            if include_tags {
                item["tags"] = serde_json::json!(entry.tags);
            }
            if include_reading_progress {
                item["last_page"] = serde_json::json!(u32::from(entry.last_page) + 1);
                item["progress_percent"] =
                    serde_json::json!(entry.page_count.map_or(0.0, |page_count| {
                        (f32::from(entry.last_page.saturating_add(1))
                            / f32::from(page_count.max(1)))
                            * 100.0
                    }));
            }
            item
        })
        .collect::<Vec<_>>();
    Ok(serde_json::to_vec_pretty(&items)?)
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

pub(crate) fn attribute_pending_metadata_task(db: Arc<Db>) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || attribute_pending_metadata(&db)).await? },
        |result| match result {
            Ok(()) => Message::AuthorAttributionFinished,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) async fn search_library_task(
    db: Arc<Db>,
    query: String,
    sort_mode: LibrarySortMode,
    trash_view_active: bool,
) -> anyhow::Result<(Vec<LibraryEntry>, HashMap<EntryId, u16>)> {
    tokio::task::spawn_blocking(move || {
        let entries = if trash_view_active {
            db.get_trashed_entries()?
        } else {
            db.get_entries_sorted(sort_mode)?
        };
        let normalized = query.trim().to_lowercase();
        let search_index = SearchIndex::open_default()?;
        let hits = if trash_view_active {
            Vec::new()
        } else {
            search_index.search(&query, 200).unwrap_or_default()
        };
        let mut hit_pages = HashMap::new();
        let mut ordered_entries = Vec::new();

        for hit in hits {
            let id = EntryId::new(hit.id);
            if hit_pages.contains_key(&id) {
                continue;
            }
            hit_pages.insert(id.clone(), hit.page.min(u64::from(u16::MAX)) as u16);
            if let Some(entry) = entries.iter().find(|entry| entry.id == id) {
                ordered_entries.push(entry.clone());
            }
        }

        for entry in entries {
            if hit_pages.contains_key(&entry.id) || !entry_matches_query(&entry, &normalized) {
                continue;
            }
            ordered_entries.push(entry);
        }

        Ok((ordered_entries, hit_pages))
    })
    .await?
}

pub(crate) fn import_folder_with_index(db: &Db, root: &Path) -> anyhow::Result<ImportSummary> {
    let paths = scan_pdf_files(root)?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        match import_pdf_with_index(db, path.clone()) {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    Ok(ImportSummary { entries, errors })
}

pub(crate) fn apply_watch_event(db: &Db, event: LibraryWatchEvent) -> anyhow::Result<()> {
    match event {
        LibraryWatchEvent::PdfCreated(path) => {
            if path.exists() {
                import_pdf_with_index(db, path)?;
            }
        }
        LibraryWatchEvent::PdfRemoved(path) => {
            db.set_missing_by_path(&path, true)?;
        }
    }
    Ok(())
}

fn refresh_entry_metadata(db: &Db, entry: &mut LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let author = attributed_author(&doc);
    let page_count = Some(doc.page_count());
    db.update_author_attribution(&entry.id, author.as_deref())?;
    db.update_page_count_attribution(&entry.id, page_count)?;
    entry.author = author;
    entry.page_count = page_count;
    entry.author_attributed = true;
    entry.page_count_attributed = true;
    Ok(())
}

fn reindex_entry(search_index: &SearchIndex, entry: &LibraryEntry) -> anyhow::Result<()> {
    reindex_entries(search_index, std::slice::from_ref(entry))
}

fn reindex_entries(search_index: &SearchIndex, entries: &[LibraryEntry]) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut documents = Vec::new();
    for entry in entries {
        documents.extend(index_documents_for_entry(entry)?);
    }
    search_index.replace_entries_pages(documents)?;
    Ok(())
}

fn reindex_entries_collecting_errors(
    search_index: &SearchIndex,
    entries: &[LibraryEntry],
) -> (usize, Vec<String>) {
    let mut documents = Vec::new();
    let mut updated = 0;
    let mut errors = Vec::new();

    for entry in entries {
        match index_documents_for_entry(entry) {
            Ok(entry_documents) => {
                documents.extend(entry_documents);
                updated += 1;
            }
            Err(error) => errors.push(format!("{}: {error}", entry_title(entry))),
        }
    }

    if !documents.is_empty() {
        if let Err(error) = search_index.replace_entries_pages(documents) {
            errors.push(format!("search index: {error}"));
            updated = 0;
        }
    }

    (updated, errors)
}

fn index_documents_for_entry(entry: &LibraryEntry) -> anyhow::Result<Vec<IndexDocument>> {
    let doc = PdfDoc::open(&entry.path)?;
    let title = entry_title(entry);
    let author = entry_author(entry);
    let mut documents = Vec::with_capacity(usize::from(doc.page_count()));
    for page in 0..doc.page_count() {
        documents.push(IndexDocument {
            id: entry.id.as_str().to_owned(),
            title: title.clone(),
            author: author.clone(),
            body: doc.text_on_page(page).unwrap_or_default(),
            page: u64::from(page),
        });
    }
    Ok(documents)
}

pub(crate) fn import_pdf_with_index(db: &Db, path: PathBuf) -> anyhow::Result<ImportedEntry> {
    let id = EntryId::new(hash_file(&path)?);
    let inserted = db.entry_by_path(&path)?.is_none();
    let doc = PdfDoc::open(&path)?;
    let title = attributed_title(&doc).or_else(|| title_from_path(&path));
    let page_count = doc.page_count();
    let author = attributed_author(&doc);

    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.clone(),
        title: title.clone(),
        author: author.clone(),
        author_attributed: true,
        page_count_attributed: true,
        page_count: Some(page_count),
        file_size: file_size(&path),
        cover_hash: None,
    })?;
    cache_thumbnail_variants(&id, &doc)?;

    let search_index = SearchIndex::open_default()?;
    let mut documents = Vec::with_capacity(usize::from(page_count));
    for page in 0..page_count {
        let body = doc.text_on_page(page).unwrap_or_default();
        documents.push(IndexDocument {
            id: id.as_str().to_owned(),
            title: title.clone().unwrap_or_default(),
            author: author.clone().unwrap_or_default(),
            body,
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;

    Ok(ImportedEntry { id, path, inserted })
}

fn attribute_pending_metadata(db: &Db) -> anyhow::Result<()> {
    for entry in db.get_all_entries()?.into_iter().filter(|entry| {
        !entry.missing && (!entry.author_attributed || !entry.page_count_attributed)
    }) {
        let doc = open_entry_doc(&entry);
        if !entry.author_attributed {
            let author = doc.as_ref().and_then(attributed_author);
            db.update_author_attribution(&entry.id, author.as_deref())?;
        }
        if !entry.page_count_attributed {
            let page_count = doc.as_ref().map(|doc| doc.page_count());
            db.update_page_count_attribution(&entry.id, page_count)?;
        }
    }

    Ok(())
}

fn open_entry_doc(entry: &LibraryEntry) -> Option<PdfDoc> {
    entry
        .path
        .exists()
        .then(|| PdfDoc::open(&entry.path).ok())
        .flatten()
}

fn attributed_author(doc: &PdfDoc) -> Option<String> {
    doc.metadata_author()
        .ok()
        .flatten()
        .and_then(|author| clean_author_candidate(&author))
        .or_else(|| author_from_contents(doc))
}

fn attributed_title(doc: &PdfDoc) -> Option<String> {
    doc.metadata_title()
        .ok()
        .flatten()
        .and_then(clean_import_title)
}

#[cfg(test)]
pub(crate) fn title_from_path(path: &Path) -> Option<String> {
    title_from_path_inner(path)
}

#[cfg(not(test))]
fn title_from_path(path: &Path) -> Option<String> {
    title_from_path_inner(path)
}

fn title_from_path_inner(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(clean_import_title)
}

pub(crate) fn clean_import_title(value: impl AsRef<str>) -> Option<String> {
    let title = value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() || title.eq_ignore_ascii_case("untitled") {
        None
    } else {
        Some(title.chars().take(512).collect())
    }
}

fn author_from_contents(doc: &PdfDoc) -> Option<String> {
    let pages_to_scan = doc.page_count().min(3);
    for page in 0..pages_to_scan {
        let text = doc.text_on_page(page).ok()?;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(author) = author_from_line(line) {
                return Some(author);
            }
        }
    }
    None
}

fn author_from_line(line: &str) -> Option<String> {
    let normalized = line.trim().trim_matches(['.', ',', ';', ':']);
    for prefix in ["Author:", "Authors:", "By:", "Written by "] {
        if let Some(candidate) = normalized.strip_prefix(prefix) {
            return clean_author_candidate(candidate);
        }
    }

    normalized
        .strip_prefix("By ")
        .and_then(clean_author_candidate)
}

fn clean_author_candidate(candidate: &str) -> Option<String> {
    let candidate = candidate
        .trim()
        .trim_matches(['.', ',', ';', ':', '-', ' '])
        .replace('\n', " ");
    let candidate = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = candidate.to_lowercase();
    let digit_count = candidate.chars().filter(|ch| ch.is_ascii_digit()).count();

    if candidate.len() < 2
        || candidate.len() > 80
        || lower == "anonymous"
        || lower == "unknown"
        || lower.contains("http")
        || lower.contains("www.")
        || lower.contains("copyright")
        || digit_count > 4
    {
        return None;
    }

    Some(candidate)
}
