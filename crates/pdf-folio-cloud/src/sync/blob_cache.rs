use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Content-addressed local PDF blob cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobCache {
    root: PathBuf,
}

impl BlobCache {
    /// Opens the default blob cache under PDF-Folio's data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform data directory cannot be resolved.
    pub fn open_default() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
            .context("Could not find a data directory for PDF-Folio.")?;
        Ok(Self {
            root: project_dirs.data_dir().join("sync").join("blobs"),
        })
    }

    /// Creates a blob cache rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the cache path for a BLAKE3 hash.
    pub fn path_for_hash(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(0..2).unwrap_or("xx");
        self.root.join(prefix).join(format!("{hash}.pdf"))
    }

    /// Returns true when the cache already has this blob.
    pub fn contains(&self, hash: &str) -> bool {
        self.path_for_hash(hash).is_file()
    }

    /// Root directory for the cache.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
