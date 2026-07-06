CREATE TABLE IF NOT EXISTS libraries (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
);

CREATE TABLE IF NOT EXISTS library_entries (
    id          TEXT NOT NULL,
    library_id  TEXT NOT NULL,
    title       TEXT,
    author      TEXT,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER,
    PRIMARY KEY (library_id, id)
);

CREATE TABLE IF NOT EXISTS library_folders (
    id          TEXT NOT NULL,
    library_id  TEXT NOT NULL,
    name        TEXT,
    parent_id   TEXT,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER,
    PRIMARY KEY (library_id, id)
);

CREATE TABLE IF NOT EXISTS library_entry_folders (
    library_id   TEXT NOT NULL,
    entry_id    TEXT NOT NULL,
    folder_id   TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER,
    PRIMARY KEY (library_id, entry_id, folder_id)
);

CREATE TABLE IF NOT EXISTS library_entry_tags (
    library_id   TEXT NOT NULL,
    entry_id    TEXT NOT NULL,
    tag         TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER,
    PRIMARY KEY (library_id, entry_id, tag)
);

CREATE TABLE IF NOT EXISTS reading_progress (
    entry_id    TEXT NOT NULL,
    library_id  TEXT NOT NULL,
    progress    REAL NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (entry_id, library_id)
);

CREATE INDEX IF NOT EXISTS idx_libraries_updated
    ON libraries(updated_at);

CREATE INDEX IF NOT EXISTS idx_library_entries_updated
    ON library_entries(library_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_library_folders_updated
    ON library_folders(library_id, updated_at);

CREATE INDEX IF NOT EXISTS idx_library_entry_folders_updated
    ON library_entry_folders(library_id, updated_at);
