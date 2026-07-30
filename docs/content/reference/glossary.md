---
title: Glossary
eyebrow: Reference
lede: Shared vocabulary used across crates, guides, and rustdoc — so “entry”, “tile”, and “vault” mean the same thing everywhere.
order: 40
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/overview.md">Architecture</a> <span class="sep">·</span> <a href="../architecture/state.md">State</a> <span class="sep">·</span> <a href="../subsystems/database.md">Database</a> <span class="sep">·</span> <a href="../subsystems/sync.md">Sync</a> <span class="sep">·</span> <a href="../index.md">Home</a></p>

Terms are listed alphabetically. Code identifiers use Rust casing in backticks. Prefer these links over inventing new names in PRs.

## A–C

| Term | Meaning |
| --- | --- |
| **[AppMode](../api/pdf-folio-ui/shell/app.md)** | Which primary UI surface is active: signed-out, library, viewer, or library switcher. [Overview · modes](../architecture/overview.md#modes). |
| **[AppearanceRuntime](../architecture/state.md#appearance-appearanceruntime)** | Holds theme selection and the loaded [`StyleBook`](../api/pdf-folio-style/book.md). |
| **Blob / blob cache** | Content-addressed PDF bytes for sync. Remote keys look like `blobs/<blake3>.pdf`. Local cache under `sync/blobs/` — [sync](../subsystems/sync.md), [blobs API](../api/pdf-folio-cloud/sync/blobs.md). |
| **[Bulk action](../subsystems/bulk-action.md)** | Multi-entry library mutation (move, tag, trash, metadata, export, …) with progress UI. |
| **[ChromeRuntime](../architecture/state.md)** | Cross-mode UI: confirmations, context menus, command palette, cursor. |
| **Command / [CommandId](../api/pdf-folio-ui/shell/commands.md)** | Named user intent from menus or the command palette; resolves into [messages](../architecture/messages.md)/tasks. |
| **Control plane** | Self-hosted [`pdf-folio-sync-server`](../api/pdf-folio-cloud/bin/pdf-folio-sync-server.md) that authenticates the user and mints short-lived Turso/R2 credentials. Does not store library content. [Sync · three tiers](../subsystems/sync.md#three-tiers). |
| **CRDT op** | Append-only operation in `sync_crdt_operations` used for last-writer-wins merge of library metadata across devices. [CRDT model](../subsystems/sync.md#crdt-metadata-model). |
| **[ConfirmationAction](../api/pdf-folio-ui/shell/messages.md)** | Destructive action that must pass through a confirm dialog before executing. |

## D–L

| Term | Meaning |
| --- | --- |
| **[Db](../api/pdf-folio-core/db.md)** | SQLite library handle in [`pdf-folio-core`](../crates/core.md). Path-backed; short-lived connections per call. [Database](../subsystems/database.md). |
| **Display metadata** | User-overridable title/author (etc.) separate from extracted PDF metadata. |
| **Entry / [EntryId](../api/pdf-folio-core/db/types.md)** | One PDF in a library. Id is the **BLAKE3 hex of file bytes**, not the path. [Identity](../subsystems/database.md#pdf-identity-is-its-bytes). |
| **Entry folder membership** | Row linking an entry to a folder, with optional manual order. |
| **Extracted metadata** | Title/author/page count taken from the PDF at import time. |
| **Folder / [FolderId](../api/pdf-folio-core/db/types.md)** | App-level organization node (not a filesystem directory). Soft-deleted via `trashed_at`. |
| **Generation (counter)** | Monotonic id used to ignore stale async results (search, zoom settle). [Messages](../architecture/messages.md#generation-counters). |
| **Hrana** | HTTP protocol used to talk to Turso/libSQL from the desktop client. |
| **Hydration** | Sync step that materializes remote library rows and downloads missing blobs into the local cache. [Sync pass](../subsystems/sync.md#a-sync-pass). |
| **Library (vault)** | One discrete SQLite database + its entries/folders. Users may keep several profiles — [multi-library](../subsystems/multi-library.md). |
| **[LibraryPreferences](../api/pdf-folio-core/db/types.md)** | Per-library UI prefs stored in SQLite (sort, layout, sidebar, …). |
| **[LibraryRuntime](../architecture/state.md#library-libraryruntime)** | All in-memory library browsing state on [`PDFolioApp`](../api/pdf-folio-ui/shell/app.md). |
| **LWW** | Last-writer-wins conflict rule for CRDT ops, ordered by `(logical_time, device_id, op_id)`. |

## M–R

| Term | Meaning |
| --- | --- |
| **Manual order** | User-defined sort positions using spaced integer keys (`MANUAL_ORDER_GAP`) so inserts need not renumber everything. |
| **[Message](../architecture/messages.md)** | Single global UI event enum; only path into `update`. |
| **Missing** | Entry flag when the file path no longer exists on disk; metadata/tags kept for relink. |
| **Organization snapshot** | Full capture of folders/memberships/tags/trash used for undo/redo (not inverse ops). [Undo](../subsystems/database.md#undo-as-snapshots). |
| **[PDFolioApp](../api/pdf-folio-ui/shell/app.md)** | Root iced application state. [Runtime state](../architecture/state.md). |
| **[PdfDoc](../api/pdf-folio-core/pdf/document.md)** | Cloneable PDF handle storing path + page count; reopens Pdfium per operation. [Rendering](../subsystems/rendering.md). |
| **Pdfium** | Native PDF library used via `pdfium-render` for render/text/outline. |
| **PKCE** | OAuth proof-key flow used for Google sign-in to the control plane / Raindrop. |
| **[Raindrop](../subsystems/raindrop.md)** | Third-party bookmark service; PDF-Folio can import PDF raindrops and mirror collections. |
| **Registry / libraries.json** | List of library profiles (id, name, db path) for multi-library support. [Multi-library](../subsystems/multi-library.md), [data dirs](../operations/data-dirs.md). |
| **REGISTRY_LIBRARY_ID** | Synthetic library id for syncing profile existence separately from content. |
| **RenderedPage** | RGBA bitmap + dimensions for one page at a target width. |

## S–Z

| Term | Meaning |
| --- | --- |
| **[SearchIndex](../api/pdf-folio-core/db/search.md)** | Tantivy index of per-page PDF text for library search. [Search](../subsystems/search.md). |
| **Session (app)** | `session.json` — window size, mode, last document restore. [Shell](../architecture/shell.md#session-and-auth). |
| **Session (sync)** | Cached OAuth/control-plane credentials under `sync/session.json`. |
| **Shell** | [`pdf-folio-ui/src/shell/`](../architecture/shell.md) orchestration layer. |
| **Spread mode** | Viewer single vs two-page layout. |
| **[StyleBook](../api/pdf-folio-style/book.md)** | Parsed KDL design tokens/classes used by styled widgets. [Style system](../subsystems/style-system.md). |
| **SyncAuthRuntime** | Sign-in state machine for optional sync gate. |
| **Task** | iced async/blocking unit of work that completes as a [`Message`](../architecture/messages.md). [Overview](../architecture/overview.md#side-effects-as-tasks). |
| **Tile / [TileKey](../api/pdf-folio-core/pdf/renderer.md) / [TileCache](../api/pdf-folio-core/pdf/renderer.md)** | Cached page raster keyed by `{ page, width_px }` with LRU eviction. [Tiles](../subsystems/rendering.md#tiles-and-the-render-cache). |
| **Trash** | Soft-delete via `trashed_at` timestamps on entries and folders. [Folders & trash](../subsystems/database.md#folders-tags-and-trash). |
| **Turso** | Hosted libSQL used as the sync metadata store. |
| **Vault** | Informal synonym for a discrete library profile. |
| **[ViewerRuntime](../architecture/state.md#viewer-viewerruntime)** | Open document + render/find/outline state. |
| **Watch / LibraryWatcher** | `notify`-based filesystem watch that reimports/changes library rows via messages. [Search & watching](../subsystems/search.md). |
| **Zoom settle** | Delay after wheel zoom before re-rasterizing at the final width (avoids thrash). [Zoom](../subsystems/rendering.md#zoom-without-flicker). |

## Acronyms & stack

| Acronym | Expansion in this project |
| --- | --- |
| **CRDT** | Conflict-free replicated data type (here: op log + LWW) — [sync](../subsystems/sync.md) |
| **JWT** | Session token from control plane |
| **KDL** | Document language for themes (`styles/*.kdl`) — [style system](../subsystems/style-system.md) |
| **LRU** | Least-recently-used (tile cache) |
| **R2** | Cloudflare object storage for PDF blobs |
| **XDG** | Freedesktop base dirs for data/config/cache — [data dirs](../operations/data-dirs.md) |
