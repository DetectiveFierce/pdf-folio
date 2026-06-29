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

- Import a folder recursively and add all discovered PDF files.
- Content-based PDF IDs using BLAKE3 hashes.
- SQLite-backed library metadata store.
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
  - Delete selected PDFs from library metadata
- Confirmation dialogs for destructive or overwriting actions.

### Menus, Keyboard, And UI

- Application menu bar with File, Edit, View, Document, Library, Tools, and Help menus.
- Contextual selection toolbar for selected PDFs.
- Dark and light themes.
- Resizable library sidebar.
- Collapsible library/sidebar panels.
- Bundled Geist Mono Nerd Font Propo font family for consistent UI typography.
- Native file and folder dialogs through `rfd`.
- Keyboard shortcuts for common actions including:
  - Zoom in/out/reset
  - Toggle theme
  - Jump to page
  - Select all visible PDFs
  - Open selected PDF
  - Delete selected PDF metadata
  - Page up/down
  - Fine scroll
  - Horizontal pan
  - Escape to close overlays/panels

## Architecture

The workspace is split into four crates:

```text
crates/
  pdf-folio-core/     PDF loading, rendering, tile cache, annotations
  pdf-folio-library/  SQLite library store, imports, search index, watcher
  pdf-folio-ui/       iced application, views, messages, styling
  pdf-folio-main/     CLI and binary entrypoint
```

### `pdf-folio-core`

Handles PDF-specific functionality without depending on the UI:

- `PdfDoc` opens PDFs, renders pages, extracts page text, reads author metadata, and exposes outline nodes.
- `RenderedPage` stores RGBA page render output.
- `TileCache` stores rendered page tiles in a thread-safe LRU cache.
- Annotation types model highlights, notes, and drawings independently from the UI.

### `pdf-folio-library`

Owns local library state and indexing:

- SQLite database in the XDG data directory.
- Recursive PDF folder import.
- BLAKE3 content hashes for stable entry IDs.
- Folder and tag membership.
- Sort preferences and library layout preferences.
- Tantivy full-text page index.
- Filesystem watcher for PDF create/modify/remove events.
- Thumbnail cache path management in the XDG cache directory.

### `pdf-folio-ui`

Contains the application state machine and `iced` UI:

- Top-level library/viewer modes.
- Update loop and message routing.
- Library grid/list rendering and virtualization.
- Sidebar file tree and tag browser.
- Viewer canvas, page rendering, zoom, scrolling, and outline panel.
- Menu bar, selection toolbar, dialogs, and overlays.
- Shared style system under `src/style/`.

### `pdf-folio-main`

Provides the `pdf-folio` binary:

- Parses an optional startup PDF path.
- Initializes tracing.
- Launches the UI.

## Data Locations

PDF-Folio uses XDG project directories with the application identity `dev/pdf-folio/PDF-Folio`.

- Library database: XDG data directory, `library.db`
- Tantivy search index: XDG data directory, `search-index/`
- Thumbnail cache: XDG cache directory, `thumbs/`

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

## Project Notes

- The app is local-first. There is no cloud sync or remote account system.
- Library delete actions remove PDF-Folio metadata only; they do not delete source PDF files.
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

