---
title: Search & Filesystem Watching
eyebrow: Subsystems
lede: Per-page Tantivy index and notify-based import events that re-enter the app as ordinary messages.
order: 22
---

**Code:**  
- `pdf-folio-core/src/db/search.rs`  
- `pdf-folio-core/src/db/import.rs` (watcher + import)  
- UI: `library/tasks.rs` (search scheduling, reindex, watch apply)

**API:** [search](../api/pdf-folio-core/db/search.md) · [import](../api/pdf-folio-core/db/import.md)

## Full-text index (Tantivy)

[`SearchIndex`](../api/pdf-folio-core/db/search.md) uses a fixed schema and indexes **per page**, not per document:

```text
IndexDocument {
  entry_id,
  page,          // 0-based or 1-based — match implementation
  title,         // denormalized for display
  body,          // page text
  …
}
```

Hits return `SearchHit` rows the UI maps back to entries and optional page jumps.

### Why per-page?

| Benefit | Detail |
| --- | --- |
| Jump to page | Result can open viewer at the matching page |
| Incremental work | Reindex one entry’s pages after metadata/text change |
| Ranking locality | Matches are about a page, not a giant concatenated blob |

### Index lifecycle

| Event | Index action |
| --- | --- |
| Import PDF | Extract text pages (blocking) → add documents |
| Trash / restore | Remove or re-add as visibility requires |
| Permanent delete | Remove all pages for entry id |
| Bulk reindex | Rebuild selected or all entries |
| Org snapshot undo | Reindex entries whose visibility changed |

Text extraction uses `PdfDoc::text_on_page` (Pdfium). Large imports should stay on the blocking pool and report progress messages.

### UI search path

```text
User types query
  → Message updates query string
  → schedule_search bumps generation
  → Task queries SearchIndex
  → SearchFinished { generation, hits }
  → if generation matches, store hits and filter grid
```

Library filters (folder scope, reading state, tags) still apply on top of hit sets in pure helpers (`components/library/filters.rs`).

### Default index location

Under the XDG data directory (`search-index/`) unless a multi-library layout uses a path associated with the active vault. See [Data directories](../operations/data-dirs.md).

## Filesystem watching

[`LibraryWatcher`](../api/pdf-folio-core/db/import.md) wraps `notify` and emits `LibraryWatchEvent` values for configured roots (`Settings::watch_directories` and import sources as applicable).

```text
notify event
  → subscription / channel
  → Message wrapping LibraryWatchEvent
  → library tasks apply_watch_event
  → import/hash/update missing flags
  → refresh entries + maybe reindex
```

### Design constraints

- Watch events are **not** applied inside the notify callback thread to `PDFolioApp` — they become messages.
- Debounce / coalesce when possible to avoid thrashing on editors that write temp files.
- Only `.pdf` paths matter for import; ignore unrelated noise.

### Apply semantics (typical)

| FS change | Library effect |
| --- | --- |
| New PDF under watch root | Import (idempotent by hash) |
| PDF removed | Mark `missing` or remove depending on policy in code |
| PDF replaced (same path, new bytes) | New hash → new entry id; old path row may go missing |

Read `apply_watch_event` in UI tasks and core import helpers for the exact policy when changing behavior.

## Import API (related)

Core helpers used by both UI and watchers:

| Function | Role |
| --- | --- |
| `hash_file` | BLAKE3 hex |
| `import_pdf` | Hash + metadata + insert |
| `import_folder` | Recursive scan + per-file errors |
| `scan_pdf_files` | List PDFs under a root |
| `thumbnail_path` | Cache path for covers |

See [Database](database.md) for identity rules and [Bulk action](bulk-action.md) for UI orchestration.

## Testing

- Core search tests live under `db/search/tests.rs` (or adjacent).
- Prefer temp index dirs.
- Fixtures: `tests/fixtures/*.pdf`.

```bash
cargo test -p pdf-folio-core
```

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Search empty after import | Did reindex run? Is index path writable? |
| Stale hits after trash | Generation + remove-from-index on trash |
| Watch does nothing | `watch_directories`, subscription enabled after startup, notify permissions |
| High CPU on watch | Editor temp files; filter non-PDF; debounce |

## Related

- [Database](database.md)
- [Rendering (text extraction)](rendering.md)
- [Development](../operations/development.md)
