---
title: pdf-folio-ui
eyebrow: Crates
lede: The iced application — shell orchestration, library manager, PDF viewer, and UI components.
order: 12
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/overview.md">Architecture</a> <span class="sep">·</span> <a href="../architecture/shell.md">Shell</a> <span class="sep">·</span> <a href="../architecture/messages.md">Messages</a> <span class="sep">·</span> <a href="../subsystems/bulk-action.md">Bulk action</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/index.md">API</a></p>

**Path:** `crates/pdf-folio-ui/`  
**Role:** All interactive state and views. Depends on `pdf-folio-core`, `pdf-folio-cloud`, and `pdf-folio-style`.

This is the largest crate ([API index](../api/pdf-folio-ui/index.md)) (~25k+ lines of Rust under `src/`). Navigation by **subtree** matters more than reading `lib.rs` end-to-end — `lib.rs` is a large re-export/import hub for historical reasons.

## Top-level layout

```text
pdf-folio-ui/src/
  lib.rs              # run(), re-exports, heavy use imports
  tests.rs            # integration-style UI tests
  shell/              # app state, messages, update, session, shortcuts
  library/            # library domain: update, tasks, view, registry
  viewer/             # viewer domain: update, tasks, view, document state
  components/         # pure-ish UI widgets and helpers
    shared/
    library/
    viewer/
```

## Subtree responsibilities

### `shell/` — process-level app

Owns `PDFolioApp`, `Message`, top-level `update`, subscriptions, menus/command wiring, session, sync auth. See [Application shell](../architecture/shell.md) and [Runtime state](../architecture/state.md).

### `library/` — library mode domain

| Module | Role |
| --- | --- |
| `update.rs` | Message handling for library (~2.5k lines) |
| `tasks.rs` | Async import, bulk ops, Raindrop, export, search (~2k lines) |
| `actions.rs` | Intent helpers composing tasks/state |
| `state.rs` | Extra library enums (filters, density, etc.) |
| `data.rs` / `layout.rs` | Derived data and layout math |
| `thumbnails.rs` | Cover thumbnail cache keys and render tasks |
| `registry/` | Multi-library profiles, previews, switcher tasks |
| `view/` | Compose library screen (`root`, `entries`, `folders`, `sidebar`) |

`library/mod.rs` re-exports pure helpers from `components/library` (`drag`, `filters`, `metadata`, `selection`) so domain code and components share one path.

### `viewer/` — viewer mode domain

| Module | Role |
| --- | --- |
| `document.rs` | `ViewerRuntime` fields |
| `state.rs` | Viewer helpers on `PDFolioApp` |
| `update.rs` | Scroll, zoom, find, outline messages |
| `tasks.rs` | Open document, render page, text layer tasks |
| `rendering.rs` | Zoom policies, settle timing |
| `navigation.rs` / `layout.rs` | Page geometry, ranges |
| `view/` | Canvas composition |

### `components/` — UI building blocks

Prefer putting **geometry and presentation** here and **persistence** in `library/tasks` or core.

**Shared** (`components/shared/`):

| File | Purpose |
| --- | --- |
| `command_palette.rs` | Palette UI |
| `context_menu.rs` | Positioned menus |
| `menus.rs` | Window/menu bar structures |
| `sidebar.rs` | Reusable sidebar chrome |
| `sync_status.rs` | Sync indicator |
| `error_banner.rs` / `loading.rs` | Feedback |
| `icons.rs` / `root_surface.rs` | Icons and root chrome |

**Library** (`components/library/`):

| File | Purpose |
| --- | --- |
| `cards.rs` / `view.rs` | Cards and list presentation |
| `drag.rs` | Drag hit-testing and reorder math |
| `selection.rs` | Multi-select / range helpers |
| `filters.rs` | Visibility and search field matching |
| `folder_tree.rs` / `inspector.rs` | Tree and details |
| `dialogs.rs` | Modal dialogs (large) |
| `import_status.rs` | Import progress UI |
| `metadata.rs` | Labels and formatting |

**Viewer** (`components/viewer/`):

| File | Purpose |
| --- | --- |
| `canvas.rs` | Page canvas drawing |
| `toolbar.rs` / `page_controls.rs` / `zoom.rs` | Controls |
| `find_bar.rs` / `outline.rs` / `sidebar.rs` | Find and TOC chrome |

Styled primitives (buttons, tags, cards) often come from **`pdf-folio-style`**, not from inventing local colors here.

## Update path (reminder)

```text
shell::update
  → library::update  (Option<Task>)
  → viewer::update   (Option<Task>)
  → shell match (auth, sync, chrome, startup, …)
```

## Assets

```text
pdf-folio-ui/assets/icons/   # SVG icons (folder, overflow, …)
```

Fonts are embedded in `pdf-folio-style` and registered at startup from the UI crate.

## Tests

```bash
cargo test -p pdf-folio-ui
```

`src/tests.rs` is large and exercises selection, filters, and shell behaviors without requiring a full GPU interactive session for every case.

## API reference

- [pdf-folio-ui](../api/pdf-folio-ui/index.md)
- [shell](../api/pdf-folio-ui/shell.md) · [app](../api/pdf-folio-ui/shell/app.md) · [messages](../api/pdf-folio-ui/shell/messages.md)
- [library](../api/pdf-folio-ui/library.md) · [viewer](../api/pdf-folio-ui/viewer.md) · [components](../api/pdf-folio-ui/components.md)

## Related

- [Architecture overview](../architecture/overview.md)
- [Bulk action walkthrough](../subsystems/bulk-action.md)
- [Style system](../subsystems/style-system.md)
