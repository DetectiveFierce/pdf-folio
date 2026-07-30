//! Database opening, connection setup, and SQLite migrations.
//!
//! Owns the lifecycle of the on-disk `library.db` file used by [`Db`].
//! Opening always runs `migrate`, which creates the baseline schema (entries,
//! folders, tags, import/raindrop tables, sync/CRDT tables) and applies
//! additive `ALTER TABLE` upgrades for older installs.
//!
//! Connections are not pooled: each domain method opens a short-lived rusqlite
//! handle with foreign keys enabled. Prefer [`Db::open_default`] for production
//! (XDG data dir) and [`Db::open`] with a temp path in tests.
//!
//! # See also
//!
//! - [`crate::LibraryEntry`], [`crate::Folder`], and related types for row shapes.
//! - [`super::library`], [`super::organization`], [`super::sync`] for queries
//!   against this schema.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use rusqlite::{params, Connection};

use super::naming::MANUAL_ORDER_GAP;
use super::Db;

impl Db {
    /// Opens the default library database under the XDG data directory.
    ///
    /// Resolves `…/PDF-Folio/library.db` via the `dev.pdf-folio.PDF-Folio`
    /// project dirs, creating the parent directory when needed.
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

    /// Opens a library database at `path` and runs migrations before returning.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot open or migrate the database.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let db = Self { path: path.into() };
        db.migrate()?;
        Ok(db)
    }

    /// Returns the filesystem path of this database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens a short-lived SQLite connection with foreign keys enabled.
    ///
    /// Used by all domain methods on [`Db`]. Callers must not hold the
    /// connection across `await` points; drop it promptly after the query.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be opened by rusqlite.
    pub(super) fn connection(&self) -> Result<Connection> {
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
                display_title TEXT,
                display_author TEXT,
                sort_title TEXT,
                sort_author TEXT,
                metadata_locked INTEGER DEFAULT 0 NOT NULL,
                manual_order INTEGER DEFAULT 0 NOT NULL,
                author_attributed INTEGER DEFAULT 0 NOT NULL,
                page_count_attributed INTEGER DEFAULT 0 NOT NULL,
                added_at    INTEGER NOT NULL,
                opened_at   INTEGER,
                page_count  INTEGER,
                file_size   INTEGER,
                last_page   INTEGER DEFAULT 0,
                rating      INTEGER DEFAULT 0,
                cover_hash  TEXT,
                missing     INTEGER DEFAULT 0 NOT NULL,
                trashed_at  INTEGER
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

            CREATE TABLE IF NOT EXISTS folders (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                parent_id   TEXT REFERENCES folders(id) ON DELETE CASCADE,
                manual_order INTEGER NOT NULL,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL,
                trashed_at  INTEGER
            );

            CREATE TABLE IF NOT EXISTS entry_folders (
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                folder_id   TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                manual_order INTEGER NOT NULL,
                PRIMARY KEY (entry_id, folder_id)
            );

            CREATE TABLE IF NOT EXISTS library_preferences (
                key         TEXT PRIMARY KEY,
                value       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS import_sources (
                id          TEXT PRIMARY KEY,
                kind        TEXT NOT NULL,
                account_id  TEXT,
                display_name TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS raindrop_collections (
                source_id   TEXT NOT NULL REFERENCES import_sources(id) ON DELETE CASCADE,
                collection_id INTEGER NOT NULL,
                folder_id   TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
                parent_collection_id INTEGER,
                title       TEXT NOT NULL,
                PRIMARY KEY (source_id, collection_id)
            );

            CREATE TABLE IF NOT EXISTS raindrop_entries (
                source_id   TEXT NOT NULL REFERENCES import_sources(id) ON DELETE CASCADE,
                raindrop_id INTEGER NOT NULL,
                entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                collection_id INTEGER,
                remote_link TEXT,
                remote_title TEXT,
                remote_updated_at TEXT,
                file_name   TEXT,
                file_size   INTEGER,
                PRIMARY KEY (source_id, raindrop_id)
            );

            CREATE TABLE IF NOT EXISTS sync_entries (
                id          TEXT NOT NULL,
                library_id  TEXT NOT NULL,
                title       TEXT,
                author      TEXT,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (id, library_id)
            );

            CREATE TABLE IF NOT EXISTS sync_folders (
                id          TEXT NOT NULL,
                library_id  TEXT NOT NULL,
                name        TEXT NOT NULL,
                parent_id   TEXT,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (id, library_id)
            );

            CREATE TABLE IF NOT EXISTS sync_entry_folders (
                entry_id    TEXT NOT NULL,
                folder_id   TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                deleted_at  INTEGER,
                PRIMARY KEY (entry_id, folder_id)
            );

            CREATE TABLE IF NOT EXISTS sync_checkpoints (
                library_id      TEXT NOT NULL,
                device_id       TEXT NOT NULL,
                last_synced_at  INTEGER NOT NULL,
                PRIMARY KEY (library_id, device_id)
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_operations (
                op_id           TEXT PRIMARY KEY,
                library_id      TEXT NOT NULL,
                device_id       TEXT NOT NULL,
                logical_time    INTEGER NOT NULL,
                entity_kind     TEXT NOT NULL,
                entity_id       TEXT NOT NULL,
                payload         TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                remote_sequence INTEGER,
                pushed_at       INTEGER
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_entity_versions (
                library_id      TEXT NOT NULL,
                entity_kind     TEXT NOT NULL,
                entity_id       TEXT NOT NULL,
                payload_hash    TEXT NOT NULL,
                logical_time    INTEGER NOT NULL,
                device_id       TEXT NOT NULL,
                PRIMARY KEY (library_id, entity_kind, entity_id)
            );

            CREATE TABLE IF NOT EXISTS sync_crdt_checkpoints (
                library_id            TEXT NOT NULL,
                device_id             TEXT NOT NULL,
                last_remote_sequence  INTEGER NOT NULL,
                PRIMARY KEY (library_id, device_id)
            );

            CREATE TABLE IF NOT EXISTS sync_blob_uploads (
                hash        TEXT PRIMARY KEY,
                uploaded_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS library_change_state (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                revision    INTEGER NOT NULL
            );

            INSERT OR IGNORE INTO library_change_state (id, revision) VALUES (1, 0);

            CREATE TABLE IF NOT EXISTS sync_local_snapshots (
                library_id      TEXT PRIMARY KEY,
                local_revision  INTEGER NOT NULL,
                captured_at     INTEGER NOT NULL
            );

            CREATE TRIGGER IF NOT EXISTS entries_change_revision_insert
            AFTER INSERT ON entries BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS entries_change_revision_update
            AFTER UPDATE ON entries BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS entries_change_revision_delete
            AFTER DELETE ON entries BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;

            CREATE TRIGGER IF NOT EXISTS tags_change_revision_insert
            AFTER INSERT ON tags BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS tags_change_revision_update
            AFTER UPDATE ON tags BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS tags_change_revision_delete
            AFTER DELETE ON tags BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;

            CREATE TRIGGER IF NOT EXISTS folders_change_revision_insert
            AFTER INSERT ON folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS folders_change_revision_update
            AFTER UPDATE ON folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS folders_change_revision_delete
            AFTER DELETE ON folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;

            CREATE TRIGGER IF NOT EXISTS entry_folders_change_revision_insert
            AFTER INSERT ON entry_folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS entry_folders_change_revision_update
            AFTER UPDATE ON entry_folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;
            CREATE TRIGGER IF NOT EXISTS entry_folders_change_revision_delete
            AFTER DELETE ON entry_folders BEGIN
                UPDATE library_change_state SET revision = revision + 1 WHERE id = 1;
            END;

            CREATE INDEX IF NOT EXISTS idx_sync_crdt_operations_library_entity
                ON sync_crdt_operations(library_id, entity_kind, entity_id);

            CREATE INDEX IF NOT EXISTS idx_sync_crdt_operations_pending
                ON sync_crdt_operations(library_id, device_id, pushed_at);

            INSERT OR IGNORE INTO schema_version (version) VALUES (1);
            "#,
        )?;
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN missing INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN author_attributed INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN page_count_attributed INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN display_title TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN display_author TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN sort_title TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN sort_author TEXT", []);
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN file_size INTEGER", []);
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN metadata_locked INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute(
            "ALTER TABLE entries ADD COLUMN manual_order INTEGER DEFAULT 0 NOT NULL",
            [],
        );
        let _ = connection.execute("ALTER TABLE entries ADD COLUMN trashed_at INTEGER", []);
        let _ = connection.execute("ALTER TABLE folders ADD COLUMN trashed_at INTEGER", []);
        connection.execute(
            "UPDATE entries
             SET manual_order = rowid * ?1
             WHERE manual_order = 0",
            params![MANUAL_ORDER_GAP],
        )?;
        connection.execute(
            "UPDATE entries
             SET sort_title = lower(COALESCE(display_title, title))
             WHERE sort_title IS NULL AND COALESCE(display_title, title) IS NOT NULL",
            [],
        )?;
        connection.execute(
            "UPDATE entries
             SET sort_author = lower(COALESCE(display_author, author))
             WHERE sort_author IS NULL AND COALESCE(display_author, author) IS NOT NULL",
            [],
        )?;
        backfill_file_sizes(&connection)?;
        Ok(())
    }
}

/// Fills null `entries.file_size` from on-disk metadata for non-missing paths.
fn backfill_file_sizes(connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT id, path FROM entries WHERE file_size IS NULL AND missing = 0")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let entries = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    for (id, path) in entries {
        if let Ok(metadata) = std::fs::metadata(&path) {
            connection.execute(
                "UPDATE entries SET file_size = ?1 WHERE id = ?2",
                params![metadata.len() as i64, id],
            )?;
        }
    }
    Ok(())
}
