//! Application subscriptions and filesystem watcher streams.

use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use iced::futures::SinkExt;
use iced::{event, mouse, stream, time, Event, Subscription};
use notify::{EventKind, RecursiveMode, Watcher};
use pdf_folio_db::{Db, LibraryWatcher};

use super::{shortcuts, AppMode, PDFolioApp, LIBRARY_CARD_HOVER_TICK_MS, VIEWER_ANIMATION_TICK_MS};
use crate::library::drag::LIBRARY_DRAG_AUTOSCROLL_TICK_MS;
use crate::messages::Message;

const AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(10);
const LIVE_SYNC_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) fn subscription(app: &PDFolioApp) -> Subscription<Message> {
    let keyboard = event::listen_with(|event, status, _window| {
        shortcuts::keyboard_event_message(event, status)
    });
    let cursor = if app.mode == AppMode::Library {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let watcher = if app.settings.watch_directories.is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.settings.watch_directories.clone(),
            watch_directories_stream,
        )
    };

    let style_watcher = if app.appearance.style_book.style_dirs().is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.appearance.style_book.style_dirs().to_vec(),
            watch_style_directories_stream,
        )
    };

    let sidebar_resize = if app.library.resizing_library_tag_sidebar {
        event::listen_with(|event, _status, _window| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::TagSidebarResizeDragged(position.x))
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::EndTagSidebarResize)
            }
            _ => None,
        })
    } else {
        Subscription::none()
    };

    let library_drag = if app.library.library_drag.is_some() {
        Subscription::batch([
            event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::LibraryEntryDragMoved(position))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::EndLibraryEntryDrag)
                }
                _ => None,
            }),
            time::every(Duration::from_millis(LIBRARY_DRAG_AUTOSCROLL_TICK_MS))
                .map(Message::LibraryAutoScrollTick),
        ])
    } else {
        Subscription::none()
    };

    let folder_drag = if app.library.folder_drag.is_some() {
        Subscription::batch([
            event::listen_with(|event, _status, _window| match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::FolderDragMoved(position))
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::EndFolderDrag)
                }
                _ => None,
            }),
            time::every(Duration::from_millis(LIBRARY_DRAG_AUTOSCROLL_TICK_MS))
                .map(Message::LibraryAutoScrollTick),
        ])
    } else {
        Subscription::none()
    };

    let animations = if app.mode == AppMode::Library
        && (app.library_card_hover_animation_active()
            || app.library.bulk_operation_progress.is_some()
            || app.library.library_history_restore_started_at.is_some()
            || app.library.folder_drop_flash.is_some())
    {
        time::every(Duration::from_millis(LIBRARY_CARD_HOVER_TICK_MS)).map(Message::AnimationFrame)
    } else if app.mode == AppMode::Viewer
        && (app.viewer.pending_document_open
            || app.viewer.zoom_preview_width_px.is_some()
            || app.viewer_page_fade_active())
    {
        time::every(Duration::from_millis(VIEWER_ANIMATION_TICK_MS)).map(Message::AnimationFrame)
    } else {
        Subscription::none()
    };

    let auto_sync = if app.sync_auth.is_signed_in() && !app.sync_in_progress {
        time::every(AUTO_SYNC_INTERVAL).map(Message::AutoSyncTick)
    } else {
        Subscription::none()
    };

    let live_sync = if app.sync_auth.is_signed_in() && !app.sync_in_progress {
        app.libraries
            .active_profile()
            .map_or_else(Subscription::none, |profile| {
                Subscription::run_with(
                    LiveSyncWatch {
                        db_path: profile.db_path.clone(),
                        library_id: profile.id.clone(),
                        device_id: default_sync_device_id(),
                    },
                    live_sync_stream,
                )
            })
    } else {
        Subscription::none()
    };

    Subscription::batch([
        keyboard,
        cursor,
        watcher,
        style_watcher,
        sidebar_resize,
        library_drag,
        folder_drag,
        animations,
        auto_sync,
        live_sync,
    ])
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LiveSyncWatch {
    db_path: PathBuf,
    library_id: String,
    device_id: String,
}

fn live_sync_stream(watch: &LiveSyncWatch) -> impl iced::futures::Stream<Item = Message> {
    let watch = watch.clone();
    stream::channel(10, async move |mut output| {
        let Ok(db) = Db::open(&watch.db_path) else {
            return;
        };
        let Ok(session) = pdf_folio_sync::cached_session() else {
            return;
        };
        let client = pdf_folio_sync::SyncClient::new(session);
        loop {
            tokio::time::sleep(LIVE_SYNC_INTERVAL).await;
            let Ok(cursor) = db.sync_crdt_remote_cursor(&watch.library_id, &watch.device_id) else {
                continue;
            };
            let Ok(remote_sequence) = client.remote_crdt_head_sequence(&watch.library_id).await
            else {
                continue;
            };
            if remote_sequence > cursor
                && output
                    .send(Message::RemoteSyncAvailable {
                        noticed_at: Instant::now(),
                        remote_sequence,
                    })
                    .await
                    .is_err()
            {
                break;
            }
        }
    })
}

fn default_sync_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

fn watch_style_directories_stream(
    paths: &Vec<PathBuf>,
) -> impl iced::futures::Stream<Item = Message> {
    let paths = paths.clone();
    stream::channel(20, async move |mut output| {
        if paths.iter().all(|path| !path.exists()) {
            return;
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(
            move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };

                if style_watch_event_should_reload(&event) {
                    let _ = sender.send(());
                }
            },
        ) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "Could not create style filesystem watcher; falling back to polling");
                None
            }
        };

        if let Some(watcher) = watcher.as_mut() {
            for path in paths.iter().filter(|path| path.exists()) {
                if let Err(error) = watcher.watch(path, RecursiveMode::Recursive) {
                    tracing::warn!(
                        path = %path.display(),
                        %error,
                        "Could not watch style directory; polling will still detect changes"
                    );
                }
            }
        }

        let receiver = Arc::new(std::sync::Mutex::new(receiver));
        let mut snapshot = style_files_snapshot(&paths);
        loop {
            let receiver = Arc::clone(&receiver);
            let event = tokio::task::spawn_blocking(move || {
                receiver
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv_timeout(Duration::from_millis(500))
            })
            .await;

            let next_snapshot = style_files_snapshot(&paths);
            let notify_changed = matches!(event, Ok(Ok(())));
            let poll_changed = next_snapshot != snapshot;

            if notify_changed || poll_changed {
                snapshot = next_snapshot;
                tokio::time::sleep(Duration::from_millis(75)).await;
                if output.send(Message::ReloadStyles).await.is_err() {
                    break;
                }
            } else if matches!(event, Ok(Err(RecvTimeoutError::Disconnected)) | Err(_)) {
                break;
            }
        }
    })
}

pub(crate) fn style_watch_event_should_reload(event: &notify::Event) -> bool {
    if !matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return false;
    }

    event.paths.is_empty()
        || event.paths.iter().any(|path| {
            path.is_dir()
                || path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        })
}

fn style_files_snapshot(paths: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut files = Vec::new();
    for path in paths {
        collect_style_files(path, &mut files);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files.dedup_by(|left, right| left.0 == right.0);
    files
}

fn collect_style_files(
    path: &std::path::Path,
    files: &mut Vec<(PathBuf, Option<SystemTime>, u64)>,
) {
    if path.is_file() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        {
            let metadata = std::fs::metadata(path).ok();
            files.push((
                path.to_path_buf(),
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.modified().ok()),
                metadata.as_ref().map_or(0, std::fs::Metadata::len),
            ));
        }
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        collect_style_files(&entry.path(), files);
    }
}

fn watch_directories_stream(paths: &Vec<PathBuf>) -> impl iced::futures::Stream<Item = Message> {
    let paths = paths.clone();
    stream::channel(100, async move |mut output| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = match LibraryWatcher::new(sender) {
            Ok(watcher) => watcher,
            Err(error) => {
                let _ = output.send(Message::LibraryError(error.to_string())).await;
                return;
            }
        };

        for path in &paths {
            if let Err(error) = watcher.watch_directory(path) {
                let _ = output
                    .send(Message::LibraryError(format!(
                        "Could not watch {}: {error}",
                        path.display()
                    )))
                    .await;
            }
        }

        let receiver = Arc::new(std::sync::Mutex::new(receiver));
        loop {
            let receiver = Arc::clone(&receiver);
            let event = tokio::task::spawn_blocking(move || {
                receiver
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .recv()
            })
            .await;

            let Ok(Ok(event)) = event else {
                break;
            };

            if output
                .send(Message::LibraryWatchEvent(event))
                .await
                .is_err()
            {
                break;
            }
        }

        drop(watcher);
    })
}
