---
title: Library Database
eyebrow: Subsystems
lede: Each named library is its own SQLite file; every PDF's identity comes from its bytes, not its path.
order: 21
---

<p class="trail"><strong>Trail</strong> <a href="../crates/core.md">core crate</a> <span class="sep">·</span> <a href="search.md">Search</a> <span class="sep">·</span> <a href="sync.md">Sync</a> <span class="sep">·</span> <a href="bulk-action.md">Bulk action</a> <span class="sep">·</span> <a href="../api/pdf-folio-core/db.md">API · db</a></p>

**Code:** `pdf-folio-core/src/db/`  
**API:** [db](../api/pdf-folio-core/db.md) · [types](../api/pdf-folio-core/db/types.md) · [schema](../api/pdf-folio-core/db/schema.md)

[`Db`](../api/pdf-folio-core/db.md) wraps a path to a bundled `rusqlite` database (no system SQLite required). Each discrete library the user creates is a separate `.db` file; switching libraries swaps `Arc<Db>` and the search index the UI uses.

Default open path: XDG data dir + `library.db` (see [Data directories](../operations/data-dirs.md)). Multi-library profiles store additional paths in `libraries.json`.

## Design principles

| Principle | Consequence |
| --- | --- |
| Content-addressed identity | Entry primary key = BLAKE3 of bytes |
| Short-lived connections | Each method opens/closes; no pool inside `Db` |
| App folders ≠ FS folders | Moving PDFs in-app does not move files on disk |
| Soft trash | `trashed_at` timestamps, not hard delete by default |
| Snapshot undo | Organizational undo restores full snapshots |
| Core has no iced | CLI and sync server reuse the same queries |

## PDF identity is its bytes

Primary key is a **BLAKE3** content hash ([`EntryId`](../api/pdf-folio-core/db/types.md)), not the filesystem path.

```text
hash_file(path) → EntryId
import_pdf → insert_entry(NewLibraryEntry { id, path, … })
```

Consequences maintainers rely on:

| Property | Why it matters |
| --- | --- |
| Idempotent import | Same bytes → same id; renames/copies do not duplicate identity |
| Path is mutable metadata | Relink/missing flags without losing tags/progress |
| Sync blob keys | R2 object key is `blobs/<hash>.pdf` — computable offline |
| Metadata split | Extracted title/author vs display overrides are independent columns |

Folder import (`import_folder`) scans recursively for `.pdf` files; per-file failures accumulate in an error list without aborting the whole scan.

### Path vs content

| Operation | Touches id? | Touches path? |
| --- | --- | --- |
| Import new file | Creates id from hash | Sets path |
| Rename on disk + reimport | Same id if bytes unchanged | Updates path |
| Edit PDF bytes | **New** id | New or same path |
| Relink missing | No | User picks new path |
| Trash | No | No |

## Schema anatomy

Created in `schema.rs` (migrations also `ALTER TABLE` for older DBs):

| Table | Purpose |
| --- | --- |
| `entries` | One row per PDF: path, extracted/display/sort metadata, progress, rating, missing, trash |
| `tags` | `(entry_id, tag)` join |
| `folders` | Nested via `parent_id`; soft-delete via `trashed_at` |
| `entry_folders` | Membership + per-folder `manual_order` |
| `bookmarks` | Local per-page landmarks (schema reserved; UI pending) |
| `annotations` | Text-anchored comments per entry (page/char ranges, quote snapshot, body; FK cascade on entry delete; not in PDF bytes) |
| `library_preferences` | Key/value UI prefs (sort, layout, sidebar width, …) |
| `import_sources` | Provenance roots (e.g. Raindrop account) |
| `raindrop_collections` / `raindrop_entries` | Remote id ↔ local folder/entry maps |
| `sync_entries` / `sync_folders` / `sync_entry_folders` | Materialized sync projections |
| `sync_crdt_operations` | Append-only CRDT op log |
| `sync_crdt_entity_versions` / `sync_crdt_checkpoints` | LWW bookkeeping |
| `sync_checkpoints` / `sync_blob_uploads` | Device checkpoints and uploaded blob hashes |

Foreign keys use `ON DELETE CASCADE` so joins stay consistent without manual cleanup in every call site.

### Migration practice

- Prefer additive migrations (`ALTER TABLE … ADD COLUMN`) with defaults.
- Keep `schema.rs` as the single open/migrate path.
- After schema changes, update `types.rs` row mappers and any UI that assumes columns.
- Sync tables must stay compatible with CRDT payload expectations in `pdf-folio-cloud`.

## Module ownership

| File | Public concerns |
| --- | --- |
| `library.rs` | Insert/lookup/update entries, missing, trash restore/purge |
| `organization.rs` | Folders, membership, tags, manual order gaps, org snapshots |
| `metadata.rs` | Display fields, locks, preferences |
| `annotations.rs` | Text-annotation CRUD for library PDFs |
| `import.rs` | Hash, import, watcher, thumbnail paths |
| `search.rs` | Tantivy wrapper |
| `raindrop.rs` | Mapping CRUD (no HTTP) |
| `sync.rs` | Seed sync metadata, prepare ops, checkpoints |
| `naming.rs` | Private helpers: sort keys, `MANUAL_ORDER_GAP` |
| `types.rs` | Shared domain types re-exported from crate root |
| `schema.rs` | Open, migrate, CREATE TABLE |

## Folders, tags, and trash

**Folders are app metadata**, not filesystem directories. Moving a PDF between folders in the UI updates `entry_folders` only.

```text
folders
  id, parent_id, name, manual_order, trashed_at, …

entry_folders
  entry_id, folder_id, manual_order
```

Tags are free-form strings per entry (`tags` table). Renaming a tag is a bulk update across rows; deleting a tag removes associations.

Trash uses `trashed_at` timestamps (entries and folders), not boolean flags. Restore clears the timestamp; permanent delete removes rows. Folder trash interacts with parentage so subtree restore/delete can be reasoned about from timestamps + tree structure.

## Undo as snapshots

Organizational undo does not store inverse ops. It captures `LibraryOrganizationSnapshot` (folders, memberships, trash states, tags) before and after an edit. Undo restores the before snapshot; redo restores after.

After restore, search may need reindex for entries whose trash visibility changed (`search_changed_entry_ids` pattern in UI tasks).

History UI state lives in the UI crate (`LibraryHistory` / nodes on `LibraryRuntime`), not in SQLite, though the snapshot payload is pure core types.

## Preferences

`LibraryPreferences` (and related sort/layout enums) store per-library presentation choices. They are **not** the same as:

- `Settings` on `PDFolioApp` (process-level: tile cache size, watch dirs)
- `AppSession` (window geometry, last document)

## Sync tables (local)

Local sync metadata is prepared so the cloud crate can push/pull without scraping UI state:

| Concern | Tables / helpers |
| --- | --- |
| Materialized remote view | `sync_entries`, `sync_folders`, `sync_entry_folders` |
| Op log | `sync_crdt_operations` |
| Versions / cursors | entity versions, checkpoints |
| Blob upload tracking | `sync_blob_uploads` |

See [Sync](sync.md) for the network/CRDT side.

## Testing

```bash
cargo test -p pdf-folio-core
```

DB tests use temp directories. Prefer testing invariants (idempotent import, trash restore, snapshot round-trip) over UI.

## Connections

- Import + watcher feed entries: [Search & watching](search.md)
- CRDT tables feed: [Sync](sync.md)
- Raindrop provenance: [Raindrop](raindrop.md)
- End-to-end edit: [Bulk action](bulk-action.md)
- Crate map: [pdf-folio-core](../crates/core.md)
