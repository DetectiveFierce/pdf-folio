---
title: Runtime State
eyebrow: Architecture
lede: The PDFolioApp tree — which nested runtime owns what, and where to look when state is wrong.
order: 4
---

<p class="trail"><strong>Trail</strong> <a href="shell.md">Shell</a> <span class="sep">·</span> <a href="messages.md">Messages</a> <span class="sep">·</span> <a href="../subsystems/rendering.md">Rendering</a> <span class="sep">·</span> <a href="../subsystems/database.md">Database</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/shell/app.md">API · app</a></p>

All UI-visible state hangs off [`PDFolioApp`](../api/pdf-folio-ui/shell/app.md) in [`shell/app.rs`](../api/pdf-folio-ui/shell/app.md). Nested runtimes keep call sites explicit about *which subsystem* they touch.

<div class="diagram"><span class="hl">PDFolioApp</span>
  mode:              AppMode
  viewer:            ViewerRuntime
  library:           LibraryRuntime
  libraries:         LibraryRegistryRuntime
  chrome:            ChromeRuntime
  appearance:        AppearanceRuntime
  settings:          Settings
  sync_auth:         SyncAuthRuntime
  db:                Arc&lt;Db&gt;
  sync_in_progress / sync_queued_libraries / last_sync_*
  startup_background_ready
  pending_session_restore</div>

When debugging “why is the UI wrong?”, identify the runtime first, then the field. Resist dumping everything into loose fields on `PDFolioApp`.

## Field guide

### `mode: AppMode`

Primary surface: `SignedOut` | `Library` | `Viewer` | `LibrarySwitcher`.

Mode changes should leave other runtimes in a coherent state (e.g. switching library clears viewer document fields and reloads entries).

### `viewer: ViewerRuntime`

Defined in [`viewer/document.rs`](../api/pdf-folio-ui/viewer/document.md) (and related viewer modules). Holds:

| Concern | Typical fields |
| --- | --- |
| Open document | `PdfDoc` (`Arc`), entry id, path |
| Raster | rendered page map, aspect ratios, `TileCache`, pending renders |
| Viewport | scroll offset, zoom width, spread/scroll mode |
| Text | text layers, selection, find bar state |
| Outline | TOC tree + selection |
| Errors | document error banner state |

Deep dive: [Rendering pipeline](../subsystems/rendering.md). API: [`PdfDoc`](../api/pdf-folio-core/pdf/document.md) · [`TileCache`](../api/pdf-folio-core/pdf/renderer.md).  
API: [viewer document](../api/pdf-folio-ui/viewer/document.md) · [viewer state](../api/pdf-folio-ui/viewer/state.md)

### `library: LibraryRuntime`

Library browsing surface. Large; group fields mentally:

| Group | Examples |
| --- | --- |
| Data | entries, trash entries, folders, trash folders, tags |
| Presentation | sort mode, layout, grid zoom, metadata density, scroll offset |
| Selection & drag | selected ids, anchor, drag state, drop flash |
| Search | query, hits, hit pages, generation |
| Chrome-in-library | sidebar width/tab, inspector, dialogs |
| Thumbs | cache maps, pending keys |
| Ops progress | bulk op progress, import/Raindrop progress |
| Feedback | status string |

Domain logic: `library/update.rs`, `library/tasks.rs`, `library/actions.rs`.  
Views: `library/view/`. Pure helpers: `components/library/`.

API: [library state](../api/pdf-folio-ui/library/state.md) · [LibraryRuntime in app](../api/pdf-folio-ui/shell/app.md)

### `libraries: LibraryRegistryRuntime`

Multi-library (vault) registry — profiles with ids, names, and SQLite paths. Switching libraries swaps `app.db` and resets viewer/library transient state. See [Multi-library registry](../subsystems/multi-library.md).

### `chrome: ChromeRuntime`

Confirmations, context menus, command palette, cursor position. Cross-mode only — do not put library-only dialog state here if it already lives on `LibraryRuntime`.

### `appearance: AppearanceRuntime`

`AppTheme` + loaded `StyleBook` + optional load error. Style reloads replace the book without restarting the process.

### `settings: Settings`

| Field | Default | Meaning |
| --- | --- | --- |
| `default_zoom_width` | 800 | Initial render width |
| `tile_cache_pages` | 64 | Tile LRU capacity |
| `watch_directories` | `[]` | Paths fed to `LibraryWatcher` |

Settings are distinct from per-library preferences (`LibraryPreferences` in the DB) and from session restore (`AppSession`).

### `sync_auth` and sync queue fields

Auth runtime plus:

| Field | Role |
| --- | --- |
| `sync_in_progress` | Library id currently syncing (exclusive) |
| `sync_queued_libraries` | Work deferred until the current pass ends |
| `last_sync_started_at` / `last_sync_completed_at` | Auto-sync scheduling |

Never start a second concurrent sync pass for overlapping libraries without going through the queue.

### `db: Arc<Db>`

Active library SQLite handle. Opening another library replaces this `Arc` and refreshes folder/entry lists. Tasks should clone the `Arc` (cheap) rather than assuming a global.

### Startup flags

| Field | Role |
| --- | --- |
| `startup_background_ready` | Gates heavy subscriptions/tasks |
| `pending_session_restore` | Deferred restore until shell is ready |

## Persistence map

| State | Stored where |
| --- | --- |
| Entries, folders, tags, prefs | Active library `.db` |
| Multi-library profiles | `libraries.json` |
| Window/mode/last doc | `session.json` |
| Sync OAuth session | `sync/session.json` |
| Thumbnails | cache dir `thumbs/` |
| Search index | data dir `search-index/` (or per-library path) |
| User styles | config `styles/*.kdl` |

See [Data directories](../operations/data-dirs.md).

## Immutability discipline

`PDFolioApp` is `Clone` for iced's model. Large resources are already behind `Arc` (`Db`, `PdfDoc`, `StyleBook`, tile buffers). Prefer cloning `Arc`s into tasks rather than deep-cloning large vectors when possible.

When adding large collections, consider whether the view needs the full clone every frame or can hold derived indices.

## Where mutations happen

| Kind of change | Typical location |
| --- | --- |
| Selection, filters, UI toggles | `library/update.rs` or `viewer/update.rs` (sync, no task) |
| Import, metadata, folder tree writes | `library/tasks.rs` → completion messages |
| Page render / open document | `viewer/tasks.rs` |
| Sync pass | shell tasks + cloud client |
| Theme reload | shell update + style book load |
| Registry switch | registry tasks + shell mode |

## Common “wrong state” checklists

| Bug | Look at |
| --- | --- |
| Stale grid after import | refresh task after import summary; search generation |
| Thumbnails missing | thumbnail cache keys, pending set, cache dir permissions |
| Wrong folder counts | smart counts vs raw folder rows; trash filters |
| Viewer shows old PDF after switch | document fields cleared on library switch |
| Undo broke search visibility | reindex after org snapshot restore |
| Drag targets wrong | cursor in chrome runtime; layout hit-test helpers |

## Related

- [Application shell](shell.md)
- [Message surface](messages.md)
- [UI crate](../crates/ui.md)
- [Library database](../subsystems/database.md)
