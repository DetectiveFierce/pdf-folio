//! Filesystem watcher scaffolding for library imports.

use std::path::Path;

use anyhow::Result;

/// Filesystem watcher for configured library folders.
#[derive(Debug, Default)]
pub struct LibraryWatcher;

impl LibraryWatcher {
    /// Creates a new library watcher.
    pub fn new() -> Self {
        Self
    }

    /// Starts watching a directory for PDF changes.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform watcher cannot subscribe to the directory.
    pub fn watch_directory(&mut self, _path: &Path) -> Result<()> {
        Ok(())
    }
}
