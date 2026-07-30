---
title: pdf-folio-core
eyebrow: Crates
lede: UI-free foundation — Pdfium document API, tile cache, SQLite library, Tantivy search, import and filesystem watch.
order: 11
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/workspace.md">Workspace</a> <span class="sep">·</span> <a href="../subsystems/database.md">Database</a> <span class="sep">·</span> <a href="../subsystems/rendering.md">Rendering</a> <span class="sep">·</span> <a href="../api/pdf-folio-core/index.md">API</a></p>

**Path:** `crates/pdf-folio-core/`  
**Rule:** no iced, no network clients for Raindrop/sync servers.

Public surface is re-exported from [`src/lib.rs`](../api/pdf-folio-core/index.md) so callers write `pdf_folio_core::{Db, PdfDoc, …}`.

## Tree

```text
pdf-folio-core/src/
  lib.rs
  pdf/
    mod.rs
    document.rs    # PdfDoc, outline, text layer, render_page
    renderer.rs    # TileKey, TileCache (LRU)
    geometry.rs    # TextRect and shared geometry
    tests.rs
  db/
    mod.rs         # Db struct shell
    types.rs       # EntryId, LibraryEntry, Folder, preferences, …
    schema.rs      # open, migrate, CREATE TABLE
    library.rs     # entry CRUD, trash, lookup
    organization.rs# folders, tags, membership, snapshots, ordering
    naming.rs      # private helpers (manual order gaps, sort keys)
    metadata.rs    # display metadata, preferences
    import.rs      # hash/import/folder scan + LibraryWatcher
    search.rs      # SearchIndex (Tantivy)
    raindrop.rs    # raindrop_* mapping tables only
    sync.rs        # local sync metadata / CRDT seed helpers
    tests.rs (+ import/tests, search/tests)
```

## `pdf/` — documents and tiles

### `PdfDoc`

Thin, clone-friendly handle: stores **path + page count only**. Operations reopen the file via an internal `with_document()` helper so no non-`Send` Pdfium document lives across `.await`.

Key APIs:

| Method | Purpose |
| --- | --- |
| `open` | Read page count |
| `render_page(index, width_px)` | RGBA bitmap at target width |
| `page_aspect_ratio` | Layout without full raster |
| `outline` | TOC tree → `OutlineNode` |
| `text_on_page` / `text_layer` | Plain text or char boxes (normalized) |
| `metadata_title` / `metadata_author` | Embedded PDF metadata |

Pdfium is bound **once per process** (`OnceLock`) and guarded by a process-wide mutex. Missing system/bundled Pdfium yields errors, not process aborts.

### `TileCache` / `TileKey`

```text
TileKey { page, width_px }  →  Arc<Vec<u8>> RGBA
```

Default capacity 64 tiles. Zoom at a new width is a different key — crisp zoom means re-rasterize, not scale forever.

Details: [Rendering pipeline](../subsystems/rendering.md).

## `db/` — library storage

`Db` holds a path; each method opens a short-lived `rusqlite::Connection` with foreign keys on. There is no long-lived connection pool inside the struct.

| Module | Owns |
| --- | --- |
| `schema` | XDG default path, migrations, table definitions |
| `library` | Entries, missing flag, trash timestamps |
| `organization` | Folders, entry_folders, tags, manual order, org snapshots |
| `metadata` | Display overrides, attribution flags, preferences |
| `import` | BLAKE3 hash, import, thumbnails paths, `LibraryWatcher` |
| `search` | Per-page Tantivy index |
| `raindrop` | Collection/entry mapping rows (not HTTP) |
| `sync` | Local CRDT/ops tables, seeding, checkpoints |

Identity model and schema: [Library database](../subsystems/database.md).  
Search/watch: [Search & watching](../subsystems/search.md).

## Types maintainers touch often

From `db/types.rs`:

- `EntryId` / `FolderId` — newtype strings (entry id = BLAKE3 hex of file bytes)
- `LibraryEntry` — path, extracted vs display metadata, progress, trash, missing
- `Folder`, `EntryFolderMembership`, `EntryTrashState`
- `LibraryOrganizationSnapshot` — undo/redo unit
- `LibraryPreferences`, `LibrarySortMode`, `LibraryLayoutMode`
- Sync-related row types used by cloud (`SyncCrdtOperation`, etc.)

## Tests

```bash
cargo test -p pdf-folio-core
```

DB tests use temp paths; PDF tests use fixtures under `tests/fixtures/`.

## API reference

Extracted rustdoc for this crate:

- [pdf-folio-core](../api/pdf-folio-core/index.md)
- [pdf](../api/pdf-folio-core/pdf.md) · [document](../api/pdf-folio-core/pdf/document.md) · [renderer](../api/pdf-folio-core/pdf/renderer.md)
- [db](../api/pdf-folio-core/db.md) · [types](../api/pdf-folio-core/db/types.md) · [schema](../api/pdf-folio-core/db/schema.md) · [library](../api/pdf-folio-core/db/library.md) · [organization](../api/pdf-folio-core/db/organization.md) · [import](../api/pdf-folio-core/db/import.md) · [search](../api/pdf-folio-core/db/search.md) · [sync](../api/pdf-folio-core/db/sync.md)

## Related

- [Workspace](../architecture/workspace.md)
- [Database](../subsystems/database.md)
- [Rendering](../subsystems/rendering.md)
