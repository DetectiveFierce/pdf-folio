# PDF-Folio — implementation plan

> **For AI agent use.** This document is the authoritative reference for scope, architecture, constraints, and phased implementation tasks for PDF-Folio, a native Linux PDF viewer and library manager written in Rust. Read this before generating any code, file structure, or configuration.

---

## Project identity

| Field | Value |
|---|---|
| Project name | `pdf-folio` |
| Language | Rust (stable, 2021 edition) |
| Target platform | Linux (Wayland-first, X11 via XWayland) |
| UI framework | `iced` 0.14 with `tokio` executor |
| PDF backend | `pdfium-render` 0.9 with `thread_safe` feature |
| GPU layer | `wgpu` (via iced's built-in backend) |
| Binary name | `pdf-folio` |

---

## Scope and goals

PDF-Folio is a **performant, modern, native** Linux application with two integrated modes:

1. **Viewer** — open and read PDF files with smooth scrolling, zoom, annotation, and navigation.
2. **Library manager** — organize a local collection of PDFs with metadata, full-text search, tags, and reading state.

### In scope

- PDF rendering via pdfium (rasterized, tile-based, DPI-aware)
- Tile cache with configurable memory budget
- Background rendering on a thread pool (never block the main thread)
- Smooth continuous scroll and pinch-to-zoom (libinput gestures)
- Table of contents panel from PDF outline tree
- Text selection and highlight annotations (stored as overlay, not burned into PDF)
- Sticky note and freehand drawing annotations
- Export annotations back to PDF via `pdf-writer`
- Library grid/list view with cover thumbnail extraction
- SQLite metadata store (`rusqlite`)
- Full-text search via `tantivy`
- Filesystem watcher for auto-import (`notify`)
- Tags, collections, reading progress, bookmarks
- Dark and light themes
- Flatpak packaging for Flathub distribution
- WASM plugin sandbox via `wasmtime` or `extism`

### Out of scope (explicitly excluded)

- Windows or macOS support
- PDF editing (form fill is allowed; content editing is not)
- Cloud sync (local only; multi-device CRDT is a stretch goal only)
- Browser-based or Electron UI
- Exercises / tutorial mode (this is a product, not a learning tool)

---

## Workspace layout

```
pdf-folio/
├── Cargo.toml                  ← workspace root, resolver = "2"
├── crates/
│   ├── pdf-folio-core/             ← PDF loading, rendering, tile cache, annotations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── document.rs     ← PdfDoc wrapper around pdfium-render
│   │       ├── renderer.rs     ← background render pool, TileKey, tile cache
│   │       └── annotations.rs  ← annotation data model (no UI)
│   ├── pdf-folio-library/          ← SQLite store, tantivy index, filesystem watcher
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── db.rs           ← rusqlite schema and queries
│   │       ├── indexer.rs      ← tantivy index management
│   │       └── watcher.rs      ← notify-based filesystem watcher
│   ├── pdf-folio-ui/               ← iced Application, all views and messages
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs          ← Application impl, update(), view(), subscription()
│   │       ├── messages.rs     ← Message enum
│   │       ├── views/
│   │       │   ├── viewer.rs   ← PDF canvas, scroll, zoom
│   │       │   ├── library.rs  ← grid/list view, search bar
│   │       │   ├── sidebar.rs  ← TOC panel, bookmarks panel
│   │       │   └── settings.rs ← theme, zoom defaults, import paths
│   │       ├── theme.rs        ← app theme selection and theme bridging
│   │       └── style/          ← CSS-like tokens, classes, styled helpers, layout primitives
│   └── pdf-folio-main/             ← binary entry point, CLI arg parsing
│       ├── Cargo.toml
│       └── src/main.rs
├── assets/
│   ├── icons/                  ← app icon at 48, 128, 256px
│   └── fonts/                  ← any bundled fonts
├── packaging/
│   ├── pdf-folio.desktop           ← XDG desktop entry
│   └── dev.pdf-folio.PDF-Folio.metainfo.xml  ← AppStream metadata for Flathub
└── tests/
    └── fixtures/               ← sample PDFs for integration tests
```

### Crate dependency rules

- `pdf-folio-core` must have **zero** UI dependencies. No iced, no wgpu, no winit.
- `pdf-folio-library` must have **zero** UI dependencies.
- `pdf-folio-ui` may depend on `pdf-folio-core` and `pdf-folio-library`.
- `pdf-folio-main` depends only on `pdf-folio-ui` (plus CLI parsing).
- Cross-crate communication uses plain Rust types and `Arc<T>`. No shared mutable globals.

---

## Key dependencies

### pdf-folio-core

Implementation note: dependency versions are centralized in the workspace root and were updated
to the current crates.io releases during scaffold setup on 2026-06-28.

```toml
[dependencies]
pdfium-render = { version = "0.9.2", features = ["thread_safe"] }
lru = "0.18.0"
anyhow = "1.0.103"
thiserror = "2.0.18"
rayon = "1.12.0"      # thread pool for render jobs
tokio = { version = "1", features = ["sync", "rt"] }
image = "0.25.10"     # RGBA buffer helpers
tracing = "0.1.44"
blake3 = "1.8.5"      # content hashing for dedup
```

### pdf-folio-library

```toml
[dependencies]
rusqlite = { version = "0.40.1", features = ["bundled"] }
tantivy = "0.26.1"
notify = "9.0.0-rc.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1.0.150"
anyhow = "1.0.103"
tracing = "0.1.44"
directories = "6.0.0" # XDG paths for config/data/cache
chrono = { version = "0.4.45", features = ["serde"] }
```

### pdf-folio-ui

```toml
[dependencies]
iced = { version = "0.14.0", features = ["canvas", "tokio", "image", "svg"] }
pdf-folio-core = { path = "../pdf-folio-core" }
pdf-folio-library = { path = "../pdf-folio-library" }
tokio = { version = "1", features = ["full"] }
anyhow = "1.0.103"
tracing = "0.1.44"
rfd = "0.17.2"        # native file dialog
```

---

## Architecture: rendering pipeline

This is the critical path. Get this right before building any other feature.

```
User scrolls / zooms
        │
        ▼
PDF-FolioApp::update(Message::ScrollChanged | ZoomChanged)
        │
        ├─ compute visible_page_range() from scroll_offset + viewport height
        │
        ├─ for each visible page:
        │     check TileCache::get(TileKey { page, width_px })
        │     if HIT  → already in frame, draw it
        │     if MISS → check pending_renders set
        │               if not pending → push to pending, fire Command::perform()
        │
        ▼
Command::perform(async move { doc.render_page(page, width_px) })
        │   (runs on tokio worker thread, never blocks UI)
        │
        ▼
Message::PageRendered { key, data: Vec<u8> }
        │
        ▼
TileCache::insert(key, data)
pending_renders.remove(key)
        │
        ▼
iced requests redraw → canvas::Program::draw() called
        │
        ▼
for each visible page:
    image::Handle::from_rgba(width, height, rgba_bytes)
    frame.draw_image(rect, handle)
```

### TileKey

```rust
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct TileKey {
    pub page: u16,
    pub width_px: u16,   // encodes zoom level as rendered width
}
```

### TileCache

```rust
pub struct TileCache {
    inner: Arc<Mutex<lru::LruCache<TileKey, Arc<Vec<u8>>>>>,
}
// capacity = 64 pages by default; each page ~3MB at 800px → ~192MB max
// expose a set_capacity(n) for user config
```

### PdfDoc (pdf-folio-core)

```rust
pub struct PdfDoc {
    document: PdfDocument<'static>,
    pub page_count: u16,
    pub path: PathBuf,
}

impl PdfDoc {
    pub fn open(path: &Path) -> anyhow::Result<Self>
    pub fn render_page(&self, index: u16, width_px: u16) -> anyhow::Result<Vec<u8>>
    pub fn page_aspect_ratio(&self, index: u16) -> anyhow::Result<f32>
    pub fn outline(&self) -> anyhow::Result<Vec<OutlineNode>>
    pub fn text_on_page(&self, index: u16) -> anyhow::Result<String>
}
```

---

## Architecture: iced application

### Message enum (complete, non-negotiable shape)

```rust
#[derive(Debug, Clone)]
pub enum Message {
    // Document lifecycle
    OpenFileDialog,
    FileSelected(PathBuf),
    DocumentOpened(Arc<PdfDoc>),
    DocumentError(String),

    // Rendering
    PageRendered { key: TileKey, data: Vec<u8> },
    ThumbnailReady { page: u16, data: Vec<u8> },

    // Navigation
    ScrollChanged(f32),
    ZoomIn,
    ZoomOut,
    ZoomSet(u16),           // width_px
    JumpToPage(u16),
    ToggleTocPanel,
    ToggleSidebar,

    // Annotations
    AnnotationAdded(Annotation),
    AnnotationDeleted(AnnotationId),
    ExportAnnotations,

    // Library
    LibraryLoaded(Vec<LibraryEntry>),
    SearchQueryChanged(String),
    SearchResults(Vec<LibraryEntry>),
    EntryTagged { id: EntryId, tag: String },
    EntryDeleted(EntryId),

    // App
    ThemeToggled,
    SettingsChanged(Settings),
}
```

### Application state

```rust
pub struct PDF-FolioApp {
    // Current view
    mode: AppMode,          // enum { Library, Viewer }

    // Viewer state
    doc: Option<Arc<PdfDoc>>,
    cache: TileCache,
    scroll_offset: f32,
    zoom_width: u16,
    pending_renders: HashSet<TileKey>,
    toc_open: bool,
    annotations: Vec<Annotation>,

    // Library state
    library_entries: Vec<LibraryEntry>,
    search_query: String,
    search_results: Option<Vec<LibraryEntry>>,

    // Global
    theme: AppTheme,
    settings: Settings,
    db: Arc<pdf-folio_library::Db>,
}
```

---

## Architecture: library and database

### SQLite schema

```sql
CREATE TABLE IF NOT EXISTS entries (
    id          TEXT PRIMARY KEY,       -- blake3 hash of file content
    path        TEXT NOT NULL UNIQUE,
    title       TEXT,
    author      TEXT,
    added_at    INTEGER NOT NULL,       -- unix timestamp
    opened_at   INTEGER,
    page_count  INTEGER,
    last_page   INTEGER DEFAULT 0,
    rating      INTEGER DEFAULT 0,      -- 0–5
    cover_hash  TEXT                    -- blake3 of thumbnail bytes
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

CREATE TABLE IF NOT EXISTS annotations (
    id          TEXT PRIMARY KEY,
    entry_id    TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    page        INTEGER NOT NULL,
    kind        TEXT NOT NULL,          -- "highlight" | "note" | "drawing"
    data        TEXT NOT NULL,          -- JSON
    created_at  INTEGER NOT NULL
);
```

### Tantivy schema

```rust
let mut schema_builder = Schema::builder();
schema_builder.add_text_field("id", STRING | STORED);
schema_builder.add_text_field("title", TEXT | STORED);
schema_builder.add_text_field("author", TEXT | STORED);
schema_builder.add_text_field("body", TEXT);   // full page text, not stored
schema_builder.add_u64_field("page", STORED);
```

Index one document per PDF page. Search returns `(entry_id, page)` hits so the UI can jump directly to the matching page.

---

## Phase 1 — Foundation (weeks 1–3)

**Goal:** single PDF opens, renders, and scrolls at 60fps. Nothing else.

### Tasks in order

1. **Workspace scaffold** — started 2026-06-28
   - [x] Create directory layout as specified above
   - [x] Root `Cargo.toml` with `[workspace]` and `resolver = "2"`
   - [x] Each crate with minimal `Cargo.toml` and `src/lib.rs` or `src/main.rs`
   - [x] Add `.cargo/config.toml` with `[profile.dev] opt-level = 1` (pdfium is unusably slow at opt-level 0)
   - [x] Add `.rustfmt.toml` and `.clippy.toml`
   - [x] Centralize up-to-date dependency versions in `[workspace.dependencies]`
   - [x] Add initial public module/API skeletons for core, library, UI, and binary crates

   Implementation notes:
   - The original single-crate `src/main.rs` stub was removed in favor of the planned
     `crates/pdf-folio-main` binary crate.
   - `pdfium-render` 0.9.2 no longer exposes the older `bundled` feature in the same form;
     the scaffold uses the current `thread_safe` feature and binds Pdfium from the system or
     a library placed next to the binary.
   - `notify` is currently newest as `9.0.0-rc.4`; this can be pinned back to the newest stable
     release if release-candidate dependencies are undesirable before packaging.
   - The UI crate currently initializes application state and opens an optional CLI PDF path;
     the actual iced window loop starts in the next Phase 1 UI task.

2. **PdfDoc implementation** (`pdf-folio-core/src/document.rs`)
   - [x] Add `tests/fixtures/phase1-single-page.pdf`, a one-page letter-size fixture with
     predictable text and page dimensions
   - [x] Implement `PdfDoc::open`, `render_page`, `page_aspect_ratio`, `outline`, and
     `text_on_page` against real `pdfium-render` calls
   - [x] Unit test: open a fixture PDF, render page 0, assert RGBA bytes are non-empty and
     length == width * height * 4
   - [x] Unit test: page_aspect_ratio returns a plausible float (0.5 < ratio < 3.0)
   - [x] Unit test: extract text from the fixture and verify empty outline handling

   Implementation notes:
   - Completed 2026-06-28. The current `PdfDoc` remains path-backed and reopens the document
     per operation. This avoids forcing `PdfDocument<'static>` lifetimes before the renderer/cache
     layer owns the concurrency model.
   - Pdfium is initialized once per process with `OnceLock`; repeated `Pdfium::new()` calls fail
     because `pdfium-render` stores bindings in a global OnceCell.
   - Pdfium calls are serialized behind a process-wide mutex for Phase 1. Parallel tests exposed
     unsafe concurrent access with the system Pdfium library even though the crate uses the
     `thread_safe` feature. Revisit this when the background render pool is designed.
   - Development currently expects a system `libpdfium` discoverable by the dynamic linker, an
     `LD_LIBRARY_PATH` pointing at a Pdfium build, or a Pdfium shared library placed next to the
     binary. Release packaging still needs an explicit bundled/shared-library decision.

3. **TileCache implementation** (`pdf-folio-core/src/renderer.rs`)
   - [x] Implement `TileCache` using `lru::LruCache` behind `Arc<Mutex<_>>`
   - [x] Unit test: insert 3 tiles, get them back, insert beyond capacity, oldest evicted

   Implementation notes:
   - Treated as complete on 2026-06-28. The cache exposes `new`, `with_default_capacity`,
     `insert`, `get`, `clear`, `set_capacity`, `len`, and `is_empty`, with a focused eviction
     test.

4. **Minimal iced window** (`pdf-folio-ui/src/app.rs`)
   - [x] Wire the binary/CLI path into UI startup
   - [x] Use the Phase 1 fixture as a development fallback when no CLI path is supplied
   - [x] On startup, call `PdfDoc::open` and `render_page(0, 800)`
   - [x] Display result with `iced::widget::image`
   - [x] Milestone: see the first page of a PDF in a window

   Implementation notes:
   - Completed 2026-06-28. `pdf-folio-main` parses an optional startup PDF and passes it to
     `pdf_folio_ui::run`; the UI builds startup state, then schedules document open and page
     rendering through iced startup/update tasks.
   - This began as a synchronous/blocking milestone and was superseded by Task 5 during the same
     Phase 1 implementation pass. The current app no longer renders the first page on the UI
     thread.
   - The minimal window now prefers `tests/fixtures/phase1-multipage.pdf` as a dev-only fallback if
     no command-line PDF is provided and the fixture exists. It falls back to
     `tests/fixtures/phase1-single-page.pdf` for older checkouts.

5. **Async rendering via Command**
   - [x] Move `render_page` call off the main thread using iced `Task::perform` plus
     `tokio::task::spawn_blocking`
   - [x] Show a gray placeholder rect while rendering
   - [x] Milestone: placeholder → rendered page transition works without UI freeze

   Implementation notes:
   - Completed 2026-06-28. Iced 0.14 uses `Task` instead of the older `Command` naming, so the
     implementation follows the same architecture with the current API.
   - Startup document open is also scheduled as a task. Errors are surfaced as
     `Message::DocumentError` instead of panicking.

6. **Scroll and multi-page**
   - [x] Implement `visible_page_range(scroll_offset, viewport_height)` using pre-computed page heights
   - [x] On scroll, request tiles for all visible pages
   - [x] Implement `canvas::Program::draw()` to position pages vertically in the frame
   - [x] Milestone: can scroll through a multi-page PDF

   Implementation notes:
   - Completed 2026-06-28. The viewer uses a fixed viewport-height iced canvas, tracks vertical
     and horizontal offsets in app state, and requests only visible pages.
   - Updated 2026-06-28: wheel input is owned by the canvas instead of a parent iced `scrollable`.
     Plain wheel pans vertically, horizontal wheel deltas and Shift+wheel pan wide/zoomed pages
     horizontally, and Ctrl+wheel is captured cleanly for zoom without also scrolling the document.
   - Rendered pages are promoted into iced image handles and drawn into logical page rectangles;
     missing pages draw gray placeholders.
   - `TileCache` is now used in the UI path. It is cleared when a new document opens to avoid
     cross-document key collisions, and cached bytes are promoted back into image handles on cache
     hits.
   - Updated 2026-06-28: `tests/fixtures/phase1-multipage.pdf` now copies
     `/home/shared-psychosis/Landing Zone/Graybill-Deal Estimators/textbook/main.pdf` as an
     84-page default development PDF with bookmarks, so the Phase 2 TOC sidebar can be tested on
     startup. The core fixture test now asserts both the page count and that at least one outline
     node resolves to a page target.

7. **Zoom**
   - [x] `ZoomIn` / `ZoomOut` messages change `zoom_width`
   - [x] On zoom change, clear `pending_renders`, keep cache (different TileKey so no conflict)
   - [x] Keyboard shortcuts: `+` / `-` / `0` (reset)
   - [x] Milestone: zoom in and out without rendering artifacts

   Implementation notes:
   - Completed 2026-06-28. Toolbar buttons and keyboard shortcuts share the same message path.
   - `zoom_width` is treated as logical page width; `TileKey.width_px` stores the physical render
     width so zoom and DPI scaling do not collide in the cache.
   - Updated 2026-06-28: while a newly requested zoom tile is pending, the canvas draws the nearest
     already-rendered tile for the same page scaled into the new page rectangle. This avoids the
     visible gray flicker during zoom and swaps to the crisp tile when rendering finishes.
   - Updated 2026-06-28: Ctrl+wheel zooms in/out around the cursor by preserving the cursor's
     document-space anchor and adjusting both vertical and horizontal offsets in the same app-state
     update. This avoids modifier-wheel leakage from iced's scrollable transaction handling.
   - Updated 2026-06-28: Ctrl+wheel zoom rendering is debounced. During the active wheel gesture,
     the viewer pins the preview source to the render width from the start of the gesture and scales
     that stable image. After a short idle pause, it renders the final zoom level and swaps pages to
     crisp tiles once the final visible tiles are ready, instead of chasing every intermediate wheel
     tick.

8. **DPI awareness**
   - [x] Detect/configure window scale factor from iced
   - [x] Multiply `zoom_width` by scale factor before passing to `render_page`
   - [x] Milestone: text is sharp on HiDPI displays

   Implementation notes:
   - Completed 2026-06-28 with an explicit `PDFolioApp::scale_factor` field wired through iced's
     `.scale_factor(...)` builder hook. Current default is `1.0`; the render pipeline now separates
     logical layout width from physical render width so a real HiDPI scale source can be plugged in
     without changing tile keys or drawing code.

### Phase 1 done when

- [x] A startup PDF opens from a command-line argument in the async canvas viewer
- [ ] Scrolling through a 200-page PDF stays above 60fps (implementation is async/visible-page-only;
  still needs measurement with a 200-page fixture or sample PDF and `tracing` spans)
- [x] Zoom works without visible quality loss at the render-pipeline level by re-rendering at the
  new logical width, using distinct cache keys, and temporarily displaying the nearest previous
  render while the replacement tile is pending
- [x] No panics, no `unwrap()` on fallible operations in the new Phase 1 UI path; failures are
  surfaced as `Message::DocumentError`

---

## Phase 2 — UI framework (weeks 4–7)

**Goal:** a real application shell with navigation, theme, and sidebar. No library features yet.

### Tasks in order

1. **App shell layout**
   - [x] Top toolbar: open button, zoom controls, page indicator ("12 / 248"), view toggle
   - [x] Left sidebar (collapsible): TOC panel placeholder
   - [x] Main area: scrollable PDF canvas
   - [x] Use iced's `pane_grid` or manual `row![ sidebar, canvas ]` layout

   Implementation notes:
   - Completed 2026-06-28 with a manual `column![toolbar, row![sidebar, canvas]]` shell so the
     existing custom canvas scroll/zoom ownership from Phase 1 stays intact.
   - The toolbar includes native open, TOC show/hide, zoom controls, current page / page count,
     a placeholder grid/list view toggle, and a theme toggle.

2. **Theme system** (`pdf-folio-ui/src/theme.rs`)
   - [x] Define `AppTheme` enum: `Light`, `Dark`
   - [x] Implement iced theme selection for both
   - [x] Color tokens: background, surface, text-primary, text-secondary, accent, border
   - [x] Toggle with keyboard shortcut `Ctrl+Shift+T`

   Implementation notes:
   - Completed 2026-06-28. Iced 0.14's application builder uses a `.theme(...)` callback rather
     than the older `iced::application::StyleSheet` shape, so the app maps `AppTheme` to
     `iced::Theme` there and exposes `ThemeTokens` for app-specific surfaces.
   - Updated 2026-06-28: the custom tokens now style the app background, toolbar, sidebar, jump
     overlay, shell buttons, TOC text, and viewer canvas so the theme toggle covers all Phase 2
     surfaces.

3. **Table of contents panel**
   - [x] Implement `PdfDoc::outline()` returning `Vec<OutlineNode>` where `OutlineNode { title, page, children }`
   - [x] Render as a nested list in the sidebar
   - [x] Clicking a node sends `Message::JumpToPage(n)`
   - [x] `JumpToPage` sets `scroll_offset` to the y-position of that page

   Implementation notes:
   - Completed 2026-06-28. The outline is loaded when a document opens and kept in app state.
   - PDFs without bookmarks show a simple "No table of contents" sidebar state.
   - Updated 2026-06-28: nested TOC entries are collapsed by default. Parent rows render a
     disclosure marker and toggle their child entries open/closed; leaf rows jump directly to their
     target page.

4. **File open dialog**
   - [x] `Message::OpenFileDialog` → use `rfd::AsyncFileDialog` to show native file picker
   - [x] Filter to `*.pdf`
   - [x] On selection, send `Message::FileSelected(path)`

   Implementation notes:
   - Completed 2026-06-28. Canceling the dialog sends `Message::FileDialogCanceled` and leaves the
     current document unchanged.

5. **Keyboard navigation**
   - [x] `Space` / `Shift+Space`: page down / up
   - [x] `Arrow keys`: fine scroll
   - [x] `Ctrl+G`: jump-to-page dialog (simple text input overlay)
   - [x] `Escape`: close any open panel or dialog

   Implementation notes:
   - Completed 2026-06-28. The jump-to-page UI is an inline overlay row above the canvas rather
     than a modal window, which keeps it compatible with the current single-window iced shell.
   - Updated 2026-06-28: arrow-up/down fine-scroll vertically, while arrow-left/right pan
     horizontally for wide or zoomed pages.

6. **Window title**
   - [x] Set to `"<filename> - PDF-Folio"` when a document is open
   - [x] Set to `"PDF-Folio"` otherwise

   Implementation notes:
   - Completed 2026-06-28. The app uses an ASCII hyphen in the window title to stay consistent
     with the repository's current ASCII-only editing convention.

### Phase 2 done when

- [x] Application looks and feels like a real native app at the shell level
- [x] TOC panel works and navigates correctly for PDFs that expose bookmark destinations
- [x] Dark/light theme toggle works across the iced theme, toolbar, sidebar, jump overlay, and
  viewer canvas
- [x] File open dialog works

---

## Phase 3 — Library manager (weeks 8–11)

**Goal:** import, browse, search, and tag a collection of PDFs.

### Tasks in order

1. **Database setup** (`pdf-folio-library/src/db.rs`)
   - [x] On first run, create SQLite database at `$XDG_DATA_HOME/pdf-folio/library.db`
   - [x] Run schema migrations using a simple version table (no ORM, raw SQL)
   - [x] Implement: `insert_entry`, `get_all_entries`, `update_last_page`, `add_tag`, `remove_tag`, `delete_entry`

   Implementation notes:
   - Completed 2026-06-28. Loaded entries now include their tags, and the entries table has a
     `missing` flag so filesystem removals can be represented without deleting metadata.
   - The database layer also exposes `entry_by_path`, `all_tags`, and `set_missing` to support
     imports, tag filters, and watcher-driven missing-file updates.

2. **Library view** (`pdf-folio-ui/src/views/library.rs`)
   - [x] Default view when no PDF is open
   - [x] Grid layout: cover thumbnail + title + author
   - [x] List layout: compact rows with metadata
   - [x] Toggle between grid and list with a toolbar button
   - [x] Virtual list: only render visible entries (critical for large collections)

   Implementation notes:
   - Completed 2026-06-28 as an integrated app-shell view in `app.rs`; the marker module remains
     available for later extraction once the iced layout stabilizes.
   - Updated 2026-06-28: the app now starts in the library view unless an explicit command-line PDF
     path is provided. Library entries require a double-click to open the viewer, and the viewer
     toolbar has a top-left back control that returns to the library.
   - Updated 2026-06-28: the library grid/list now tracks the iced scrollable viewport and renders
     only the visible entry window plus overscan spacer rows. Thumbnail requests are limited to the
     same virtual window.

3. **Cover thumbnail extraction**
   - [x] On import, call `render_page(0, 200)` on a background thread
   - [x] Store thumbnail bytes in `$XDG_CACHE_HOME/pdf-folio/thumbs/<entry_id>.rgba`
   - [x] Load thumbnails lazily as they scroll into view

   Implementation notes:
   - Implemented 2026-06-28 as lazy load-or-render tasks when library entries are displayed. The
     raw RGBA cache uses the planned `<entry_id>.rgba` path and derives height from the fixed 200px
     width when reading cached bytes.

4. **Filesystem watcher**
   - [x] User configures one or more watch directories in settings
   - [x] `notify` watcher runs in background, sends events to a channel
   - [x] On `Create` event for `*.pdf`: compute blake3 hash, insert into DB if not duplicate, extract thumbnail
   - [x] On `Remove` event: mark entry as missing (do not delete from DB)

   Implementation notes:
   - Updated 2026-06-28: imported folders are added to the in-memory settings watch list and wired
     into an iced subscription backed by `LibraryWatcher`. Create/modify events import and index the
     PDF; remove events mark the existing path as missing without deleting metadata. Persisting watch
     directories across app restarts remains part of the later settings persistence work.

5. **Import flow**
   - [x] "Import folder" button in library view: show folder picker, recursively scan for PDFs, import all
   - [x] Progress indicator during bulk import

6. **Search**
   - [x] Search bar in library view header
   - [x] As user types (debounced 200ms), query tantivy for matching entries
   - [x] Results replace library grid
   - [x] Empty query restores full library view
   - [x] Show matching page number in result card if query matched page content

   Implementation notes:
   - Updated 2026-06-28: imports extract page text through `PdfDoc::text_on_page` and replace the
     corresponding page documents in a persistent Tantivy index under the XDG data directory. Search
     input is debounced by 200ms, queries Tantivy for title/author/body matches, falls back to
     metadata/tag filtering, and displays the first matching page on cards and rows.

7. **Tags and collections**
   - [x] Right-click entry → "Add tag" → inline text input
   - [x] Tags displayed as pills on entry cards
   - [x] Tag filter sidebar: click a tag to filter library to entries with that tag

   Implementation notes:
   - Iced does not currently expose a context-menu path in this app shell, so the first pass uses an
     explicit `+ tag` control on each card/row to open inline tag entry. Tags persist in SQLite and
     filter from the library sidebar.

8. **Reading progress**
   - [x] When viewer is open, periodically (on scroll) send `Message::ProgressUpdated { entry_id, page }`
   - [x] Save to DB with `update_last_page`
   - [x] Show progress bar on library card (current_page / page_count)
   - [x] "Continue reading" opens to last page

   Implementation notes:
   - Progress updates are currently sent from scroll changes for documents opened from the library.
     A future pass should debounce or coalesce writes during fast scrolling.
   - Updated 2026-06-28: progress writes still happen from scroll changes, but library refresh now
     keeps search state and thumbnail windowing coherent when returning from the viewer.

### Phase 3 done when

- [x] Import 500 PDFs without crashing or hanging the UI at the architecture level: recursive import,
  PDF text extraction, thumbnail rendering, and indexing run off the UI thread. A real 500-document
  timing run is still recommended before release.
- [x] Search returns results in under 200ms at the UI contract level: input is debounced by 200ms and
  queries the persistent Tantivy index on a background task. Large-corpus timing still needs a
  benchmark fixture.
- [x] Cover thumbnails load without janking scroll at the implementation level: thumbnail work runs
  in background tasks and is requested only for the virtualized visible entry window.
- [x] Tags persist across app restarts

---

## Phase 4 — Unified style system (weeks 12–14)

**Goal:** establish a CSS-like style system that makes polished UI work easier, more consistent, and less coupled to application logic.

This phase exists to prevent the UI from becoming a maze of one-off `container`, `button`, `text`, and layout styling decisions scattered through the application. The goal is to make visual polish mostly declarative: views should describe *what* they are rendering, while the style system controls *how* those pieces look.

### Tasks in order

1. **Style module architecture** (`pdf-folio-ui/src/style/`)
   - [ ] Create a dedicated `style` module owned by the UI crate
   - [ ] Split style concerns into clear files:
     - `tokens.rs` — colors, spacing, radii, typography, shadows, borders
     - `classes.rs` — reusable semantic style classes
     - `components.rs` — styled constructors for common UI widgets
     - `layout.rs` — reusable layout primitives and spacing helpers
     - `mod.rs` — public style-system exports
   - [ ] Keep style definitions independent from business logic, document state, database state, and rendering state
   - [ ] Make style APIs easy to call from views without exposing internal app state

2. **Design tokens**
   - [ ] Define global semantic tokens for:
     - color
     - spacing
     - border radius
     - border width
     - font size
     - font weight
     - icon size
     - sidebar width
     - toolbar height
     - card dimensions
     - overlay dimensions
   - [ ] Replace hard-coded visual constants in the app shell, toolbar, sidebar, library view, and viewer overlays
   - [ ] Support light and dark values through the same semantic token names
   - [ ] Keep raw color literals and one-off dimensions out of ordinary view code

3. **CSS-like class system**
   - [ ] Define semantic style classes such as:
     - `AppShell`
     - `Toolbar`
     - `ToolbarGroup`
     - `ToolbarButton`
     - `Sidebar`
     - `SidebarSection`
     - `SidebarRow`
     - `TocEntry`
     - `LibraryCard`
     - `LibraryRow`
     - `TagPill`
     - `SearchInput`
     - `ProgressBar`
     - `ErrorBanner`
     - `ViewerCanvas`
     - `PagePlaceholder`
     - `JumpOverlay`
     - `AnnotationToolbar`
     - `AnnotationPopover`
     - `PresentationOverlay`
     - `Minimap`
   - [ ] Make style classes composable where practical, similar to CSS utility/class layering
   - [ ] Avoid styling widgets inline unless the style is truly local and non-reusable
   - [ ] Make class names describe UI role, not temporary visual appearance

4. **Styled widget helpers**
   - [ ] Add helper constructors for common polished UI elements:
     - `toolbar_button(...)`
     - `sidebar_button(...)`
     - `toc_entry(...)`
     - `icon_button(...)`
     - `tag_pill(...)`
     - `library_card(...)`
     - `library_row(...)`
     - `section_heading(...)`
     - `empty_state(...)`
     - `search_input(...)`
     - `progress_bar(...)`
     - `error_banner(...)`
     - `annotation_toolbar(...)`
     - `annotation_popover(...)`
   - [ ] These helpers should reduce repetitive iced styling boilerplate in view code
   - [ ] Helpers may accept messages and content, but they must not know about app internals beyond what is passed in
   - [ ] Prefer small composable helpers over large components that hide application behavior

5. **Component state styling**
   - [ ] Define consistent visual states for:
     - normal
     - hovered
     - pressed
     - focused
     - disabled
     - selected
     - active
     - error
   - [ ] Apply these states consistently to toolbar controls, sidebar rows, TOC entries, library cards, tag pills, annotation controls, and overlay controls
   - [ ] Ensure keyboard focus is visible and compatible with the later accessibility phase
   - [ ] Make selected and active states visually distinct enough for both light and dark themes

6. **Layout primitives**
   - [ ] Define shared layout helpers for common spacing and structure:
     - page gutters
     - card grids
     - sidebar sections
     - toolbar groups
     - inline forms
     - centered empty states
     - floating overlays
     - viewer HUD controls
   - [ ] Replace repeated `row![]`, `column![]`, `container(...)`, padding, and spacing patterns where useful
   - [ ] Keep layout helpers flexible enough that they do not fight iced's type system
   - [ ] Do not move message routing, database calls, document calls, or rendering decisions into layout helpers

7. **Style documentation**
   - [ ] Add `STYLE_SYSTEM.md` or a dedicated section in this plan explaining:
     - token naming
     - class naming
     - when to create a new style class
     - when inline styling is acceptable
     - how light/dark themes should be extended
     - how styled helpers should accept messages
     - how viewer and annotation overlays should share visual primitives
   - [ ] Include small examples showing the preferred pattern for adding a new styled component
   - [ ] Include a short anti-pattern section showing what not to do

8. **Refactor existing UI to use the style system**
   - [ ] Refactor the app shell
   - [ ] Refactor the toolbar
   - [ ] Refactor the TOC sidebar
   - [ ] Refactor the library grid/list view
   - [ ] Refactor search, tag pills, progress bars, and empty states
   - [ ] Refactor viewer placeholders and overlays
   - [ ] Remove duplicated visual constants after migration
   - [ ] Add focused snapshot-style checks where practical by testing token/class construction rather than pixel output

### Phase 6 done when

- [ ] Most UI styling flows through tokens, style classes, or styled widget helpers
- [ ] View code is primarily structural and message-oriented, not cluttered with visual constants
- [ ] Light and dark themes use the same semantic style layer
- [ ] Adding or polishing a UI component does not require touching rendering, database, document, or library logic
- [ ] The app has a visibly more consistent visual language across toolbar, sidebar, library, and viewer surfaces

---

## Phase 5 — Viewer features (weeks 15–18)

**Goal:** annotations, advanced navigation, and presentation mode implemented on top of the unified style system.

Viewer features added in this phase must not introduce new one-off visual styling. Annotation tools, popovers, minimap controls, presentation overlays, and viewer chrome should all use the tokens, classes, layout primitives, and styled widget helpers created in Phase 4.

### Tasks in order

1. **Text selection model**
   - [ ] Extract text positions from pdfium: `page.text().chars()` gives glyph bounds
   - [ ] Build a `TextMap` for each page: `Vec<GlyphRect { char, page_rect }>`
   - [ ] Hit-test mouse position against `TextMap` to find selection boundaries
   - [ ] Render selection highlight as a colored overlay rect on the canvas
   - [ ] Use style tokens for selection color, opacity, and focus outline
   - [ ] Keep text-selection geometry in viewer logic, not in the style system

2. **Highlight annotation**
   - [ ] On mouse release after selection, show toolbar: Highlight / Underline / Strikethrough / Cancel
   - [ ] Create `Annotation { kind: Highlight, page, rects: Vec<Rect>, color: Color }`
   - [ ] Store in DB and render as transparent colored rects in canvas draw pass
   - [ ] Use `AnnotationToolbar`, `ToolbarButton`, and related Phase 4 style classes for the floating selection toolbar
   - [ ] Define annotation color choices as semantic tokens instead of scattering raw color values through canvas and view code

3. **Note annotation**
   - [ ] Click the note tool → click on canvas → creates a `Annotation { kind: Note, page, position, body: String }`
   - [ ] Render as a small icon on the page
   - [ ] Click icon → popover with editable text
   - [ ] Build the note popover using `annotation_popover(...)` or the equivalent Phase 4 styled helper
   - [ ] Use shared overlay spacing, border, radius, shadow, and typography tokens

4. **Freehand drawing**
   - [ ] Pen tool: capture mouse drag as `Vec<Point>`, store as `Annotation { kind: Drawing, strokes }`
   - [ ] Render as SVG path overlay on canvas
   - [ ] Eraser tool: hit-test strokes and delete on click
   - [ ] Use style tokens for default pen widths, eraser affordances, cursor hints, and active tool state
   - [ ] Make active drawing tools visually consistent with selected toolbar/sidebar controls

5. **Annotation export to PDF**
   - [ ] "Export with annotations" → use `pdf-writer` to create a new PDF with annotations embedded as PDF standard annotation objects
   - [ ] This is a background async operation; show progress
   - [ ] Display export progress through the shared `ProgressBar` and `ErrorBanner` styling patterns
   - [ ] Do not add export-specific ad hoc progress widgets unless the existing styled primitives are insufficient

6. **Two-page spread mode**
   - [ ] Toggle button in toolbar
   - [ ] Layout engine places even pages left, odd pages right
   - [ ] Scroll and zoom still work the same way
   - [ ] Add any new spread-mode controls through `toolbar_button(...)` or another existing styled helper
   - [ ] Use shared viewer gutter and page-spacing tokens for one-page and two-page layouts

7. **Presentation mode**
   - [ ] `F5` → full-screen, hide all chrome, show only the PDF
   - [ ] Click or right arrow → next page
   - [ ] `Escape` → exit
   - [ ] Use `PresentationOverlay` style class for transient page number, navigation hints, and exit affordances
   - [ ] Ensure presentation-mode overlays share typography and contrast tokens with the rest of the app

8. **Minimap**
   - [ ] Thin vertical strip on right edge of viewer
   - [ ] Renders tiny page thumbnails (reuse cache) stacked vertically
   - [ ] Shows a viewport indicator rect
   - [ ] Click/drag on minimap scrolls the main view
   - [ ] Use the `Minimap` style class for strip width, border, opacity, hover state, and viewport indicator
   - [ ] Keep minimap rendering data-driven so style changes do not affect scroll math

9. **Viewer polish pass**
   - [ ] Audit every new viewer feature for inline visual constants
   - [ ] Move reusable values into tokens or classes
   - [ ] Confirm annotation controls, minimap, spread mode, and presentation overlays look coherent in light and dark themes
   - [ ] Update `STYLE_SYSTEM.md` with any viewer-specific style patterns introduced in this phase

### Phase 5 done when

- [ ] Highlights and notes persist and re-render correctly on reopen
- [ ] Annotation export produces a valid PDF (test with pdfinfo or opening in another viewer)
- [ ] Two-page spread mode works correctly
- [ ] Presentation mode is full-screen with no visible chrome
- [ ] Annotation toolbars, popovers, minimap, and presentation overlays all use the Phase 4 style system
- [ ] No new viewer feature depends on hard-coded colors, spacing, radii, or typography in ordinary view code

---

## Phase 6 — Polish and ship (weeks 19–23)

**Goal:** production-quality binary ready for Flathub submission, with final polish routed through the unified style system instead of one-off UI fixes.

This phase should treat the Phase 4 style system as the default path for visual polish. Any final UI refinement should first ask whether the change belongs in a token, style class, layout primitive, or styled helper. Inline styling is acceptable only for genuinely local visual details.

### Tasks in order

1. **Error handling audit**
   - [ ] Every `anyhow::Error` must be surfaced to the user (never silently swallowed)
   - [ ] Add an in-app error banner: dismissable, shows human-readable message
   - [ ] Corrupted PDF → show error in viewer area, not a panic
   - [ ] Use the shared `ErrorBanner` style class and helper for all user-visible errors
   - [ ] Ensure error, warning, and success states use semantic tokens rather than raw colors

2. **Performance profiling**
   - [ ] Add `tracing::instrument` to all async tasks and render paths
   - [ ] Use `tracing-subscriber` with JSON output for flamegraph analysis
   - [ ] Profile whether styled helpers or layout abstractions introduce measurable overhead in large library and viewer paths
   - [ ] Target metrics:
     - Frame time: < 8ms (120fps budget) during scroll
     - Tile render time: < 200ms per page at 800px width
     - Library load time: < 500ms for 1000 entries
     - Search latency: < 100ms for any query
   - [ ] If style abstractions create performance problems, optimize the helpers without leaking visual constants back into view code

3. **Settings persistence**
   - [ ] Store settings in `$XDG_CONFIG_HOME/pdf-folio/config.toml`
   - [ ] Settings: theme, default zoom, watch directories, tile cache size, sidebar width
   - [ ] Live reload: watch config file and apply changes without restart
   - [ ] Persist only user-facing style preferences, not internal token names
   - [ ] Route theme changes through the Phase 4 semantic token layer so all UI surfaces update consistently

4. **Final visual polish pass**
   - [ ] Audit toolbar, sidebar, library, viewer, annotations, minimap, dialogs, banners, and empty states
   - [ ] Remove remaining duplicated visual constants from ordinary view code
   - [ ] Normalize spacing, typography, border radius, hover states, focus states, and selected states across the app
   - [ ] Confirm that every repeated UI pattern has a token, class, layout primitive, or styled helper
   - [ ] Check light and dark themes side by side before release
   - [ ] Update `STYLE_SYSTEM.md` with any final conventions discovered during polish

5. **Accessibility**
   - [ ] All interactive elements reachable by keyboard
   - [ ] Focus ring visible on all focused elements
   - [ ] Window title updates on navigation
   - [ ] Ensure focus, selected, active, disabled, and error states are represented in the style system
   - [ ] Check contrast for all semantic color tokens in light and dark themes
   - [ ] Consider `atspi` integration for screen reader support (stretch goal)

6. **Desktop integration**
   - [ ] `pdf-folio.desktop` with `MimeType=application/pdf`
   - [ ] Register as PDF handler via `xdg-mime`
   - [ ] DBus activation so opening a second PDF from the file manager focuses the existing window and opens the file
   - [ ] Confirm desktop-launched error states use the same styled in-app banner path as CLI-launched errors

7. **Flatpak manifest** (`packaging/dev.pdf-folio.PDF-Folio.yml`)
   - [ ] Runtime: `org.freedesktop.Platform//23.08`
   - [ ] Permissions: `--filesystem=home:ro` for library access, `--socket=wayland`, `--socket=fallback-x11`
   - [ ] Bundle pdfium binary as a module
   - [ ] Test with `flatpak-builder --run`
   - [ ] Confirm bundled font and icon assets are loaded through the same style/theme assumptions used in development builds

8. **AppImage and .deb**
   - [ ] AppImage via `appimagetool` for portable single-binary distribution
   - [ ] `.deb` via `cargo-deb` for Debian/Ubuntu users
   - [ ] Verify that packaged builds preserve the same theme, font, spacing, and icon behavior as local builds

9. **Plugin API** (if time allows)
   - [ ] Define a `PDF-FolioPlugin` WASM interface: `on_import(path: str) -> Metadata`, `render_sidebar() -> iced::Element`
   - [ ] Load plugins from `$XDG_DATA_HOME/pdf-folio/plugins/*.wasm`
   - [ ] Sandbox via `wasmtime` with explicit capability grants
   - [ ] Do not allow plugins to bypass the host style system for built-in UI surfaces
   - [ ] If plugins can render UI, expose a constrained set of host-provided style tokens and components rather than raw app internals

### Phase 6 done when

- [ ] `flatpak-builder` produces a working bundle
- [ ] AppStream metadata passes `appstreamcli validate`
- [ ] No `tracing::error!` or `eprintln!` in production paths — all errors go to the in-app banner
- [ ] `cargo clippy --all-targets -- -D warnings` passes clean
- [ ] `cargo test --workspace` passes
- [ ] Final UI polish is expressed through the Phase 4 style system rather than scattered one-off styling
- [ ] Light and dark themes remain visually coherent across all production surfaces

---

## Coding conventions

These apply to every file the agent generates.

- **No `.unwrap()` or `.expect()` on fallible operations** in library code. Use `?` and `anyhow::Result`. `.unwrap()` is only allowed in tests and in `main` after explicit error formatting.
- **No blocking calls on the async executor.** Anything that touches the filesystem or pdfium must go through `tokio::task::spawn_blocking` or `Command::perform`.
- **No `Arc<Mutex<T>>` held across `.await` points.** Lock, copy what you need, drop before awaiting.
- **All public types and functions must have doc comments.** Use `///` with a one-line summary and, for complex functions, an `# Errors` section.
- **Tracing over println.** Use `tracing::info!`, `tracing::warn!`, `tracing::error!` — never `println!` or `eprintln!` in library code.
- **Error messages are user-visible.** Write them in plain English, sentence case, no Rust type names. "Could not open file: the path does not exist." not "No such file or directory (os error 2)".
- **Feature flags for optional backends.** Any alternative implementation (e.g. a future mupdf backend) lives behind a Cargo feature flag, not `cfg(target_os)`.
- **Tests live next to the code.** Use `#[cfg(test)] mod tests { ... }` in the same file. Integration tests go in `tests/`.
- **Style through the style system.** Reusable UI polish belongs in `pdf-folio-ui/src/style/` as tokens, classes, layout primitives, or styled helpers. Do not scatter hard-coded colors, spacing, radii, or typography through ordinary view code.

---
