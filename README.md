# PDF-Folio

PDF-Folio is a native Rust PDF reader and local library manager for Linux. It combines a continuous PDF viewer with a searchable, taggable, folder-based PDF collection, using `iced` for the UI, Pdfium for rendering, SQLite for library metadata, and Tantivy for full-text search.

The project is currently organized as a Rust workspace with separate crates for PDF rendering, library storage/search, UI, and the binary entrypoint.

## Current Features

### PDF Viewer

- Open a PDF directly from the command line or through the app file picker.
- Continuous scrolling document view with virtualized page rendering.
- DPI-aware page rendering through Pdfium.
- Background render tasks so page rendering does not block the UI thread.
- Rendered page tile cache with an LRU policy and a default capacity of 64 pages.
- Zoom controls for zoom in, zoom out, and reset to the configured default width.
- Mouse wheel zoom handling with render debouncing.
- Horizontal panning for wide or zoomed-in pages.
- Jump-to-page overlay.
- Table-of-contents sidebar from the PDF outline/bookmark tree.
- Expandable outline nodes and direct navigation to outline targets.
- Reading progress tracking for library PDFs.
- Basic in-memory annotation data model for highlights, notes, and freehand drawings.

### Library Manager

- Create, rename, delete, preview, and switch between multiple named local libraries.
- Import a folder recursively and add all discovered PDF files.
- Import selected PDFs from Raindrop.io, including uploaded PDF files, tags, and optional folder/collection mirroring.
- Content-based PDF IDs using BLAKE3 hashes.
- SQLite-backed library metadata store.
- Last-session restore for the active library, theme, window size, library filters/selection, and open viewer state.
- Persisted library preferences for sort mode, layout mode, selected folder, and sidebar width.
- Grid and list library layouts.
- Masonry-style grid cards with cached cover thumbnails.
- Virtualized list/grid rendering for large libraries.
- Search bar with debounced search.
- Full-text search index powered by Tantivy.
- Search results can show the matching page for a PDF.
- Sort modes:
  - Manual
  - Title A-Z
  - Title Z-A
  - Author A-Z
  - Author Z-A
  - Recently Added
  - Recently Opened
  - Progress
  - Page Count
  - Missing
- Manual reorder support when the library is unfiltered and sorted manually.
- Drag-and-drop visual reorder support with autoscroll.
- Cut/copy/paste support for selected PDFs and folders within the library.
- Undo/redo support for library organization edits.
- Virtual Trash Can for PDFs and folder subtrees, with restore and permanent-delete flows.
- Missing-file tracking when watched files disappear from disk.
- Cached thumbnails stored under the XDG cache directory.

### Folders And Tags

- User-managed folders with nested parent/child relationships.
- Folder cards appear at the top of the scrollable library content.
- Breadcrumb navigation above the library content, starting from `Library`.
- Breadcrumbs are clickable and jump to parent folders.
- Dedicated library sidebar with separate `Files` and `Tags` tabs.
- `Files` tab uses a VS Code/Zed-inspired file tree:
  - `Library` root node.
  - Nested folder rows.
  - SVG chevron buttons for expand/collapse.
  - Folder-name click selects/navigates to that folder.
- `Tags` tab lists all known tags with PDF counts.
- Tag filters can be applied from the sidebar or tag pills on PDF cards/rows.
- Inline `+ tag` entry on PDFs.
- Sidebar tags can be renamed inline or deleted from every PDF that uses them.
- Bulk add/remove tag actions for selected PDFs.
- Bulk add/remove selected PDFs to/from the active folder.

### Selection And Metadata Tools

- Single and multi-select support in the library.
- Shift-click range selection.
- Ctrl-click toggle selection.
- Select all visible PDFs.
- Clear selection.
- Single-selection details panel with:
  - Thumbnail
  - Title
  - Author
  - Status
  - Page count
  - Reading progress
  - File size
  - Last opened date
  - Added date
  - File name
  - Folders
  - Tags
- Editable display title and author for a single selected PDF.
- Reset edited display metadata to extracted metadata.
- Bulk metadata maintenance:
  - Reset display metadata
  - Recompute title sort keys
  - Refresh extracted PDF metadata
  - Rebuild thumbnails
  - Reindex full text
  - Move selected PDFs to trash
- Confirmation dialogs for destructive or overwriting actions.

### Menus, Keyboard, And UI

- Application menu bar with File, Edit, View, Document, Library, Tools, and Help menus.
- Contextual selection toolbar for selected PDFs.
- Dark and light themes.
- Runtime KDL style reload support.
- Resizable library sidebar.
- Collapsible library/sidebar panels.
- Bundled IBM Plex Sans font family for consistent UI typography.
- Bundled Vollkorn font family for document- and book-like display typography.
- Native file and folder dialogs through `rfd`.
- Keyboard shortcuts for common actions including:
  - Undo/redo
  - Cut/copy/paste
  - Zoom in/out/reset
  - Toggle theme
  - Reload styles
  - Jump to page
  - Select all visible PDFs
  - Open selected PDF
  - Move selected PDFs to trash
  - Page up/down
  - Fine scroll
  - Horizontal pan
  - Escape to close overlays/panels

## Architecture

PDF-Folio is organized as a Rust workspace. The important boundary is that PDF/domain code does not depend on the app shell, reusable UI helpers do not depend on top-level app state, and `pdf-folio-ui` coordinates everything through `iced` messages and tasks.

```text
crates/
  iced-widget-patch/        Local patched iced_widget scrollable implementation
  pdf-folio-core/           PDF loading, rendering, text extraction, tile cache, annotations
  pdf-folio-db/             SQLite persistence, imports, folders/tags, search index, watcher
  pdf-folio-raindrop/       Raindrop.io OAuth/API/download/import integration
  pdf-folio-style/          KDL style book, tokens, classes, fonts, styled widget helpers
  pdf-folio-viewer/         Viewer domain state such as find/search and text selection
  pdf-folio-ui-components/  Reusable library UI logic and rendered component helpers
  pdf-folio-ui/             App shell, runtime state, update loop, views, tasks, menus
  pdf-folio-main/           CLI and binary entrypoint
```

### Crate Responsibilities

`pdf-folio-core` handles PDF functionality without UI or database dependencies:

- `PdfDoc` opens PDFs, renders pages, extracts text, reads metadata, and exposes outline nodes.
- `RenderedPage` stores RGBA page render output.
- `TileCache` stores rendered page tiles in a thread-safe LRU cache.
- Annotation types model highlights, notes, and drawings independently from the UI.

`pdf-folio-db` owns local library state and indexing:

- SQLite database access and schema.
- Recursive folder import.
- BLAKE3 content hashes for stable entry IDs.
- Folder and tag membership.
- Trash state for entries and folder subtrees.
- Library snapshots used by undo/redo and rollback flows.
- Sort, layout, sidebar, and tree preferences.
- Tantivy full-text page index.
- Filesystem watcher events for PDF create/modify/remove flows.

`pdf-folio-raindrop` owns Raindrop.io integration:

- OAuth browser sign-in and token caching.
- Optional `PDF_FOLIO_RAINDROP_TOKEN` bearer-token auth for testing or local use.
- Remote PDF preview loading, thumbnail fetching, and selective imports.
- Folder/collection mirroring, tag mirroring, rollback support for cancelled imports, and local downloaded-file storage.

`pdf-folio-style` owns the shared style system:

- Bundled KDL style files under `styles/`.
- User style override loading.
- Theme tokens, component classes, layout values, fonts, and styled widget helpers.
- Viewer-specific styling in `styles/components/viewer/viewer.kdl`.

`pdf-folio-viewer` owns viewer-domain data structures that do not need app orchestration:

- Viewer scroll/spread mode types.
- Rendered page view metadata.
- Find-in-document state and matching behavior.
- Text-selection anchors/ranges.

`pdf-folio-ui-components` owns reusable library UI logic and app-independent rendered controls:

- Library drag/drop geometry, filtering, selection helpers, metadata labels, and library UI state enums.
- Rendered component helpers in `src/library/view.rs`, including breadcrumb buttons, sort/metadata pickers, grid zoom, layout toggle, drop zones, tag rows, and preview placeholders.
- This crate can depend on `iced`, `pdf-folio-db`, `pdf-folio-core`, and `pdf-folio-style`, but it does not depend on `pdf-folio-ui`.

`pdf-folio-ui` is the app shell:

- Owns `PDFolioApp`, the top-level `Message` enum, update loop, app view, subscriptions, menus, and platform integrations.
- Coordinates cross-domain flows, such as opening a database entry in the viewer or saving library preferences after UI changes.
- Holds app-owned tasks for file dialogs, thumbnails, imports, metadata updates, viewer rendering, and search.

`pdf-folio-main` is intentionally small:

- Parses an optional startup PDF path.
- Initializes tracing.
- Calls `pdf_folio_ui::run`.

### App Shell Structure

The `pdf-folio-ui` crate uses `src/app.rs` as its library entrypoint, but the implementation is split into focused modules under `src/app/`:

```text
crates/pdf-folio-ui/src/
  app.rs                    App entrypoint, shared types, runtime state structs
  app/
    update.rs               Top-level message reducer
    update/shortcuts.rs     Shortcut side effects
    view.rs                 Top-level app shell view
    messages.rs             App menus, commands, Message, Shortcut
    menu.rs                 Application menu bar/dropdowns
    menu/selection.rs       Selection toolbar/dropdown helpers
    shortcuts.rs            Keyboard event to Message mapping
    subscriptions.rs        Window, style-watch, watcher, and animation subscriptions
    platform.rs             File-manager commands and file URI helpers
    libraries.rs            Named-library registry, switching, previews, create/rename/delete
    session.rs              Last-session load/save and restore adapters
    viewer_state.rs         Viewer document lifecycle, render/text tasks, find/selection state
    viewer_navigation.rs    Scrolling, paging, panning, and zoom navigation
    viewer_layout.rs        Viewer page grouping and layout math
    library_clipboard.rs    Library cut/copy/paste and undo/redo history helpers
    library_data.rs         Library refresh and thumbnail request coordination
    library_drag.rs         Library/folder drag lifecycle and autoscroll
    library_folders.rs      Folder tree helpers and breadcrumbs
    library_layout.rs       Library grid/list sizing and visible entry filtering
    library_selection.rs    Selection, range selection, details sync
    library_view_state.rs   Library viewport, hover, progress, and transient UI state
```

`PDFolioApp` is deliberately split into nested runtime structs:

```text
PDFolioApp
  mode: AppMode
  viewer: ViewerRuntime       Open document, render cache, zoom, scroll, find, outline
  library: LibraryRuntime     Entries, folders, filters, selection, thumbnails, drag state
  libraries: LibraryRegistryRuntime
                              Named library profiles, active library, previews, switcher state
  chrome: ChromeRuntime       Menus, flyouts, selection menu, confirmation modal
  appearance: AppearanceRuntime
                              Theme, loaded StyleBook, style load errors
  settings: Settings
  db: Arc<Db>
```

This keeps call sites explicit: viewer code reads `app.viewer.*`, library code reads `app.library.*`, menu/dialog state reads `app.chrome.*`, and theme/style state reads `app.appearance.*`.

### Library UI Structure

The library feature is split between reusable component code and app-owned adapters.

Reusable logic and widgets live in `pdf-folio-ui-components/src/library/`:

```text
drag.rs       Drag/drop state machines and hit testing
filters.rs    Folder/tag/search/reading-state filtering helpers
metadata.rs   Display labels, metadata cleanup, progress/file-size helpers
selection.rs  Selection/range/master-checkbox helpers
state.rs      Library UI enums such as metadata density and reading filters
view.rs       App-independent rendered library widgets and controls
```

The app-owned library surface lives under `pdf-folio-ui/src/library/`:

```text
tasks.rs             Async library/import/search/metadata tasks
thumbnails.rs        Thumbnail load/render task helpers
view.rs              Library root layout and app-specific adapter glue
view/dialogs.rs      Confirmation, create-folder, and bulk-progress UI
view/entries.rs      Library card/list-row rendering
view/folders.rs      Folder cards, folder drag previews, masonry/drop-zone helpers
view/sidebar.rs      Files/tags/details sidebar rendering
```

The remaining `pdf-folio-ui/src/library/view/*` modules still accept `&PDFolioApp` because they adapt app-owned data, app messages, thumbnail caches, and cross-domain commands. Reusable pieces that do not need `PDFolioApp` should live in `pdf-folio-ui-components`.

### Viewer UI Structure

Viewer-specific app rendering currently lives in `pdf-folio-ui/src/viewer/`:

```text
canvas.rs    Iced canvas drawing, hit testing, page images, text selection overlays
outline.rs   Viewer sidebar, outline tree, thumbnail strip, jump dialog
tasks.rs     Open/render/schedule viewer tasks
zoom.rs      Zoom input, preset menu, zoom control rendering
```

Viewer state and behavior shared outside the app shell lives in `pdf-folio-viewer/src/state.rs`. The app shell re-exports that state module through `pdf-folio-ui/src/viewer/mod.rs` and coordinates rendering/tasks through `PDFolioApp.viewer`.

### Styling Structure

Styles are KDL-backed and live in `pdf-folio-style`:

```text
styles/application.kdl
styles/themes/light.kdl
styles/themes/espresso.kdl
styles/components/core.kdl
styles/components/library/library.kdl
styles/components/library/sidebar.kdl
styles/components/viewer/viewer.kdl
```

Rust code should use style classes/tokens from `pdf-folio-style` instead of hard-coding visual values where practical. Viewer-specific visual styling belongs in `viewer.kdl`; behavior such as scroll math, render scheduling, text hit testing, and page layout stays in Rust.

### Dependency Direction

The intended dependency direction is:

```text
pdf-folio-main
  -> pdf-folio-ui
      -> pdf-folio-ui-components
      -> pdf-folio-viewer
      -> pdf-folio-raindrop
      -> pdf-folio-style
      -> pdf-folio-core
      -> pdf-folio-db

pdf-folio-ui-components -> pdf-folio-style, pdf-folio-core, pdf-folio-db, iced
pdf-folio-raindrop     -> pdf-folio-core, pdf-folio-db, reqwest, tokio
pdf-folio-viewer       -> pdf-folio-core, iced
pdf-folio-style        -> iced, kdl
pdf-folio-db           -> no UI crates
pdf-folio-core         -> no UI or database crates
```

Feature/component crates should not depend on `pdf-folio-ui`. Cross-domain workflows belong in the app shell.

## Data Locations

PDF-Folio uses XDG project directories with the application identity `dev/pdf-folio/PDF-Folio`.

- Library registry: XDG data directory, `libraries.json`
- Default library database: XDG data directory, `library.db`
- Additional library databases: XDG data directory, `libraries/<library-id>/library.db`
- Last app session: XDG data directory, `session.json`
- Tantivy search index: XDG data directory, `search-index/`
- Raindrop token cache: XDG data directory, `raindrop/token.json`
- Raindrop downloaded files: XDG data directory, `raindrop/<source-id>/files/`
- Thumbnail cache: XDG cache directory, `thumbs/`
- User KDL style overrides: XDG config directory, `pdf-folio/styles/`

Exact paths depend on the user's Linux environment.

## Build And Run

### Requirements

- Rust stable, edition 2021.
- A Linux desktop environment supported by `iced`/`wgpu`.
- Pdfium available as a system library, via `LD_LIBRARY_PATH`, or next to the binary.

If Pdfium cannot be found, PDF opening/rendering will fail with an initialization error.

### Build

```sh
cargo build
```

### Run The App

```sh
cargo run -p pdf-folio-main
```

### Open A PDF At Startup

```sh
cargo run -p pdf-folio-main -- /path/to/file.pdf
```

### Check The Workspace

```sh
cargo check
```

### Run Tests

```sh
cargo test
```

### Experimental Sync

PDF-Folio has an experimental single-user sync path. The desktop app now runs
automatic CRDT metadata sync after Google sign-in, immediately starts a sync
after local library/progress edits, and keeps a short periodic sync as a safety
net. The sync server runs on `mind-palace`, verifies Google sign-in for the
configured account, mints short-lived Turso/R2 credentials, and stores the
desktop session locally.

```sh
cargo run -p pdf-folio-main --bin pdf-folio -- sync health
cargo run -p pdf-folio-main --bin pdf-folio -- sync auth
cargo run -p pdf-folio-main --bin pdf-folio -- sync status
cargo run -p pdf-folio-main --bin pdf-folio -- sync ensure-schema
cargo run -p pdf-folio-main --bin pdf-folio -- sync sync-once
```

For a second-machine smoke test, `sync-once` uploads local PDF blobs, runs the
same CRDT metadata pass used by the UI auto-sync loop, and downloads pulled PDF
blobs into the local sync blob cache. That validates the auth flow, control
plane, Turso access, CRDT operation exchange, R2 blob transfer, and remote
library hydration. Synced library state includes PDFs, folders, folder
membership, user-edited display metadata, page count, reading position,
opened-at state, and tags.

## Project Notes

- The app is local-first. Experimental single-user CRDT sync hydrates remote
  libraries across devices; annotations/bookmarks are still evolving beyond the
  current persisted library metadata surface.
- Library trash and permanent-delete actions affect PDF-Folio metadata only; they do not delete source PDF files.
- Imported PDFs remain at their original paths.
- Folder membership is app metadata, separate from filesystem folders.
- Tags are stored per library entry.
- Full-text search depends on PDFs having been indexed.
- Some annotation infrastructure exists as data/model code; export and full annotation editing are still areas of ongoing development.

## License

The workspace package metadata declares:

```text
MIT OR Apache-2.0
```
