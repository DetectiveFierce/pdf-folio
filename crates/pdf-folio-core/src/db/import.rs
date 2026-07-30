//! Library import helpers: hashing, scanning, thumbnails, sources, and watching.
//!
//! Turns filesystem paths into [`crate::LibraryEntry`] rows. Content identity
//! is a BLAKE3 digest of file bytes ([`hash_file`]), stored as
//! [`crate::EntryId`], so the same PDF under a new path merges on re-import.
//! Folder import aggregates per-file failures into [`ImportSummary::errors`]
//! rather than aborting the whole batch.
//!
//! Also owns:
//! - Thumbnail cache path helpers under the XDG cache directory.
//! - External [`crate::ImportSource`] rows (e.g. Raindrop accounts).
//! - [`LibraryWatcher`], a thin `notify` wrapper that emits
//!   [`LibraryWatchEvent`]s for PDF create/modify/remove under watched roots.
//!
//! # See also
//!
//! - [`crate::Db::insert_entry`] for the database write path.
//! - [`crate::pdf::PdfDoc`] for post-import metadata enrichment.
//! - [`super::raindrop`] for remote collection mapping after import.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use blake3::Hasher;
use directories::ProjectDirs;

use super::{Db, EntryId, NewLibraryEntry};

/// Result of importing a single PDF path (always includes the content-hash id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedEntry {
    /// Stable content-derived identifier (BLAKE3 hex of file bytes).
    pub id: EntryId,
    /// Absolute or caller-supplied path written to the library row.
    pub path: PathBuf,
    /// `true` when no row existed for this path before the upsert.
    pub inserted: bool,
}

/// Aggregate result of [`import_folder`]: successes plus per-path error messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    /// Successfully hashed/upserted PDFs (includes already-known files).
    pub entries: Vec<ImportedEntry>,
    /// Non-fatal per-file failures (`"{path}: {error}"`); does not abort the batch.
    pub errors: Vec<String>,
}

/// Recursively scans a directory for PDF files.
///
/// # Errors
///
/// Returns an error when the root directory cannot be read.
pub fn scan_pdf_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    scan_pdf_files_into(root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Imports all PDFs below a folder.
///
/// # Errors
///
/// Returns an error when the root directory cannot be scanned.
pub fn import_folder(db: &Db, root: &Path) -> Result<ImportSummary> {
    let paths = scan_pdf_files(root)?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        match import_pdf(db, &path) {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    Ok(ImportSummary { entries, errors })
}

/// Imports a single PDF path by content hash.
///
/// # Errors
///
/// Returns an error when the file cannot be hashed or the database write fails.
pub fn import_pdf(db: &Db, path: &Path) -> Result<ImportedEntry> {
    let id = EntryId::new(hash_file(path)?);
    let inserted = db.entry_by_path(path)?.is_none();
    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.to_path_buf(),
        title: title_from_path(path),
        author: None,
        author_attributed: false,
        page_count_attributed: false,
        page_count: None,
        file_size: file_size(path),
        cover_hash: None,
    })?;

    Ok(ImportedEntry {
        id,
        path: path.to_path_buf(),
        inserted,
    })
}

fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

/// Returns the thumbnail cache directory.
///
/// # Errors
///
/// Returns an error when an XDG cache directory cannot be resolved or created.
pub fn thumbnail_cache_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a cache directory for PDF-Folio.")?;
    let dir = project_dirs.cache_dir().join("thumbs");
    fs::create_dir_all(&dir)
        .with_context(|| format!("Could not create thumbnail cache: {}.", dir.display()))?;
    Ok(dir)
}

/// Returns the raw RGBA thumbnail path for an entry.
///
/// # Errors
///
/// Returns an error when the thumbnail cache directory cannot be created.
pub fn thumbnail_path(entry_id: &EntryId) -> Result<PathBuf> {
    Ok(thumbnail_cache_dir()?.join(format!("{}.rgba", entry_id.as_str())))
}

/// Streams `path` through BLAKE3 and returns the lowercase hex digest.
///
/// Used as the stable [`EntryId`] for library rows so identical file bytes
/// map to the same entry across paths and machines.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or read.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Could not open file: {}.", path.display()))?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("Could not read file: {}.", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn scan_pdf_files_into(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)
        .with_context(|| format!("Could not read import folder: {}.", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            scan_pdf_files_into(&path, files)?;
        } else if file_type.is_file() && is_pdf_path(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

fn title_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(clean_import_title)
}

fn clean_import_title(value: impl AsRef<str>) -> Option<String> {
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

// --- External import sources -------------------------------------------------

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

use super::ImportSource;

impl Db {
    /// Creates or updates an external import source.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the source.
    pub fn upsert_import_source(
        &self,
        id: &str,
        kind: &str,
        account_id: Option<&str>,
        display_name: Option<&str>,
    ) -> Result<ImportSource> {
        let connection = self.connection()?;
        let now = Utc::now().timestamp();
        connection.execute(
            "INSERT INTO import_sources
                (id, kind, account_id, display_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                account_id = excluded.account_id,
                display_name = excluded.display_name,
                updated_at = excluded.updated_at",
            params![id, kind, account_id, display_name, now],
        )?;
        self.import_source(id)?
            .with_context(|| format!("Import source {id} was not saved."))
    }

    /// Returns one import source.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query the source.
    pub fn import_source(&self, id: &str) -> Result<Option<ImportSource>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT id, kind, account_id, display_name, created_at, updated_at
                 FROM import_sources
                 WHERE id = ?1",
                params![id],
                row_to_import_source,
            )
            .optional()
            .context("Could not load import source.")
    }
}

fn row_to_import_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImportSource> {
    let created_at: i64 = row.get(4)?;
    let updated_at: i64 = row.get(5)?;
    Ok(ImportSource {
        id: row.get(0)?,
        kind: row.get(1)?,
        account_id: row.get(2)?,
        display_name: row.get(3)?,
        created_at: DateTime::from_timestamp(created_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        updated_at: DateTime::from_timestamp(updated_at, 0).unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
    })
}

// --- Filesystem watching -----------------------------------------------------
//
// `LibraryWatcher` lives in the import module because both are concerned with
// PDFs entering the library: the import helpers do the initial scan/import, and
// the watcher reacts to filesystem changes in already-imported folders.

use std::sync::mpsc::Sender;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// High-level PDF file event emitted by [`LibraryWatcher`] (non-PDF paths are ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryWatchEvent {
    /// A PDF file was created or modified under a watched root.
    PdfCreated(PathBuf),
    /// A PDF file was removed under a watched root.
    PdfRemoved(PathBuf),
}

/// Filesystem watcher that translates `notify` events into [`LibraryWatchEvent`]s.
///
/// Only paths with a `.pdf` extension (case-insensitive) are forwarded. The
/// caller owns the `mpsc` channel and should import/remove library rows in
/// response. Dropping this value stops watching.
#[derive(Debug)]
pub struct LibraryWatcher {
    watcher: RecommendedWatcher,
}

impl LibraryWatcher {
    /// Creates a watcher that sends events on `sender` (errors from notify are dropped).
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

    /// Recursively watches `path` for create/modify/remove events on PDF files.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform watcher cannot subscribe to the directory.
    pub fn watch_directory(&mut self, path: &Path) -> Result<()> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
