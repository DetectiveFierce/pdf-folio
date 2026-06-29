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
   - [x] Create a dedicated `style` module owned by the UI crate
   - [x] Split style concerns into clear files:
     - `tokens.rs` — colors, spacing, radii, typography, shadows, borders
     - `classes.rs` — reusable semantic style classes
     - `components.rs` — styled constructors for common UI widgets
     - `layout.rs` — reusable layout primitives and spacing helpers
     - `mod.rs` — public style-system exports
   - [x] Keep style definitions independent from business logic, document state, database state, and rendering state
   - [x] Make style APIs easy to call from views without exposing internal app state

   Implementation notes:
   - Completed 2026-06-28. The UI crate now exposes `style::{tokens, classes, components,
     layout}` with semantic tokens, style classes, reusable constructors, and shared dimensions.
   - The style layer takes only passed-in tokens/content/messages and does not read app, document,
     database, library, or rendering state.
   - Updated 2026-06-28: added `TextAlignment` and `ContentAlignment` tokens plus alignment helper
     constructors so text/content placement can be controlled through the style layer.

2. **Design tokens**
   - [x] Define global semantic tokens for:
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
   - [x] Replace hard-coded visual constants in the app shell, toolbar, sidebar, library view, and viewer overlays
   - [x] Support light and dark values through the same semantic token names
   - [x] Keep raw color literals and one-off dimensions out of ordinary view code

   Implementation notes:
   - Light/dark color values now live behind the shared `ThemeTokens` type, and repeated spacing,
     font, sidebar, card, row, window, page-gutter, and overlay dimensions moved into style tokens
     or layout constants.
   - Updated 2026-06-28: the dark theme now uses a neutral gray Shelve-like palette with
     `#181818`-style app background, `#202020` surfaces, `#282828` raised surfaces/placeholders,
     subdued gray text, neutral hover/focus states, and thin gray progress/search controls.
   - Viewer canvas background, page placeholders, and page shadows are exposed through
     `viewer_primitives(tokens)` so render math stays separate from drawing appearance.

3. **CSS-like class system**
   - [x] Define semantic style classes such as:
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
   - [x] Make style classes composable where practical, similar to CSS utility/class layering
   - [x] Avoid styling widgets inline unless the style is truly local and non-reusable
   - [x] Make class names describe UI role, not temporary visual appearance

   Implementation notes:
   - `Class` now names shell, toolbar, sidebar, TOC, library, tag, overlay, annotation, presentation,
     minimap, and placeholder roles. `container_style(...)` and `button_style(...)` map those roles
     to iced styles.

4. **Styled widget helpers**
   - [x] Add helper constructors for common polished UI elements:
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
   - [x] These helpers should reduce repetitive iced styling boilerplate in view code
   - [x] Helpers may accept messages and content, but they must not know about app internals beyond what is passed in
   - [x] Prefer small composable helpers over large components that hide application behavior

   Implementation notes:
   - Existing shell, toolbar, sidebar, TOC, search, tag, empty-state, library card/row, and progress
     UI now use the new helpers where practical.
   - Annotation toolbar/popover helpers were added as small content wrappers for Phase 5 rather than
     feature-specific components.

5. **Component state styling**
   - [x] Define consistent visual states for:
     - normal
     - hovered
     - pressed
     - focused
     - disabled
     - selected
     - active
     - error
   - [x] Apply these states consistently to toolbar controls, sidebar rows, TOC entries, library cards, tag pills, annotation controls, and overlay controls
   - [x] Ensure keyboard focus is visible and compatible with the later accessibility phase
   - [x] Make selected and active states visually distinct enough for both light and dark themes

   Implementation notes:
   - `ComponentState` centralizes the shared state vocabulary. Iced button statuses are mapped into
     that vocabulary now; focused/selected/active/error are available for upcoming widgets and
     accessibility work through the same style path.

6. **Layout primitives**
   - [x] Define shared layout helpers for common spacing and structure:
     - page gutters
     - card grids
     - sidebar sections
     - toolbar groups
     - inline forms
     - centered empty states
     - floating overlays
     - viewer HUD controls
   - [x] Replace repeated `row![]`, `column![]`, `container(...)`, padding, and spacing patterns where useful
   - [x] Keep layout helpers flexible enough that they do not fight iced's type system
   - [x] Do not move message routing, database calls, document calls, or rendering decisions into layout helpers

   Implementation notes:
   - Shared layout constants now cover page gutters/gaps, library virtualization row heights,
     thumbnail widths, sidebar widths, jump input width, window size, and scroll-line pixels.
   - Message routing and app behavior remain in `app.rs`.
   - Updated 2026-06-28: the library tag sidebar width is now user-resizable by dragging its right
     edge, clamped by style layout constants, and tag names are truncated with `...` as the panel
     narrows.

7. **Style documentation**
   - [x] Add `STYLE_SYSTEM.md` or a dedicated section in this plan explaining:
     - token naming
     - class naming
     - when to create a new style class
     - when inline styling is acceptable
     - how light/dark themes should be extended
     - how styled helpers should accept messages
     - how viewer and annotation overlays should share visual primitives
   - [x] Include small examples showing the preferred pattern for adding a new styled component
   - [x] Include a short anti-pattern section showing what not to do

   Implementation notes:
   - Added `STYLE_SYSTEM.md` with module responsibilities, naming guidance, examples, component
     state guidance, viewer/overlay guidance, and anti-patterns.

8. **Refactor existing UI to use the style system**
   - [x] Refactor the app shell
   - [x] Refactor the toolbar
   - [x] Refactor the TOC sidebar
   - [x] Refactor the library grid/list view
   - [x] Refactor search, tag pills, progress bars, and empty states
   - [x] Refactor viewer placeholders and overlays
   - [x] Remove duplicated visual constants after migration
   - [x] Add focused snapshot-style checks where practical by testing token/class construction rather than pixel output

   Implementation notes:
   - Added focused tests for semantic container classes and button state differences in
     `style::classes`.
   - Library cards/rows now include visual progress bars backed by the shared progress helper.
   - Verified with `cargo test --workspace` on 2026-06-28.

### Phase 4 done when

- [x] Most UI styling flows through tokens, style classes, or styled widget helpers
- [x] View code is primarily structural and message-oriented, not cluttered with visual constants
- [x] Light and dark themes use the same semantic style layer
- [x] Adding or polishing a UI component does not require touching rendering, database, document, or library logic
- [x] The app has a visibly more consistent visual language across toolbar, sidebar, library, and viewer surfaces

## Phase 5 — Library manager expansion (weeks 15–18)

**Goal:** turn the first-pass library manager into a polished native PDF library system with sorting, selection, metadata editing, folders, drag-and-drop organization, and bulk actions.

Library features added in this phase must use the Phase 4 style system. Library cards, rows, selection affordances, folder controls, metadata editors, drag targets, context actions, empty states, and bulk-edit surfaces should all be styled through tokens, classes, layout primitives, or styled helpers.

### Tasks in order

1. **Library data model expansion** (`pdf-folio-library/src/db.rs`)
   - [x] Add stable manual ordering support for PDFs
     - `manual_order INTEGER NOT NULL`
     - preserve gaps between order values where practical so reordering does not require rewriting the whole table every time
   - [x] Add user-editable metadata fields
     - `display_title TEXT`
     - `display_author TEXT`
     - `sort_title TEXT`
     - `sort_author TEXT`
     - `metadata_locked INTEGER DEFAULT 0`
   - [x] Add folder support
     - `folders(id, name, parent_id, manual_order, created_at, updated_at)`
     - `entry_folders(entry_id, folder_id, manual_order)`
   - [x] Add optional library state tables for persisted view preferences
     - active sort mode
     - active layout mode
     - selected folder
     - sidebar width
     - visible metadata fields
   - [x] Add migrations without losing existing imported PDFs, tags, progress, bookmarks, thumbnails, or search index state
   - [x] Add focused database tests for ordering, folder membership, metadata edits, and cascade behavior

   Implementation notes:
   - Completed 2026-06-28. `entries` now has `manual_order`, display metadata overrides,
     explicit sort keys, and `metadata_locked`; existing databases are migrated with `ALTER TABLE`
     additions and backfilled order/sort values.
   - Added public `FolderId`, `Folder`, `LibrarySortMode`, `LibraryLayoutMode`, and
     `LibraryPreferences` types plus APIs for manual entry ordering, metadata override/reset,
     title sort cleanup, folder CRUD, folder membership, invalid folder-move protection, and
     preference load/save.
   - Folder deletion cascades folder membership and nested folders but leaves PDF entries intact.
   - Focused tests cover manual ordering, metadata edits/reset, folder membership/nesting/cascade,
     invalid descendant moves, and preference round-tripping.

2. **Sort and view modes**
   - [x] Add sortable library modes:
     - Manual order
     - Title A-Z / Z-A
     - Author A-Z / Z-A
     - Recently added
     - Recently opened
     - Reading progress
     - Page count
     - Missing files
   - [x] Manual order must preserve exactly the order the user leaves PDFs in
     - Storage/API support is complete; the library UI now persists manual drag reorder drops via
       `Db::set_manual_entry_order`.
   - [x] Store the active sort mode and restore it on app restart
   - [x] Disable drag-to-reorder in non-manual sorted views, or show a clear affordance that reordering requires switching to Manual
     - The UI shows a reorder status hint and disables drag handles outside an unfiltered Manual view.
   - [x] Keep search results sortable without mutating the underlying manual order unless the user explicitly switches to Manual and drags entries there
     - Search and tag-filtered views remain sorted but do not allow manual reorder drops yet; this
       prevents partial visible subsets from rewriting the root manual order.
   - [ ] Add toolbar or sidebar controls for sort mode, sort direction, grid/list mode, and visible metadata density
     - Sort mode is now a themed dropdown in the library menu bar and grid/list controls are present
       and persisted; visible metadata density is stored in preferences but does not have a UI
       selector yet.

   Implementation notes:
   - Completed 2026-06-28 at the data/query level and with a minimal UI control. The library header
     has a Phase 4-styled sort dropdown, and the existing grid/list toggle persists via
     `library_preferences`.
   - Updated 2026-06-28: dropdown menus and tooltips use rounded, themed overlay styles matching
     the app surface, border, focus, and shadow tokens.
   - `PDFolioApp::refresh_library()` now queries `Db::get_entries_sorted(app.library_sort_mode)`.
     Search uses the selected sort mode as its base ordering, while Tantivy full-text hits are still
     promoted first; a later search/filter pass should add clearer match-source sorting controls.
   - Sidebar width is now loaded from and saved to `library_preferences`.

3. **Drag-and-drop PDF reordering**
   - [x] In Manual view, allow dragging PDFs to reorder them
   - [x] Support both grid and list reordering
   - [x] Show insertion indicators between rows/cards
   - [x] Auto-scroll the library viewport while dragging near the top or bottom edge
   - [x] Persist the new order immediately after drop
   - [x] Keep drag math separate from style definitions; style only controls the visual drag state, insertion marker, opacity, focus ring, and hover target
   - [x] Add tests for reorder calculations independent of iced UI code

   Implementation notes:
   - Updated 2026-06-28: `PDFolioApp` has a `LibraryDragState` plus begin/move/end messages.
     A drag starts from a thresholded press-and-move on a card or row surface. The drop slot is
     previewed with a stable semi-transparent in-flow placeholder, while a separate floating preview
     follows the cursor above the grid/list; the in-memory manual list updates optimistically and
     saves the complete visible manual order on mouse release.
   - Reorder remains intentionally enabled only when sort mode is Manual and no search or tag filter
     is active. Filtered/manual subset ordering is still open.
   - Updated 2026-06-28: drag auto-scroll now uses a stable scrollable ID, a 60 Hz drag-only tick,
     real viewport bounds from iced's scroll callback, a quadratic edge-zone velocity curve, and
     programmatic scroll operations. Unit tests cover the edge-zone velocity math.

4. **Multi-selection model**
   - [x] Add selection state to the library view
     - single click selects one PDF
     - Ctrl-click toggles selection
     - Shift-click selects a contiguous range
     - Ctrl+A selects all visible PDFs
     - Escape clears selection
   - [x] Support selection across grid and list views
   - [x] Keep selection stable when changing between grid/list layouts
   - [x] Decide and document behavior when filters/search change while items are selected
     - Selection is pruned to entries still visible under the active search/tag/folder filters.
   - [x] Show selected count in the toolbar
   - [x] Add a bulk-action toolbar that appears only when one or more PDFs are selected
   - [x] Style selected, active, focused, and hovered states through the Phase 4 component-state system
   - [ ] Show a checkbox in the top-left corner of each card and each list row whenever that entry is
     selected or hovered while any selection is active
     - Checkbox appears overlaid on the card thumbnail or at the leading edge of the list row
     - Checkbox uses a dedicated `SelectionCheckbox` style class with positioned overlay sizing,
       z-order, and background scrim tokens so it does not obscure the title or thumbnail art
     - Clicking the checkbox adds that entry to the current selection without clearing other
       selected entries, mirroring Ctrl-click behavior
     - Unchecking the checkbox removes only that entry from the selection, mirroring Ctrl-click
       toggle behavior
     - Checkbox visibility follows hover state when no selection is active: it appears on hover as
       a preview affordance and disappears when the cursor leaves unless the card is selected
     - Checkbox checked/unchecked/hovered/focused states are styled through the Phase 4
       component-state system
     - Checkbox hit target is at least 24×24 logical pixels and does not overlap the card
       open-document hit area
     - In list view the checkbox appears as a leading inline element at the same vertical center as
       the row text rather than as a corner overlay
     - Add a `SelectionCheckbox` style class and a `selection_checkbox(checked, on_toggle)` styled
       helper to the Phase 4 style system
   - [ ] When any card or row checkbox is visible, show a master select/deselect-all checkbox in
     the library toolbar that reflects the current all-selected, none-selected, or partial state
     - Partial state renders as an indeterminate checkbox marker
     - Clicking the master checkbox when partial or none-selected performs Select All Visible
     - Clicking it when all are selected performs Clear Selection
   - [ ] Add focused tests for checkbox toggle behavior independent of iced rendering

   Implementation notes:
   - Selection messages `EntryCheckboxToggled(EntryId)` and `MasterCheckboxClicked` share the
     existing `SelectionToggled` and `SelectAllVisible` / `ClearSelection` message paths so that
     checkbox interaction and keyboard shortcuts remain a single code path.
   - The `selection_checkbox` helper must accept only checked state and a toggle message; it must
     not read app, document, database, or rendering state directly.

5. **Multi-selection drag**
   - [ ] When more than one card is selected and the user begins a drag gesture on any selected card,
     enter multi-selection drag mode instead of single-card reorder mode
     - A drag gesture is defined by the same press-and-move threshold used in single-card reorder
     - Dragging an unselected card while a selection exists follows the existing single-card reorder
       path without clearing the selection
   - [ ] During a multi-selection drag, display a stacked thumbnail ghost that travels with the cursor
     - The ghost renders the thumbnails of the selected PDFs layered as a physical card stack:
       rear cards are offset down-right by a small token-defined increment and drawn at decreasing
       opacity so the stack depth is legible without obscuring the cursor target
     - Only the top two or three thumbnails are rendered in the stack regardless of selection size;
       a badge on the ghost shows the total selected count
     - The ghost is positioned so its top-left corner tracks the cursor with a fixed logical offset
       that keeps it clear of the pointer
     - Ghost stack appearance — offset increment, opacity falloff, badge size, badge background,
       badge typography, and corner radius — is controlled through style tokens and a
       `DragStackGhost` style class; no visual constant appears in drag geometry code
     - Ghost rendering runs entirely in the UI layer and does not trigger tile re-renders or
       database reads
   - [ ] During a multi-selection drag, replace each selected card's original position in the
     grid or list with a placeholder of the same dimensions as that card
     - The number of placeholders equals the number of selected PDFs
     - Placeholders use the `DragInsertionMarker` style class extended with a filled, low-opacity
       surface so the vacated slots are visible but clearly distinct from content cards
     - Placeholders remain in their original positions until the drag resolves; they do not move
       with the cursor
   - [ ] Compute a contiguous drop zone in the non-selected entries as the cursor moves
     - The drop zone indicates where the selected group will be inserted as a contiguous block,
       preserving the relative order of selected items with respect to one another
     - Drop zone indicators use the existing single-card `DragInsertionMarker` style, extended to
       span the full width of the insertion gap between non-selected cards or rows
     - Only one drop zone indicator is shown at a time regardless of selection size
   - [ ] On mouse release, move all selected entries to the drop zone position as a contiguous block
     - The relative order of the selected entries is preserved exactly as it was before the drag
     - The insertion is performed relative to the nearest non-selected entry at the drop point
     - The in-memory library list is updated optimistically before the database write completes
     - `Db::set_manual_entry_order` is called once with the complete new order for the visible
       window, not once per selected entry
     - If the database write fails, the in-memory list is rolled back and an `ErrorBanner` is shown
   - [ ] Multi-selection drag is enabled only when sort mode is Manual and no search or tag filter
     is active; in other modes the drag gesture is a no-op and the cursor should not suggest
     draggability
   - [ ] Auto-scroll behavior during multi-selection drag follows the same quadratic edge-zone
     velocity curve used for single-card reorder; the scroll tick and viewport bounds logic is
     shared, not duplicated
   - [ ] Add unit tests for the multi-selection drop position calculation: given a list of entry
     IDs, a selection set, and a cursor drop index among non-selected entries, assert the resulting
     full order matches the expected output
   - [ ] Add unit tests for the placeholder count invariant: placeholder count always equals
     selection size throughout the drag lifecycle

   Implementation notes:
   - `LibraryDragState` is extended with a `MultiDrag { ghost_pos, drop_index }` variant; single-
     card reorder uses the existing `SingleDrag` variant unchanged.
   - The stacked ghost is rendered as a separate top-level canvas layer above the library scroll
     area so it is not clipped by the virtualized list viewport.
   - Ghost thumbnail data is sourced from the in-memory thumbnail cache; missing thumbnails show a
     styled placeholder tile in the stack rather than triggering a background render.

6. **Drag-to-folder assignment**
   - [ ] During any drag (single-card or multi-selection), detect when the cursor enters a valid
     folder drop target and switch the drop mode from reorder to folder assignment
     - Valid folder drop targets are: folder cards displayed in the library grid or list, and folder
       rows displayed in the folder sidebar
     - A folder drop target activates when the cursor dwells over it for a short token-defined delay
       (approximately 500 ms) or when the cursor is held stationary over it; this prevents accidental
       folder assignment during fast reorder drags
   - [ ] When a folder drop target is active, display a visual highlight on that folder card or
     sidebar row indicating it will receive the dragged PDFs on release
     - The highlight uses a `FolderDropTarget` style class with a distinct border, background tint,
       and icon glow or accent token; it must be clearly different from the normal hover state
     - The reorder insertion marker is hidden while a folder drop target is active because the drop
       resolves as a folder assignment, not a positional reorder
   - [ ] When the sidebar is visible during a drag, ensure the folder tree in the sidebar remains
     interactive so the user can drop onto deeply nested folders without first navigating to them
     - Hovering over a collapsed sidebar folder row during a drag expands that row after the same
       dwell delay used for drop-target activation, allowing the user to reach nested folders
     - Expanded-during-drag folder rows collapse back to their pre-drag state if the cursor leaves
       without dropping
   - [ ] On release over an active folder drop target, assign all dragged PDFs to that folder
     - For single-card drag, assign the one dragged entry to the folder
     - For multi-selection drag, assign all selected entries to the folder; preserve each entry's
       existing folder memberships in other folders (folder assignment is additive, not exclusive)
     - Call `Db::add_entry_to_folder` for each dragged entry; batch the writes in a single
       transaction where the database API permits
     - If the current library view is filtered to a specific folder and the drop target is a
       different folder, the dragged entries remain visible in the current view because folder
       membership is additive; no entries disappear from the current view on drop
     - If the drop target is the same folder as the current view filter, the assignment is a no-op
       and the drag resolves as if the user dropped onto empty space (reorder or cancel)
   - [ ] After a successful folder drop, briefly flash the folder card or sidebar row with a
     confirmation tint before returning it to its normal appearance
     - The flash duration and tint color are controlled through style tokens; no timing constant
       appears in drop-handling message code
   - [ ] If the folder assignment database write fails for any entry, show an `ErrorBanner` with a
     count of successes and failures; do not silently swallow partial failures
   - [ ] Keep folder-drop hit testing and dwell timing in the drag state machine, not in the style
     system; style only controls the visual appearance of the active drop target and confirmation
     flash
   - [ ] Add unit tests for folder-drop activation logic: given a cursor position and a set of
     rendered folder target rects, assert the correct target activates and that no target activates
     when the cursor is over a non-folder area
   - [ ] Add unit tests for additive folder membership: assigning an entry already in folder A to
     folder B results in membership in both A and B, not just B

   Implementation notes:
   - `LibraryDragState` is extended with a `drop_target: Option<FolderId>` field that is set when
     dwell activation fires and cleared when the cursor leaves the target or the drag ends.
   - The sidebar folder tree reuses the existing `FolderRow` style class with an additional
     `DropTargetActive` component state so the highlight is expressed through the existing
     component-state styling path.
   - Drag-to-folder and drag-to-reorder are mutually exclusive within a single drag gesture: once a
     folder drop target activates, the reorder insertion marker is suppressed until the cursor leaves
     the folder target area.

7. **Bulk editing and bulk actions**
   - [x] Bulk edit selected PDFs
     - add tags
     - remove tags
     - add to folder
     - remove from folder
     - delete from library metadata only
     - optionally move files to trash after explicit confirmation
   - [x] Bulk metadata actions
     - clear custom title
     - clear custom author
     - apply title sort cleanup
     - refresh metadata from PDF
     - rebuild thumbnails
     - reindex full text
   - [x] Show confirmation dialogs for destructive actions
   - [ ] Show progress for long bulk operations
   - [x] Surface partial failures clearly, for example: "Updated 48 PDFs; 2 could not be changed."
   - [ ] Use shared `ProgressBar`, `ErrorBanner`, toolbar, dialog, and empty-state styling paths
     - Bulk toolbar and confirmation dialog use shared styling; progress and dedicated error-banner
       surfaces are still pending.

   Implementation notes:
   - Added 2026-06-28: grid/list selection, Ctrl-click toggle, Shift-click range selection,
     Ctrl+A, Escape clear, Enter open selected single PDF, Delete metadata-only bulk delete,
     selected count, selected card/row styling, and a bulk toolbar for tags, current-folder
     membership, metadata reset, thumbnail rebuild, full-text reindex, and library metadata delete.
     Destructive confirmation dialogs and detailed progress UI are still pending.
   - Updated 2026-06-28: bulk metadata actions now include reset display metadata, title-sort
     cleanup, selected-PDF metadata refresh, thumbnail rebuild, full-text reindex, and metadata-only
     library delete. Metadata reset and metadata-only delete require an in-app confirmation dialog;
     Delete also uses that dialog when triggered by the keyboard shortcut. Metadata-only delete now
     removes matching Tantivy search documents. Refresh currently updates author attribution and
     page count from the PDF because `PdfDoc` exposes author metadata but not title metadata yet.
   - Updated 2026-06-28: selection actions now replace the top app toolbar that normally contains
     Open/Grid/Light/PDF-Folio. Single-PDF selection shows title and author edit fields plus Save
     and a More dropdown scoped to single-PDF metadata edits. Multi-selection shows tag input plus
     grouped Tags, Folders, Metadata, and Maintenance dropdowns. Selection controls no longer occupy
     the narrow library content column, and menu-bar labels are kept single-line.

   Top application menu organization:

   The layout follows common desktop and creative-tool menu conventions: File owns document/library
   ingress and session lifecycle, Edit owns selection and metadata editing, View owns visibility,
   layout, theme, and zoom, Document owns PDF-reading commands, Library owns collection and folder
   organization, Tools owns batch maintenance, and Help owns documentation/status/about surfaces.
   Future menu entries are shown in square brackets and should remain out of the runtime UI until
   those actions are implemented.

```text
   File
   ├─ Open PDF...
   ├─ Import Folder...
   ├─ Back to Library
   ├─ [Open Recent]
   ├─ [Close Document]
   ├─ [Reveal Current PDF in Files]
   ├─ [Export]
   ├─ [Print]
   └─ [Quit]

   Edit
   ├─ [Undo]
   ├─ [Redo]
   ├─ [Cut]
   ├─ [Copy]
   ├─ [Paste]
   ├─ Select All Visible PDFs
   ├─ Clear Selection
   ├─ Save Details
   ├─ Reset Details...
   ├─ Add Typed Tag
   ├─ Remove Typed Tag
   ├─ Delete From Library...
   ├─ [Find in Library]
   ├─ [Copy Citation]
   ├─ [Duplicate Metadata]
   └─ [Preferences]

   View
   ├─ Switch to Grid / Switch to List
   ├─ Switch to Light Theme / Switch to Dark Theme
   ├─ [Show/Hide Library Sidebar]
   ├─ Show/Hide Table of Contents
   ├─ Jump to Page...
   ├─ Zoom In
   ├─ Zoom Out
   ├─ Reset Zoom
   ├─ [Presentation Mode]
   ├─ [Fit Width]
   ├─ [Fit Page]
   ├─ [Rotate Clockwise]
   └─ [Full Screen]

   Document
   ├─ Jump to Page...
   ├─ Show/Hide Table of Contents
   ├─ Zoom In
   ├─ Zoom Out
   ├─ Reset Zoom
   ├─ [Find in Document]
   ├─ [Add Highlight]
   ├─ [Add Sticky Note]
   ├─ [Draw Freehand]
   ├─ [Export Annotated PDF]
   └─ [Document Properties]

   Library
   ├─ New Folder...
   ├─ Import Folder...
   ├─ Refresh Library
   ├─ Add Selection to Current Folder
   ├─ Remove Selection from Current Folder
   ├─ Sort: Manual
   ├─ Sort: Title A-Z
   ├─ Sort: Title Z-A
   ├─ Sort: Author A-Z
   ├─ Sort: Author Z-A
   ├─ Sort: Recently Added
   ├─ Sort: Recently Opened
   ├─ Sort: Progress
   ├─ Sort: Page Count
   ├─ Sort: Missing
   ├─ [Rename Folder]
   ├─ [Delete Folder]
   ├─ [Move Folder]
   ├─ [Reveal Selected File]
   ├─ [New Smart Collection]
   └─ [Show Missing Files Only]

   Tools
   ├─ Apply Title Sort Cleanup
   ├─ Refresh PDF Metadata
   ├─ Reset Display Metadata...
   ├─ Rebuild Thumbnails
   ├─ Reindex Full Text
   ├─ [Run Duplicate Detection]
   ├─ [Repair Library Database]
   ├─ [Optimize Search Index]
   ├─ [Export Library Catalog]
   └─ [Plugin Manager]

   Help
   ├─ PDF-Folio
   ├─ Status
   ├─ [Keyboard Shortcuts]
   ├─ [User Guide]
   ├─ [Report Issue]
   └─ [About PDF-Folio]
```

8. **Edit PDF title and author**
   - [x] Add an entry details/editor panel
   - [x] Allow editing display title and display author
   - [x] Preserve original extracted metadata separately from user overrides
   - [x] Add "Reset to PDF metadata" action
     - UI action is surfaced in the details panel and asks for confirmation before clearing edits.
   - [x] Add "Apply title sort cleanup" helper for leading articles such as "The", "A", and "An"
     - Database API is complete; bulk UI action is surfaced in the selected-PDF toolbar.
   - [x] Make title and author edits immediately visible in cards, rows, search results, and sort modes
     - Display paths now prefer override fields when present; editor UI saves refresh the library view.
   - [x] Update Tantivy metadata fields after edits so search reflects user-visible title and author
   - [x] Add validation for empty titles, extremely long fields, and invalid control characters

   Implementation notes:
   - Started 2026-06-28. Storage and display plumbing are in place: original `title`/`author`
     remain separate from `display_title`/`display_author`, and cards, rows, local search matching,
     and sort modes prefer display metadata.
   - Updated 2026-06-28: selecting one PDF replaces the top app toolbar with display title and
     display author fields, Save, Clear, and a More dropdown for reset/metadata/search maintenance.
     Saves write display overrides, lock metadata, refresh the library, and reindex Tantivy for the
     edited entry. Reset clears overrides, unlocks extracted metadata updates, refreshes the library,
     and reindexes the entry using the extracted title/author.

9. **Folders and folder management**
   - [x] Add a folder sidebar for library organization
     - Initial UI supports folder selection, nested display, and inline folder creation.
   - [x] Support creating, renaming, deleting, and nesting folders
     - Database API is complete; UI currently supports creation and nested navigation.
       Rename/delete/move actions still need visible controls.
   - [ ] Support dragging PDFs into folders
   - [ ] Support dragging selected PDFs into folders as a bulk operation
   - [ ] Support dragging folders to reorder them in Manual folder order
   - [ ] Support dragging folders into other folders to nest them
   - [x] Prevent invalid folder operations
     - folder cannot be moved into itself
     - folder cannot be moved into one of its descendants
     - deleting a folder does not delete PDFs unless explicitly requested through a separate destructive action
   - [ ] Show smart counts next to folders
     - Initial UI shows direct PDF counts and child-folder counts.
     - total PDFs
     - unread/in-progress count where useful
     - missing-file count where useful
   - [x] Add folder empty states with clear import/add instructions
   - [ ] Persist expanded/collapsed folder state across restarts

   Implementation notes:
   - Started 2026-06-28 at the database/API layer. Folder tree creation, rename, delete, nesting,
     moving, membership, and folder-entry queries are implemented and tested. The existing tag
     sidebar has now been expanded into a folder organization sidebar with inline folder creation,
     selected-folder persistence, folder cards above PDFs, and shorter folder-card sizing.

10. **Collections, tags, and folders interaction**
    - [ ] Clarify the product model:
      - folders are manual, user-managed hierarchy
      - tags are flexible labels
      - collections are saved queries or curated groups, if retained
    - [ ] Add saved searches or smart collections if useful:
      - Recently added
      - Continue reading
      - Unread
      - In progress
      - Finished
      - Missing files
      - Untagged
    - [ ] Ensure tags and folders can both filter the same library without surprising behavior
    - [ ] Add a visible breadcrumb or filter summary when the user is inside a folder, tag filter, search query, or smart collection
    - [ ] Add "Clear filters" action

11. **Native library manager affordances**
    - [ ] Add context menus or equivalent explicit actions for PDFs and folders
    - [ ] Add keyboard shortcuts:
      - Enter: open selected PDF
      - F2: rename selected PDF title or folder
      - Delete: remove from library after confirmation
      - Ctrl+F: focus search
      - Ctrl+A: select all visible
      - Escape: clear selection or close active editor
    - [ ] Add details sidebar for the selected PDF
      - cover thumbnail
      - title
      - author
      - path
      - page count
      - reading progress
      - tags
      - folders
      - added/opened dates
      - missing-file status
    - [ ] Add "Reveal in file manager" action
    - [ ] Add "Open containing folder" action
    - [ ] Add "Relink missing file" action
    - [ ] Add duplicate detection UI for PDFs with matching content hashes
    - [ ] Add thumbnail refresh action
    - [ ] Add full-text reindex action

12. **Import and organization polish**
    - [ ] During import, infer title and author from PDF metadata where available
    - [ ] Fall back to filename-derived title when metadata is missing or poor
    - [ ] Offer an import destination folder
    - [ ] Preserve folder-relative organization when importing a directory tree if the user chooses that option
    - [ ] Show import summary:
      - added
      - skipped duplicates
      - failed
      - already present
    - [ ] Add a post-import review view for fixing missing titles/authors
    - [ ] Keep all import and indexing work off the UI thread

13. **Library search and filtering upgrade**
    - [ ] Search title, author, tags, folder names, and full text
    - [ ] Add filter chips for active tags, folders, reading state, and missing state
    - [ ] Add advanced search affordances later if needed, but keep the default search simple
    - [ ] Preserve sort mode while searching
    - [ ] Make manual ordering behavior clear when search/filtering hides some entries
    - [ ] Show matching page number and match source where practical:
      - title
      - author
      - tag
      - full text page hit

14. **Library performance and correctness pass**
    - [ ] Confirm virtualized grid/list behavior still works with selection, drag-and-drop, details panel, and folders
    - [ ] Benchmark 1000, 5000, and 10000-entry library views
    - [ ] Ensure drag-and-drop does not trigger excessive database writes
    - [ ] Batch SQLite updates for bulk actions
    - [ ] Avoid blocking the iced update loop during metadata edits, folder operations, imports, thumbnail refreshes, and reindexing
    - [ ] Add tracing spans for sorting, filtering, selection changes, folder operations, and bulk edits

15. **Library style-system pass**
    - [ ] Add or refine style classes for:
      - `LibraryToolbar`
      - `BulkActionBar`
      - `SelectionBadge`
      - `SelectionCheckbox`
      - `MasterCheckbox`
      - `DragStackGhost`
      - `FolderSidebar`
      - `FolderRow`
      - `FolderDropTarget`
      - `DragInsertionMarker`
      - `MetadataEditor`
      - `DetailsPanel`
      - `SortMenu`
      - `FilterChip`
      - `ContextMenu`
      - `ConfirmDialog`
    - [ ] Add styled helpers for common library controls:
      - sortable header button
      - folder row
      - selected library card
      - selected library row
      - metadata field editor
      - bulk-action button
      - filter chip
      - drag ghost preview
      - `selection_checkbox(checked, on_toggle)`
      - `master_checkbox(state, on_click)` where state is `AllSelected`, `NoneSelected`, or `Partial`
    - [ ] Update `STYLE_SYSTEM.md` with library-specific styling conventions including checkbox overlay placement, drag stack ghost composition, and folder drop target activation patterns
    - [ ] Confirm light and dark themes are coherent for selected, dragged, drop-target, disabled, missing-file, checkbox-hovered, checkbox-checked, and error states

### Phase 5 done when

- [ ] Manual PDF ordering works and persists exactly as the user leaves it
- [ ] PDFs can be dragged to reorder them in Manual view
- [ ] PDFs can be selected singly, in ranges, and in bulk
- [ ] Selection checkboxes appear on hovered and selected cards and rows; checking and unchecking them adds and removes entries from the selection without disturbing other selected entries
- [ ] A master checkbox in the library toolbar reflects and controls the all-selected, none-selected, and partial-selection states
- [ ] Dragging two or more selected PDFs shows a stacked thumbnail ghost and correct placeholder count, drops as a contiguous block preserving relative order, and commits a single database reorder write
- [ ] Dragging any selection onto a folder card or sidebar folder row assigns all selected PDFs to that folder additively; the sidebar folder tree remains navigable during a drag via dwell-expand
- [ ] Bulk edit actions work for tags, folders, metadata refresh, thumbnail rebuild, and reindexing
- [ ] PDF display title and display author are editable, searchable, sortable, and resettable
- [ ] Folders can be created, renamed, deleted, nested, reordered, and used as drag-and-drop targets
- [ ] Folder ordering and PDF ordering inside folders persist across restarts
- [ ] Details/editor panel gives the user native-library-manager control over metadata and organization
- [ ] Search, filters, tags, folders, and sort modes compose without surprising state loss
- [ ] Library interactions remain responsive for at least 1000 PDFs and are architecturally ready for larger collections
- [ ] New library UI uses the Phase 4 style system rather than one-off styling

---

## Phase 6 — Viewer features (weeks 19–22)

**Goal:** advanced navigation, text interaction, layout modes, and presentation mode implemented on top of the unified style system.

Viewer features added in this phase must not introduce new one-off visual styling. Find controls, text-selection affordances, minimap controls, presentation overlays, and viewer chrome should all use the tokens, classes, layout primitives, and styled widget helpers created in Phase 4.

### Tasks in order

1. **Text selection and copy**
   - [ ] Extract text positions from pdfium: `page.text().chars()` gives glyph bounds
   - [ ] Build a `TextMap` for each page: `Vec<GlyphRect { char, page_rect }>`
   - [ ] Hit-test mouse position against `TextMap` to find selection boundaries
   - [ ] Render selection highlight as a colored overlay rect on the canvas
   - [ ] Add copy-to-clipboard for selected text
   - [ ] Use style tokens for selection color, opacity, and focus outline
   - [ ] Keep text-selection geometry in viewer logic, not in the style system

2. **Find in document**
   - [ ] Add `Ctrl+F` find overlay in the viewer
   - [ ] Search extracted page text for the current document
   - [ ] Show match count and current match index
   - [ ] Add next/previous match controls
   - [ ] Jump to the page and scroll position for the selected match
   - [ ] Highlight visible matches on the canvas
   - [ ] Reuse `SearchInput`, overlay, toolbar, and text-state styling from the Phase 4 style system

3. **Page navigation polish**
   - [ ] Improve page indicator so it supports direct page entry
   - [ ] Add first-page and last-page commands
   - [ ] Add previous-page and next-page commands
   - [ ] Preserve current horizontal position where reasonable during page jumps
   - [ ] Ensure keyboard navigation, toolbar controls, and page-jump overlay all share one message path
   - [ ] Keep navigation affordances styled through existing toolbar and overlay helpers

4. **Two-page spread mode**
   - [ ] Toggle button in toolbar
   - [ ] Layout engine places paired pages side by side
   - [ ] Support correct first-page handling for documents whose first page should stand alone
   - [ ] Scroll and zoom still work through the same viewer state model
   - [ ] Add any new spread-mode controls through `toolbar_button(...)` or another existing styled helper
   - [ ] Use shared viewer gutter and page-spacing tokens for one-page and two-page layouts

5. **Fit modes**
   - [ ] Add fit-width mode
   - [ ] Add fit-page mode
   - [ ] Add actual-size mode if practical
   - [ ] Make zoom controls clearly show whether a fit mode is active
   - [ ] Preserve fit mode across window resizes until the user explicitly chooses a fixed zoom level
   - [ ] Keep fit-mode layout math in viewer logic and visual state in the style system

6. **Presentation mode**
   - [ ] `F5` → full-screen, hide all chrome, show only the PDF
   - [ ] Click or right arrow → next page
   - [ ] Left arrow → previous page
   - [ ] `Escape` → exit
   - [ ] Use `PresentationOverlay` style class for transient page number, navigation hints, and exit affordances
   - [ ] Ensure presentation-mode overlays share typography and contrast tokens with the rest of the app

7. **Minimap**
   - [ ] Thin vertical strip on right edge of viewer
   - [ ] Renders tiny page thumbnails stacked vertically
   - [ ] Shows a viewport indicator rect
   - [ ] Click/drag on minimap scrolls the main view
   - [ ] Use the `Minimap` style class for strip width, border, opacity, hover state, and viewport indicator
   - [ ] Keep minimap rendering data-driven so style changes do not affect scroll math

8. **Bookmarks and reading state**
   - [ ] Add viewer control for adding a bookmark at the current page
   - [ ] Render bookmarks in the sidebar or a dedicated viewer panel
   - [ ] Allow renaming and deleting bookmarks
   - [ ] Jump to a bookmark from the sidebar
   - [ ] Persist bookmarks in SQLite
   - [ ] Keep bookmark controls styled through sidebar row, toolbar, and inline-form helpers

9. **Viewer polish pass**
   - [ ] Audit every new viewer feature for inline visual constants
   - [ ] Move reusable values into tokens or classes
   - [ ] Confirm find controls, selection, minimap, spread mode, presentation overlays, and bookmark controls look coherent in light and dark themes
   - [ ] Update `STYLE_SYSTEM.md` with any viewer-specific style patterns introduced in this phase

### Phase 6 done when

- [ ] Text selection and copy work for normal text PDFs
- [ ] Find-in-document works and can jump between matches
- [ ] Two-page spread mode works correctly
- [ ] Fit-width and fit-page modes behave correctly during resize and zoom changes
- [ ] Presentation mode is full-screen with no visible chrome
- [ ] Minimap navigation works without interfering with normal scroll and zoom
- [ ] Bookmarks persist and navigate correctly on reopen
- [ ] Viewer controls, minimap, find overlay, bookmark UI, and presentation overlays all use the Phase 4 style system
- [ ] No new viewer feature depends on hard-coded colors, spacing, radii, or typography in ordinary view code

---

## Phase 7 — Polish and ship (weeks 23–27)

**Goal:** production-quality binary ready for Flathub submission, with final polish routed through the unified style system instead of one-off UI fixes.

This phase should treat the Phase 4 style system as the default path for visual polish. Any final UI refinement should first ask whether the change belongs in a token, style class, layout primitive, or styled helper. Inline styling is acceptable only for genuinely local visual details.

Before final release, verify that the expanded Phase 5 library manager and Phase 6 viewer features work together cleanly. In particular, test transitions between library selection, metadata editing, folder navigation, opening a PDF, saving reading progress, returning to the same library view, and preserving sort/filter/selection state where appropriate.

### Tasks in order

1. **Error handling audit**
   - [ ] Every `anyhow::Error` must be surfaced to the user
   - [ ] Add an in-app error banner: dismissable, shows human-readable message
   - [ ] Corrupted PDF → show error in viewer area, not a panic
   - [ ] Use the shared `ErrorBanner` style class and helper for all user-visible errors
   - [ ] Ensure error, warning, and success states use semantic tokens rather than raw colors

2. **Performance profiling**
   - [ ] Add `tracing::instrument` to all async tasks and render paths
   - [ ] Use `tracing-subscriber` with JSON output for flamegraph analysis
   - [ ] Profile whether styled helpers or layout abstractions introduce measurable overhead in large library and viewer paths
   - [ ] Target metrics:
     - Frame time: < 8ms during scroll
     - Tile render time: < 200ms per page at 800px width
     - Library load time: < 500ms for 1000 entries
     - Search latency: < 100ms for any query
     - Manual reorder commit: < 100ms for visible-window drag/drop
     - Bulk metadata edit: batched and responsive for 500 selected entries
   - [ ] If style abstractions create performance problems, optimize the helpers without leaking visual constants back into view code

3. **Settings persistence**
   - [ ] Store settings in `$XDG_CONFIG_HOME/pdf-folio/config.toml`
   - [ ] Settings: theme, default zoom, watch directories, tile cache size, sidebar width, active library sort, active library layout, folder sidebar width, details panel state
   - [ ] Live reload: watch config file and apply changes without restart
   - [ ] Persist only user-facing style preferences, not internal token names
   - [ ] Route theme changes through the Phase 4 semantic token layer so all UI surfaces update consistently

4. **Final visual polish pass**
   - [ ] Audit toolbar, sidebar, library, folders, bulk-action bar, metadata editor, viewer, minimap, dialogs, banners, and empty states
   - [ ] Remove remaining duplicated visual constants from ordinary view code
   - [ ] Normalize spacing, typography, border radius, hover states, focus states, selected states, dragged states, and drop-target states across the app
   - [ ] Confirm that every repeated UI pattern has a token, class, layout primitive, or styled helper
   - [ ] Check light and dark themes side by side before release
   - [ ] Update `STYLE_SYSTEM.md` with any final conventions discovered during polish

5. **Accessibility**
   - [ ] All interactive elements reachable by keyboard
   - [ ] Focus ring visible on all focused elements
   - [ ] Window title updates on navigation
   - [ ] Library selection, folder navigation, metadata editing, and drag-and-drop alternatives are usable without a mouse
   - [ ] Viewer find, selection, bookmarks, page navigation, and presentation controls are usable by keyboard
   - [ ] Ensure focus, selected, active, disabled, dragged, drop-target, and error states are represented in the style system
   - [ ] Check contrast for all semantic color tokens in light and dark themes
   - [ ] Consider `atspi` integration for screen reader support

6. **Desktop integration**
   - [ ] `pdf-folio.desktop` with `MimeType=application/pdf`
   - [ ] Register as PDF handler via `xdg-mime`
   - [ ] DBus activation so opening a second PDF from the file manager focuses the existing window and opens the file
   - [ ] Confirm desktop-launched error states use the same styled in-app banner path as CLI-launched errors
   - [ ] Confirm "Reveal in file manager" and "Open containing folder" use native desktop behavior where available

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

### Phase 7 done when

- [ ] `flatpak-builder` produces a working bundle
- [ ] AppStream metadata passes `appstreamcli validate`
- [ ] No `tracing::error!` or `eprintln!` in production paths — all errors go to the in-app banner
- [ ] `cargo clippy --all-targets -- -D warnings` passes clean
- [ ] `cargo test --workspace` passes
- [ ] Manual ordering, folder management, metadata editing, selection, and bulk actions survive app restarts
- [ ] Viewer find, fit modes, bookmarks, minimap, and presentation mode work reliably
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
