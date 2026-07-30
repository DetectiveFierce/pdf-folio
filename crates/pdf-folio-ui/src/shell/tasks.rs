use crate::library::registry::{
    load_library_preview, sync_library_registry_profiles, sync_library_rows_for_registry,
    LibraryProfile,
};
use crate::*;
use anyhow::Context;

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
