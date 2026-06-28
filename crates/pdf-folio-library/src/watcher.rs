//! Filesystem watcher for library imports.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// High-level PDF file event emitted by the library watcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryWatchEvent {
    /// A PDF file was created or modified.
    PdfCreated(PathBuf),
    /// A PDF file was removed.
    PdfRemoved(PathBuf),
}

/// Filesystem watcher for configured library folders.
#[derive(Debug)]
pub struct LibraryWatcher {
    watcher: RecommendedWatcher,
}

impl LibraryWatcher {
    /// Creates a new library watcher.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform watcher cannot be created.
    pub fn new(sender: Sender<LibraryWatchEvent>) -> Result<Self> {
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else {
                return;
            };

            let watch_event = match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => event
                    .paths
                    .into_iter()
                    .find(|path| is_pdf_path(path))
                    .map(LibraryWatchEvent::PdfCreated),
                EventKind::Remove(_) => event
                    .paths
                    .into_iter()
                    .find(|path| is_pdf_path(path))
                    .map(LibraryWatchEvent::PdfRemoved),
                _ => None,
            };

            if let Some(event) = watch_event {
                let _ = sender.send(event);
            }
        })
        .context("Could not create filesystem watcher.")?;

        Ok(Self { watcher })
    }

    /// Starts watching a directory for PDF changes.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform watcher cannot subscribe to the directory.
    pub fn watch_directory(&mut self, _path: &Path) -> Result<()> {
        self.watcher.watch(_path, RecursiveMode::Recursive)?;
        Ok(())
    }
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}
