//! Application subscriptions and filesystem watcher streams.

use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use iced::futures::SinkExt;
use iced::{event, mouse, stream, time, Event, Subscription};
use notify::{EventKind, RecursiveMode, Watcher};
use pdf_folio_library::LibraryWatcher;

use super::{shortcuts, AppMode, PDFolioApp, LIBRARY_CARD_HOVER_TICK_MS};
use crate::library::drag::LIBRARY_DRAG_AUTOSCROLL_TICK_MS;
use crate::messages::Message;

pub(crate) fn subscription(app: &PDFolioApp) -> Subscription<Message> {
    let keyboard = event::listen_with(|event, status, _window| {
        shortcuts::keyboard_event_message(event, status)
    });

    let watcher = if app.settings.watch_directories.is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.settings.watch_directories.clone(),
            watch_directories_stream,
        )
    };

    let style_watcher = if app.style_book.style_dirs().is_empty() {
        Subscription::none()
    } else {
        Subscription::run_with(
            app.style_book.style_dirs().to_vec(),
            watch_style_directories_stream,
        )
    };

    let sidebar_resize = if app.resizing_library_tag_sidebar {
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

    let library_drag = if app.library_drag.is_some() {
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

    let folder_drag = if app.folder_drag.is_some() {
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
            || app.bulk_operation_progress.is_some()
            || app.folder_drop_flash.is_some())
    {
        time::every(Duration::from_millis(LIBRARY_CARD_HOVER_TICK_MS)).map(Message::AnimationFrame)
    } else {
        Subscription::none()
    };

    Subscription::batch([
        keyboard,
        watcher,
        style_watcher,
        sidebar_resize,
        library_drag,
        folder_drag,
        animations,
    ])
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
