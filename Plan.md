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
│   │       └── theme.rs        ← color tokens, light/dark
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
   - Added `tests/fixtures/phase1-multipage.pdf`, a 24-page manual-test fixture with a core test
     asserting the expected page count.

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
   - Top toolbar: open button, zoom controls, page indicator ("12 / 248"), view toggle
   - Left sidebar (collapsible): TOC panel placeholder
   - Main area: scrollable PDF canvas
   - Use iced's `pane_grid` or manual `row![ sidebar, canvas ]` layout

2. **Theme system** (`pdf-folio-ui/src/theme.rs`)
   - Define `AppTheme` enum: `Light`, `Dark`
   - Implement `iced::application::StyleSheet` for both
   - Color tokens: background, surface, text-primary, text-secondary, accent, border
   - Toggle with keyboard shortcut `Ctrl+Shift+T`

3. **Table of contents panel**
   - Implement `PdfDoc::outline()` returning `Vec<OutlineNode>` where `OutlineNode { title, page, children }`
   - Render as a nested list in the sidebar
   - Clicking a node sends `Message::JumpToPage(n)`
   - `JumpToPage` sets `scroll_offset` to the y-position of that page

4. **File open dialog**
   - `Message::OpenFileDialog` → use `rfd::AsyncFileDialog` to show native file picker
   - Filter to `*.pdf`
   - On selection, send `Message::FileSelected(path)`

5. **Keyboard navigation**
   - `Space` / `Shift+Space`: page down / up
   - `Arrow keys`: fine scroll
   - `Ctrl+G`: jump-to-page dialog (simple text input overlay)
   - `Escape`: close any open panel or dialog

6. **Window title**
   - Set to `"<filename> — PDF-Folio"` when a document is open
   - Set to `"PDF-Folio"` otherwise

### Phase 2 done when

- [ ] Application looks and feels like a real native app
- [ ] TOC panel works and navigates correctly
- [ ] Dark/light theme toggle works across all surfaces
- [ ] File open dialog works

---

## Phase 3 — Library manager (weeks 8–11)

**Goal:** import, browse, search, and tag a collection of PDFs.

### Tasks in order

1. **Database setup** (`pdf-folio-library/src/db.rs`)
   - On first run, create SQLite database at `$XDG_DATA_HOME/pdf-folio/library.db`
   - Run schema migrations using a simple version table (no ORM, raw SQL)
   - Implement: `insert_entry`, `get_all_entries`, `update_last_page`, `add_tag`, `remove_tag`, `delete_entry`

2. **Library view** (`pdf-folio-ui/src/views/library.rs`)
   - Default view when no PDF is open
   - Grid layout: cover thumbnail + title + author
   - List layout: compact rows with metadata
   - Toggle between grid and list with a toolbar button
   - Virtual list: only render visible entries (critical for large collections)

3. **Cover thumbnail extraction**
   - On import, call `render_page(0, 200)` on a background thread
   - Store thumbnail bytes in `$XDG_CACHE_HOME/pdf-folio/thumbs/<entry_id>.rgba`
   - Load thumbnails lazily as they scroll into view

4. **Filesystem watcher**
   - User configures one or more watch directories in settings
   - `notify` watcher runs in background, sends events to a channel
   - On `Create` event for `*.pdf`: compute blake3 hash, insert into DB if not duplicate, extract thumbnail
   - On `Remove` event: mark entry as missing (do not delete from DB)

5. **Import flow**
   - "Import folder" button in library view: show folder picker, recursively scan for PDFs, import all
   - Progress indicator during bulk import

6. **Search**
   - Search bar in library view header
   - As user types (debounced 200ms), query tantivy for matching entries
   - Results replace library grid
   - Empty query restores full library view
   - Show matching page number in result card if query matched page content

7. **Tags and collections**
   - Right-click entry → "Add tag" → inline text input
   - Tags displayed as pills on entry cards
   - Tag filter sidebar: click a tag to filter library to entries with that tag

8. **Reading progress**
   - When viewer is open, periodically (on scroll) send `Message::ProgressUpdated { entry_id, page }`
   - Save to DB with `update_last_page`
   - Show progress bar on library card (current_page / page_count)
   - "Continue reading" opens to last page

### Phase 3 done when

- [ ] Import 500 PDFs without crashing or hanging the UI
- [ ] Search returns results in under 200ms
- [ ] Cover thumbnails load without janking scroll
- [ ] Tags persist across app restarts

---

## Phase 4 — Viewer features (weeks 12–15)

**Goal:** annotations, advanced navigation, presentation mode.

### Tasks in order

1. **Text selection model**
   - Extract text positions from pdfium: `page.text().chars()` gives glyph bounds
   - Build a `TextMap` for each page: `Vec<GlyphRect { char, page_rect }>`
   - Hit-test mouse position against `TextMap` to find selection boundaries
   - Render selection highlight as a colored overlay rect on the canvas

2. **Highlight annotation**
   - On mouse release after selection, show toolbar: Highlight / Underline / Strikethrough / Cancel
   - Create `Annotation { kind: Highlight, page, rects: Vec<Rect>, color: Color }`
   - Store in DB and render as transparent colored rects in canvas draw pass

3. **Note annotation**
   - Click the note tool → click on canvas → creates a `Annotation { kind: Note, page, position, body: String }`
   - Render as a small icon on the page
   - Click icon → popover with editable text

4. **Freehand drawing**
   - Pen tool: capture mouse drag as `Vec<Point>`, store as `Annotation { kind: Drawing, strokes }`
   - Render as SVG path overlay on canvas
   - Eraser tool: hit-test strokes and delete on click

5. **Annotation export to PDF**
   - "Export with annotations" → use `pdf-writer` to create a new PDF with annotations embedded as PDF standard annotation objects
   - This is a background async operation; show progress

6. **Two-page spread mode**
   - Toggle button in toolbar
   - Layout engine places even pages left, odd pages right
   - Scroll and zoom still work the same way

7. **Presentation mode**
   - `F5` → full-screen, hide all chrome, show only the PDF
   - Click or right arrow → next page
   - `Escape` → exit

8. **Minimap**
   - Thin vertical strip on right edge of viewer
   - Renders tiny page thumbnails (reuse cache) stacked vertically
   - Shows a viewport indicator rect
   - Click/drag on minimap scrolls the main view

### Phase 4 done when

- [ ] Highlights and notes persist and re-render correctly on reopen
- [ ] Annotation export produces a valid PDF (test with pdfinfo or opening in another viewer)
- [ ] Two-page spread mode works correctly
- [ ] Presentation mode is full-screen with no visible chrome

---

## Phase 5 — Polish and ship (weeks 16–20)

**Goal:** production-quality binary ready for Flathub submission.

### Tasks in order

1. **Error handling audit**
   - Every `anyhow::Error` must be surfaced to the user (never silently swallowed)
   - Add an in-app error banner: dismissable, shows human-readable message
   - Corrupted PDF → show error in viewer area, not a panic

2. **Performance profiling**
   - Add `tracing::instrument` to all async tasks and render paths
   - Use `tracing-subscriber` with JSON output for flamegraph analysis
   - Target metrics:
     - Frame time: < 8ms (120fps budget) during scroll
     - Tile render time: < 200ms per page at 800px width
     - Library load time: < 500ms for 1000 entries
     - Search latency: < 100ms for any query

3. **Settings persistence**
   - Store settings in `$XDG_CONFIG_HOME/pdf-folio/config.toml`
   - Settings: theme, default zoom, watch directories, tile cache size, sidebar width
   - Live reload: watch config file and apply changes without restart

4. **Accessibility**
   - All interactive elements reachable by keyboard
   - Focus ring visible on all focused elements
   - Window title updates on navigation
   - Consider `atspi` integration for screen reader support (stretch goal)

5. **Desktop integration**
   - `pdf-folio.desktop` with `MimeType=application/pdf`
   - Register as PDF handler via `xdg-mime`
   - DBus activation so opening a second PDF from the file manager focuses the existing window and opens the file

6. **Flatpak manifest** (`packaging/dev.pdf-folio.PDF-Folio.yml`)
   - Runtime: `org.freedesktop.Platform//23.08`
   - Permissions: `--filesystem=home:ro` for library access, `--socket=wayland`, `--socket=fallback-x11`
   - Bundle pdfium binary as a module
   - Test with `flatpak-builder --run`

7. **AppImage and .deb**
   - AppImage via `appimagetool` for portable single-binary distribution
   - `.deb` via `cargo-deb` for Debian/Ubuntu users

8. **Plugin API** (if time allows)
   - Define a `PDF-FolioPlugin` WASM interface: `on_import(path: str) -> Metadata`, `render_sidebar() -> iced::Element`
   - Load plugins from `$XDG_DATA_HOME/pdf-folio/plugins/*.wasm`
   - Sandbox via `wasmtime` with explicit capability grants

### Phase 5 done when

- [ ] `flatpak-builder` produces a working bundle
- [ ] AppStream metadata passes `appstreamcli validate`
- [ ] No `tracing::error!` or `eprintln!` in production paths — all errors go to the in-app banner
- [ ] `cargo clippy --all-targets -- -D warnings` passes clean
- [ ] `cargo test --workspace` passes

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

---
