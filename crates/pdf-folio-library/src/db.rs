//! SQLite database setup and library entry queries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};

/// Stable library entry identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntryId(String);

impl EntryId {
    /// Creates an entry identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A PDF entry stored in the local library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// Stable content-derived identifier.
    pub id: EntryId,
    /// Absolute or user-selected path to the PDF.
    pub path: PathBuf,
    /// Optional document title.
    pub title: Option<String>,
    /// Optional document author.
    pub author: Option<String>,
    /// Timestamp when the entry was added.
    pub added_at: DateTime<Utc>,
    /// Most recent open timestamp.
    pub opened_at: Option<DateTime<Utc>>,
    /// Page count, if known.
    pub page_count: Option<u16>,
    /// Last zero-based page read by the user.
    pub last_page: u16,
    /// User rating from 0 to 5.
    pub rating: u8,
    /// Hash of the cached cover thumbnail bytes.
    pub cover_hash: Option<String>,
    /// User tags attached to the entry.
    pub tags: Vec<String>,
    /// True when the source file disappeared from disk.
    pub missing: bool,
}

/// Input data for creating a library entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewLibraryEntry {
    /// Stable content-derived identifier.
    pub id: EntryId,
    /// Path to the PDF.
    pub path: PathBuf,
    /// Optional document title.
    pub title: Option<String>,
    /// Optional document author.
    pub author: Option<String>,
    /// Page count, if known.
    pub page_count: Option<u16>,
    /// Hash of the cached cover thumbnail bytes.
    pub cover_hash: Option<String>,
}

/// SQLite-backed PDF-Folio library database.
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
}

impl Db {
    /// Opens the default library database under the XDG data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when an XDG project directory cannot be resolved, the database directory
    /// cannot be created, or SQLite cannot open or migrate the database.
    pub fn open_default() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
            .context("Could not find a data directory for PDF-Folio.")?;
        let data_dir = project_dirs.data_dir();
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Could not create data directory: {}.", data_dir.display()))?;
        Self::open(data_dir.join("library.db"))
    }

    /// Opens a library database at `path` and runs migrations.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot open or migrate the database.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db = Self { path: path.into() };
        db.migrate()?;
        Ok(db)
    }

    /// Returns the database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Inserts or replaces a library entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the entry.
    pub fn insert_entry(&self, entry: &NewLibraryEntry) -> Result<()> {
        let connection = self.connection()?;
        let now = Utc::now().timestamp();
        connection.execute(
            "INSERT OR REPLACE INTO entries
                (id, path, title, author, added_at, page_count, cover_hash, missing)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
             ON CONFLICT(id) DO UPDATE SET
                path = excluded.path,
                title = excluded.title,
                author = excluded.author,
                page_count = excluded.page_count,
                cover_hash = COALESCE(excluded.cover_hash, entries.cover_hash),
                missing = 0",
            params![
                entry.id.as_str(),
                entry.path.to_string_lossy(),
                entry.title,
                entry.author,
                now,
                entry.page_count.map(i64::from),
                entry.cover_hash,
            ],
        )?;
        Ok(())
    }

    /// Returns all library entries ordered by most recent addition.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn get_all_entries(&self) -> Result<Vec<LibraryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, added_at, opened_at, page_count, last_page, rating, cover_hash, missing
             FROM entries
             ORDER BY added_at DESC",
        )?;

        let rows = statement.query_map([], |row| {
            let added_at: i64 = row.get(4)?;
            let opened_at: Option<i64> = row.get(5)?;
            let page_count: Option<i64> = row.get(6)?;
            let last_page: i64 = row.get(7)?;
            let rating: i64 = row.get(8)?;
            let id = EntryId::new(row.get::<_, String>(0)?);

            Ok(LibraryEntry {
                id,
                path: PathBuf::from(row.get::<_, String>(1)?),
                title: row.get(2)?,
                author: row.get(3)?,
                added_at: DateTime::from_timestamp(added_at, 0)
                    .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
                opened_at: opened_at.and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
                page_count: page_count.map(|value| value as u16),
                last_page: last_page as u16,
                rating: rating as u8,
                cover_hash: row.get(9)?,
                tags: Vec::new(),
                missing: row.get::<_, i64>(10)? != 0,
            })
        })?;

        let mut entries = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load library entries.")?;
        for entry in &mut entries {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
        }
        Ok(entries)
    }

    /// Updates reading progress for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn update_last_page(&self, entry_id: &EntryId, page: u16) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET last_page = ?1, opened_at = ?2 WHERE id = ?3",
            params![i64::from(page), Utc::now().timestamp(), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Adds a tag to an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the tag.
    pub fn add_tag(&self, entry_id: &EntryId, tag: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO tags (entry_id, tag) VALUES (?1, ?2)",
            params![entry_id.as_str(), tag],
        )?;
        Ok(())
    }

    /// Removes a tag from an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the tag.
    pub fn remove_tag(&self, entry_id: &EntryId, tag: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM tags WHERE entry_id = ?1 AND tag = ?2",
            params![entry_id.as_str(), tag],
        )?;
        Ok(())
    }

    /// Deletes an entry and its dependent rows.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the entry.
    pub fn delete_entry(&self, entry_id: &EntryId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM entries WHERE id = ?1",
            params![entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Marks an entry as missing or present without deleting its metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn set_missing(&self, entry_id: &EntryId, missing: bool) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET missing = ?1 WHERE id = ?2",
            params![i64::from(missing), entry_id.as_str()],
        )?;
        Ok(())
    }

    /// Marks an entry as missing or present by its source path.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the entry.
    pub fn set_missing_by_path(&self, path: &Path, missing: bool) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE entries SET missing = ?1 WHERE path = ?2",
            params![i64::from(missing), path.to_string_lossy()],
        )?;
        Ok(())
    }

    /// Returns the entry with the given path, if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query entries.
    pub fn entry_by_path(&self, path: &Path) -> Result<Option<LibraryEntry>> {
        let connection = self.connection()?;
        let path = path.to_string_lossy();
        let mut statement = connection.prepare(
            "SELECT id, path, title, author, added_at, opened_at, page_count, last_page, rating, cover_hash, missing
             FROM entries
             WHERE path = ?1",
        )?;
        let mut entry = statement
            .query_row(params![path], |row| {
                let added_at: i64 = row.get(4)?;
                let opened_at: Option<i64> = row.get(5)?;
                let page_count: Option<i64> = row.get(6)?;
                let last_page: i64 = row.get(7)?;
                let rating: i64 = row.get(8)?;
                Ok(LibraryEntry {
                    id: EntryId::new(row.get::<_, String>(0)?),
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    title: row.get(2)?,
                    author: row.get(3)?,
                    added_at: DateTime::from_timestamp(added_at, 0)
                        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
                    opened_at: opened_at
                        .and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
                    page_count: page_count.map(|value| value as u16),
                    last_page: last_page as u16,
                    rating: rating as u8,
                    cover_hash: row.get(9)?,
                    tags: Vec::new(),
                    missing: row.get::<_, i64>(10)? != 0,
                })
            })
            .optional()
            .context("Could not load library entry by path.")?;

        if let Some(entry) = &mut entry {
            entry.tags = self.tags_for_entry_with_connection(&connection, &entry.id)?;
        }

        Ok(entry)
    }

    /// Returns all tags currently used in the library.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query tags.
    pub fn all_tags(&self) -> Result<Vec<String>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT DISTINCT tag FROM tags ORDER BY tag")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load library tags.")
    }

    fn tags_for_entry_with_connection(
        &self,
        connection: &Connection,
        entry_id: &EntryId,
    ) -> Result<Vec<String>> {
        let mut statement =
            connection.prepare("SELECT tag FROM tags WHERE entry_id = ?1 ORDER BY tag")?;
        let rows =
            statement.query_map(params![entry_id.as_str()], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load entry tags.")
    }

    fn connection(&self) -> Result<Connection> {
        let connection = Connection::open(&self.path).with_context(|| {
            format!("Could not open library database: {}.", self.path.display())
        })?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(connection)
    }

    fn migrate(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS entries (
                id          TEXT PRIMARY KEY,
                path        TEXT NOT NULL UNIQUE,
                title       TEXT,
                author      TEXT,
                added_at    INTEGER NOT NULL,
                opened_at   INTEGER,
                page_count  INTEGER,
                last_page   INTEGER DEFAULT 0,
                rating      INTEGER DEFAULT 0,
                cover_hash  TEXT,
                missing     INTEGER DEFAULT 0 NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tags (
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                tag         TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag)
            );

            CREATE TABLE IF NOT EXISTS bookmarks (
                id          TEXT PRIMARY KEY,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                page        INTEGER NOT NULL,
                label       TEXT,
                created_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS annotations (
                id          TEXT PRIMARY KEY,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                page        INTEGER NOT NULL,
                kind        TEXT NOT NULL,
                data        TEXT NOT NULL,
                created_at  INTEGER NOT NULL
            );

            INSERT OR IGNORE INTO schema_version (version) VALUES (1);
            "#,
        )?;
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN missing INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        Ok(())
    }
}
