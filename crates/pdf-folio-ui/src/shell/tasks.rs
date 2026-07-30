//! Shell-owned async tasks (registry sync fan-out and related background work).
//!
//! Library-domain tasks (import, bulk ops, thumbnails) live under
//! `library::tasks`. Viewer open/render tasks live under `viewer::tasks`.
//! This module covers cross-library cloud work that the shell schedules from
//! auto-sync ticks, sign-in completion, and remote-available signals.
//!
//! # Key constructors
//!
//! - [`sync_library_registry_task`] / [`sync_library_registry_for_app_task`] —
//!   pull/push multi-library registry CRDT state.
//! - [`auto_sync_task`] — one library CRDT sync pass.
//! - Preview / fan-out helpers used when the switcher or remote watcher needs
//!   multiple libraries updated.
//!
//! Tasks emit `Message::LibraryRegistrySyncFinished`, `AutoSyncFinished`, or
//! library preview messages handled by shell or library update.

use crate::library::registry::{
    load_library_preview, sync_library_registry_profiles, sync_library_rows_for_registry,
    LibraryProfile,
};
use crate::*;
use anyhow::Context;

/// Syncs the multi-library registry with the remote CRDT log.
///
/// When `push_local` is true, local profiles are uploaded before merge. On
/// completion emits [`Message::LibraryRegistrySyncFinished`]; if
/// `sync_all_after` is set, shell update then queues per-library auto-sync.
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
            let rows = if push_local { sync_library_rows_for_registry(&registry) } else { Default::default() };
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

/// Convenience wrapper: registry sync using the active library profile's DB path.
///
/// Returns `Task::none()` when no active profile is available.
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

/// Runs one CRDT sync pass for `library`, emitting [`Message::AutoSyncFinished`].
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

/// Loads switcher card preview stats for `profile` on a blocking thread.
///
/// Emits [`Message::LibraryPreviewRefreshed`] or [`Message::LibraryError`].
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

/// Looks up a registry profile by id and refreshes its switcher preview.
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

/// Stable-ish device id for CRDT sync (hostname, or `"local-device"` fallback).
pub(crate) fn default_sync_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

/// Queues the active library for auto-sync when the user is signed in.
pub(crate) fn start_auto_sync_now(app: &mut PDFolioApp) -> Task<Message> {
    if app.sync_auth.is_signed_in() {
        app.sync_queued_libraries
            .insert(app.libraries.active_library_id.clone());
    }
    Task::none()
}

/// Queues every known library profile for auto-sync when signed in.
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

/// Starts sync for `library_id`, or queues it if another sync is in progress.
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

/// Pops the next queued library id and starts its auto-sync task, if any.
pub(crate) fn start_next_queued_sync(app: &mut PDFolioApp) -> Task<Message> {
    let Some(library_id) = app.sync_queued_libraries.iter().next().cloned() else {
        return Task::none();
    };
    auto_sync_library_task(app, library_id)
}
