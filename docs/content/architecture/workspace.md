---
title: Workspace & Crates
eyebrow: Architecture
lede: A source map of the six workspace crates — dependency rules, module trees, and how they compose into the desktop app.
order: 2
---

<p class="trail"><strong>Trail</strong> <a href="overview.md">Overview</a> <span class="sep">·</span> <a href="shell.md">Shell</a> <span class="sep">·</span> <a href="../crates/ui.md">ui</a> <span class="sep">·</span> <a href="../crates/core.md">core</a> <span class="sep">·</span> <a href="../api/index.md">API</a></p>

The workspace was consolidated from twelve crates into **six**. This page is the **structural map of the Rust source**: where modules live, what each crate may depend on, and how those pieces wire together at runtime. Historical consolidation notes live under `scratch/organization.md`; only the **current** tree is described here.

For the Elm-style update loop, start with [Architecture overview](overview.md). For per-crate file maps written for day-to-day navigation, use the [Crates](../crates/main.md) section. API detail is extracted rustdoc under [API Reference](../api/index.md).

## Members

| Crate | Path | Kind | One-line role |
| --- | --- | --- | --- |
| [`iced-widget-patch`](../crates/iced-patch.md) | `crates/iced-widget-patch` | lib (patches `iced_widget`) | Local scrollable override |
| [`pdf-folio-core`](../crates/core.md) | `crates/pdf-folio-core` | lib | PDF + DB + search + import |
| [`pdf-folio-cloud`](../crates/cloud.md) | `crates/pdf-folio-cloud` | lib + bins | Sync, Raindrop, control plane |
| [`pdf-folio-style`](../crates/style.md) | `crates/pdf-folio-style` | lib | KDL themes, tokens, widgets |
| [`pdf-folio-ui`](../crates/ui.md) | `crates/pdf-folio-ui` | lib | iced application |
| [`pdf-folio-main`](../crates/main.md) | `crates/pdf-folio-main` | bin `pdf-folio` | Process entry |

Root `Cargo.toml` also pins iced supporting crates to a fixed git revision and patches `iced_widget` to the local crate (see [iced-widget-patch](#iced-widget-patch)).

## Workspace layout

```text
pdf-folio/
├── Cargo.toml                 # [workspace] members, shared deps, iced [patch]
├── crates/
│   ├── iced-widget-patch/     # path-patched iced_widget (scrollable only)
│   ├── pdf-folio-core/        # UI-free domain I/O
│   ├── pdf-folio-cloud/       # remote services + sync server binary
│   ├── pdf-folio-style/       # design system (KDL + iced chrome)
│   ├── pdf-folio-ui/          # iced state machine + views
│   └── pdf-folio-main/        # thin binary
├── packaging/                 # Docker/systemd for folio-sync-server
├── tests/fixtures/            # sample PDFs for core/UI tests
└── docs/                      # this site
```

Each member has its own `Cargo.toml`. Shared dependency versions are declared once under `[workspace.dependencies]` and referenced as `dep.workspace = true` in crate manifests.

## Dependency rules

These rules are load-bearing. Breaking them reintroduces the old tangle that consolidation removed.

| Rule | Rationale |
| --- | --- |
| `pdf-folio-core` has **zero** iced / wgpu / winit | Core stays testable and usable from CLI/server tools |
| `pdf-folio-cloud` depends on `pdf-folio-core`, not UI | Sync CLI and server share the same DB primitives |
| `pdf-folio-style` may use iced + kdl only | Design system stays free of app/domain state |
| `pdf-folio-ui` may depend on core, cloud, style | Orchestration layer — the only place that holds `PDFolioApp` |
| `pdf-folio-main` stays thin | Binary = CLI parse + `ui::run` or `sync::cli` |

<div class="diagram">                    ┌───────────────────┐
                    │ iced-widget-patch │  workspace [patch.crates-io]
                    └─────────▲─────────┘
                              │ iced_widget
┌──────────────┐    ┌─────────┴─────────┐    ┌────────────────┐
│ pdf-folio-   │    │   pdf-folio-ui    │───▶│ pdf-folio-style│
│ main         │───▶│  (shell/lib/view) │    │  (KDL + iced)  │
└──────┬───────┘    └─────────┬─────────┘    └────────────────┘
       │                      │
       │                      ├──────────────▶ pdf-folio-core
       │                      │                 (pdf/ + db/)
       │                      │
       └──────────────────────┴──────────────▶ pdf-folio-cloud ──▶ pdf-folio-core
                                               (sync/raindrop/server)</div>

**Reading the graph:**

- Arrows mean “depends on / calls into.” Data and domain types flow **up** from core; the UI never appears below the cloud or core layers.
- The desktop process always enters through [pdf-folio-main](../crates/main.md). The control-plane process enters through a **binary inside** [pdf-folio-cloud](../crates/cloud.md) (`pdf-folio-sync-server`), not through main.
- Style is a leaf from UI’s perspective: views take tokens and class helpers; style never opens a database or sync client.

## How the app is assembled

This is the control-flow spine for the **desktop** product. Architecture semantics (one `Message`, one reducer) are in [Overview](overview.md); launch staging is in [Shell](shell.md#launch-sequence) and [Startup performance](startup-performance.md).

### Process entry

<div class="diagram">pdf-folio (main.rs)
  │  tracing_subscriber + clap
  │
  ├─ Some(Command::Sync(…))  ──▶  tokio Runtime
  │                                  └─ pdf_folio_cloud::sync::cli
  │                                       (auth / plan / push / pull / sync-once …)
  │
  └─ None  ────────────────────▶  pdf_folio_ui::run_with_process_start(file?, t0)
                                       │
                                       ├─ load AppSession (unless CLI file wins)
                                       ├─ load multi-library registry + open Db
                                       ├─ StyleBook::load → AppearanceRuntime
                                       ├─ PDFolioApp::with_initial_file_and_session
                                       └─ iced::application(boot, update, view)
                                            .subscription(…)
                                            .run()</div>

| Path | Crate(s) that run | Owns process? |
| --- | --- | --- |
| Desktop UI | main → **ui** (+ core, cloud, style as libraries) | yes |
| `pdf-folio sync …` | main → **cloud** `sync/cli` (+ core) | yes (Tokio only) |
| Control plane | **cloud** bin `pdf-folio-sync-server` | separate process |
| Maintenance | cloud bins `crdt-sync-once`, `ensure-turso-schema` | ops tools |

Main re-exports sync CLI via [`cli.rs`](../api/pdf-folio-main/cli.md); the implementation lives in [`pdf-folio-cloud/src/sync/cli.rs`](../api/pdf-folio-cloud/sync/cli.md). See [CLI reference](../operations/cli.md).

### iced loop inside pdf-folio-ui

Once `run` starts iced, every frame and every event stays inside the UI crate:

<div class="diagram">subscription / input / Task result
        │
        ▼
   <span class="hl">Message</span>  ──▶  shell::update
                    │
                    ├─ library::update  → Some(Task) | None
                    ├─ viewer::update   → Some(Task) | None
                    └─ shell match      → auth, chrome, startup, registry, …
                    │
                    ▼
              Task&lt;Message&gt;  (spawn_blocking / async)
                    │          calls into core / cloud
                    ▼
   view(app)  ←── components::shared::root_surface
                    │
                    ├─ AppMode::SignedOut      → sign-in surface
                    ├─ AppMode::LibrarySwitcher→ vault picker
                    ├─ AppMode::Viewer + doc   → viewer::view
                    └─ else                    → library::view
                    + stacked chrome (palette, menus, dialogs, banners)</div>

| Concern | Module | Guide |
| --- | --- | --- |
| Root state | [`shell/app.rs`](../api/pdf-folio-ui/shell/app.md) → `PDFolioApp` | [Runtime state](state.md) |
| Event vocabulary | [`shell/messages.rs`](../api/pdf-folio-ui/shell/messages.md) | [Message surface](messages.md) |
| Top-level reducer | [`shell/update.rs`](../api/pdf-folio-ui/shell/update.md) | [Overview · routing](overview.md#update-routing) |
| Root view | [`components/shared/root_surface.rs`](../api/pdf-folio-ui/components/shared/root_surface.md) | [Shell · chrome](shell.md#chrome) |
| Side effects | `library/tasks`, `viewer/tasks`, `shell/tasks` | [Overview · tasks](overview.md#side-effects-as-tasks) |

**Invariant:** only `update` mutates `PDFolioApp`. Core and cloud return plain data / `Result`s; background work proposes changes by emitting messages.

### Layer cake

| Layer | Crate(s) | What it may do | What it must not do |
| --- | --- | --- | --- |
| Binary | [main](../crates/main.md) | Parse CLI, init tracing, pick UI vs sync | Open SQLite / Pdfium / iced widgets |
| Presentation | [ui](../crates/ui.md) + [style](../crates/style.md) | Own state machine, views, chrome, tokens | Put SQL or Pdfium loops in view functions |
| Domain I/O | [core](../crates/core.md) | PDF open/render/text; SQLite; Tantivy; FS watch | Depend on iced or cloud HTTP |
| Cloud | [cloud](../crates/cloud.md) | OAuth, CRDT, R2, Raindrop HTTP, control plane | Import iced / hold `PDFolioApp` |
| Platform patch | [iced-widget-patch](../crates/iced-patch.md) | Override scrollable only | Grow into a second UI framework |

---

## Crate module maps

The subsections below walk each crate’s source tree the way a maintainer reads it on disk. Prefer these maps when deciding *where a file should live*; use subsystem pages when deciding *how a feature behaves*.

### pdf-folio-main

**Path:** `crates/pdf-folio-main/` · **Binary:** `pdf-folio` · [Guide](../crates/main.md) · [API](../api/pdf-folio-main/index.md)

```text
pdf-folio-main/src/
  main.rs    # Args, tracing, dispatch to UI or sync
  cli.rs     # re-export pdf_folio_cloud::sync::cli::{SyncArgs, run_sync_command}
```

This crate must stay small. If `main.rs` starts importing `Db`, `PdfDoc`, or iced widgets, the boundary has slipped — push the work into ui or cloud.

**Depends on:** `pdf-folio-ui`, `pdf-folio-cloud`, `clap`, `tokio`, `anyhow`, `tracing-subscriber`.

### pdf-folio-ui

**Path:** `crates/pdf-folio-ui/` · [Guide](../crates/ui.md) · [API](../api/pdf-folio-ui/index.md)

This is the largest crate. Navigation by **subtree** matters more than reading `lib.rs` end-to-end — `lib.rs` is the launch entry (`run` / `run_with_process_start`), a re-export hub, and a few cross-cutting task helpers.

```text
pdf-folio-ui/src/
  lib.rs                 # run(), font/window wiring, session-save helpers
  tests.rs               # large integration-style UI tests
  shell/                 # process-level orchestration
  library/               # library-mode domain
  viewer/                # viewer-mode domain
  components/            # presentational building blocks
    shared/
    library/
    viewer/
  assets/icons/          # SVG icons (folder, overflow, …)
```

<div class="diagram">pdf-folio-ui
├── <span class="hl">shell</span> ──────── PDFolioApp, Message, top update, session, shortcuts
├── <span class="hl">library</span> ────── domain update/tasks/view + multi-library registry
├── <span class="hl">viewer</span> ─────── document runtime, render tasks, canvas composition
└── <span class="hl">components</span> ── pure-ish widgets (shared / library / viewer)
         │
         │  uses
         ▼
   pdf-folio-core · pdf-folio-cloud · pdf-folio-style</div>

#### `shell/` — process-level app

| File | Responsibility |
| --- | --- |
| [`app.rs`](../api/pdf-folio-ui/shell/app.md) | `PDFolioApp`, `AppMode`, `LibraryRuntime`, `ChromeRuntime`, `AppearanceRuntime`, `Settings` |
| [`messages.rs`](../api/pdf-folio-ui/shell/messages.md) | Single `Message` enum + menu/context/confirmation types |
| [`update.rs`](../api/pdf-folio-ui/shell/update.md) | Top-level reducer; library → viewer → shell match |
| [`commands.rs`](../api/pdf-folio-ui/shell/commands.md) | Command palette / menu registry (`CommandId`) |
| [`shortcuts.rs`](../api/pdf-folio-ui/shell/shortcuts.md) | Key chord → message |
| [`subscriptions.rs`](../api/pdf-folio-ui/shell/subscriptions.md) | iced subscription tree |
| [`session.rs`](../api/pdf-folio-ui/shell/session.md) | `AppSession` + `SyncAuthRuntime` load/save |
| [`tasks.rs`](../api/pdf-folio-ui/shell/tasks.md) | Shell-owned async (auto-sync, registry fan-out) |
| [`platform.rs`](../api/pdf-folio-ui/shell/platform.md) | Linux file-manager reveal helpers |
| [`constants.rs`](../api/pdf-folio-ui/shell/constants.md) | Shared timings, widget ids |

Deep dive: [Application shell](shell.md).

#### `library/` — library mode domain

| Module | Role |
| --- | --- |
| [`update.rs`](../api/pdf-folio-ui/library/update.md) | Library message handler (`Option<Task>`) |
| [`tasks.rs`](../api/pdf-folio-ui/library/tasks.md) | Import, bulk ops, Raindrop, export, search tasks |
| [`actions.rs`](../api/pdf-folio-ui/library/actions.md) | Intent helpers on `PDFolioApp` (selection, clipboard, history) |
| [`state.rs`](../api/pdf-folio-ui/library/state.md) | Viewport windowing, masonry, flash animations |
| [`data.rs`](../api/pdf-folio-ui/library/data.md) | Derived lists (tags, visible refresh, thumb windows) |
| [`layout.rs`](../api/pdf-folio-ui/library/layout.md) | Zoom limits, filtered/sorted entries, scroll geometry |
| [`thumbnails.rs`](../api/pdf-folio-ui/library/thumbnails.md) | Cover cache keys + render tasks |
| [`registry/`](../api/pdf-folio-ui/library/registry.md) | Multi-vault profiles (`libraries.json`), previews, switcher tasks |
| [`view/`](../api/pdf-folio-ui/library/view.md) | Compose library screen (`root`, `entries`, `folders`, `sidebar`) |

`library/mod.rs` re-exports pure helpers from `components/library` (`drag`, `filters`, `metadata`, `selection`) so domain code and components share one path. Multi-library behavior: [Multi-library registry](../subsystems/multi-library.md). End-to-end bulk edit: [Life of a bulk action](../subsystems/bulk-action.md).

#### `viewer/` — viewer mode domain

| Module | Role |
| --- | --- |
| [`document.rs`](../api/pdf-folio-ui/viewer/document.md) | `ViewerRuntime` fields on `PDFolioApp` |
| [`state.rs`](../api/pdf-folio-ui/viewer/state.md) | Scroll/spread, selection, find; app constructors |
| [`update.rs`](../api/pdf-folio-ui/viewer/update.md) | Zoom, viewport, outline, find, render results |
| [`tasks.rs`](../api/pdf-folio-ui/viewer/tasks.md) | Open document, page render, zoom debounce |
| [`rendering.rs`](../api/pdf-folio-ui/viewer/rendering.md) | Zoom presets, settle policy |
| [`navigation.rs`](../api/pdf-folio-ui/viewer/navigation.md) | Jump / scroll / zoom methods |
| [`layout.rs`](../api/pdf-folio-ui/viewer/layout.md) | Spread groups, page rect helpers |
| [`view/`](../api/pdf-folio-ui/viewer/view.md) | Toolbar + sidebar + canvas composition |

Rendering pipeline details (tiles, zoom without flicker, text layer): [Rendering](../subsystems/rendering.md).

#### `components/` — UI building blocks

Prefer **geometry and presentation** here and **persistence** in `library/tasks` or core. Components emit `Message`s or take message callbacks; they should not open SQLite.

| Subtree | Examples |
| --- | --- |
| [`shared/`](../api/pdf-folio-ui/components/shared.md) | command palette, context menu, menus, sidebar chrome, sync indicator, root surface, banners, icons |
| [`library/`](../api/pdf-folio-ui/components/library.md) | cards, drag math, selection, filters, dialogs, inspector, folder tree, import status |
| [`viewer/`](../api/pdf-folio-ui/components/viewer.md) | canvas, toolbar, zoom, find bar, outline, page controls, sidebar |

Styled primitives (toolbar buttons, tag pills, card chrome) often come from **`pdf-folio-style`**, not local color literals.

**Depends on:** `pdf-folio-core`, `pdf-folio-cloud`, `pdf-folio-style`, `iced`, `tokio`, `notify`, `rfd`, …

### pdf-folio-core

**Path:** `crates/pdf-folio-core/` · [Guide](../crates/core.md) · [API](../api/pdf-folio-core/index.md)

UI-free foundation. Public types are re-exported from [`lib.rs`](../api/pdf-folio-core/index.md) so callers write `pdf_folio_core::{Db, PdfDoc, …}`.

```text
pdf-folio-core/src/
  lib.rs
  pdf/
    mod.rs
    document.rs    # PdfDoc, outline, text layer, render_page
    renderer.rs    # TileKey, TileCache (LRU)
    geometry.rs    # TextRect
    tests.rs
  db/
    mod.rs         # Db path handle + submodule glue
    types.rs       # EntryId, LibraryEntry, Folder, preferences, …
    schema.rs      # open, migrate, CREATE TABLE
    library.rs     # entry CRUD, trash, lookup
    organization.rs# folders, tags, membership, snapshots, ordering
    naming.rs      # private sort-key / gap-order helpers
    metadata.rs    # display overrides, preferences, progress
    import.rs      # BLAKE3 hash, import, thumbnails, LibraryWatcher
    search.rs      # SearchIndex (Tantivy)
    raindrop.rs    # raindrop_* mapping tables only (no HTTP)
    sync.rs        # local CRDT/ops tables, seeding, checkpoints
    tests.rs (+ import/tests, search/tests)
```

<div class="diagram">pdf-folio-core
├── <span class="hl">pdf/</span>   PdfDoc · TileCache · OutlineNode · PageTextLayer
│         ▲ used by viewer tasks + thumbnail render
│
└── <span class="hl">db/</span>    Db (path → short-lived rusqlite connections)
          ├── schema / library / organization / metadata
          ├── import + LibraryWatcher
          ├── search (Tantivy)
          ├── raindrop (mapping rows)
          └── sync (local CRDT seed for cloud)</div>

| Subsystem guide | Core modules |
| --- | --- |
| [Rendering](../subsystems/rendering.md) | `pdf/document`, `pdf/renderer`, `pdf/geometry` |
| [Library database](../subsystems/database.md) | `db/schema`, `library`, `organization`, `metadata`, `types` |
| [Search & watching](../subsystems/search.md) | `db/search`, `db/import` (watcher) |
| [Sync](../subsystems/sync.md) (local half) | `db/sync` |
| [Raindrop](../subsystems/raindrop.md) (local half) | `db/raindrop` |

**Rule:** no iced, no Raindrop/sync HTTP clients. Network lives in cloud; core only stores what cloud and UI write.

### pdf-folio-style

**Path:** `crates/pdf-folio-style/` · [Guide](../crates/style.md) · [API](../api/pdf-folio-style/index.md) · [Style system](../subsystems/style-system.md)

```text
pdf-folio-style/src/
  lib.rs           # font bytes, re-exports, ui_font / display_font
  book/            # StyleBook load/parse (KDL)
    parser.rs
    sources.rs     # bundled vs user XDG paths
  tokens.rs        # ThemeTokens, Spacing, FontSize, layout tokens
  classes/         # Class → iced style closures
    core.rs, library.rs, viewer.rs
  components/      # toolbar_button, tag_pill, library_card, …
    core.rs, library.rs, viewer.rs
  borders/         # side_border helpers
  theme.rs         # AppTheme bridge (Light / Dark)
styles/            # KDL sources (dev hot-reload)
  application.kdl
  themes/{espresso,light}.kdl
  components/…
assets/fonts/      # IBM Plex Sans + Vollkorn
```

<div class="diagram">bundled KDL (+ styles/ in checkout)
        │
        ▼
  StyleBook::load()
        ├── themes     → ThemeTokens
        ├── components → ClassStyle per ComponentState
        └── application layout / labels
        │
        ▼
  user overrides: $XDG_CONFIG_HOME/pdf-folio/styles/**/*.kdl
        │
        ▼
  Arc&lt;StyleBook&gt; on AppearanceRuntime  →  view paints with tokens/classes</div>

**Depends on:** `iced`, `kdl` only. Helpers must not read `PDFolioApp`, `Db`, or document state — only labels, tokens, and message callbacks.

### pdf-folio-cloud

**Path:** `crates/pdf-folio-cloud/` · [Guide](../crates/cloud.md) · [API](../api/pdf-folio-cloud/index.md)

Three products in one crate, all network-facing:

```text
pdf-folio-cloud/src/
  lib.rs
  sync/                 # desktop client + CLI surface
    auth.rs             # Google OAuth (PKCE)
    session.rs          # cached Session on disk
    client.rs           # SyncClient coordinator
    remote.rs           # Turso / Hrana client
    blobs.rs            # R2 client + local BlobCache
    crdt.rs             # CRDT ops, LWW, materialization
    run.rs              # preflight + sync_library_if_needed
    status.rs           # report types, REGISTRY_LIBRARY_ID
    cli.rs              # pdf-folio sync subcommands
  raindrop/             # Raindrop.io HTTP + import pipeline
    auth.rs, client.rs, types.rs, import.rs, matching.rs
  server/               # control-plane HTTP service
    config.rs, auth.rs, handlers.rs, storage.rs
  bin/
    pdf-folio-sync-server.rs
    crdt-sync-once.rs
    ensure-turso-schema.rs
turso_schema.sql
```

<div class="diagram">Desktop (ui / sync CLI)
        │  Google PKCE + session JWT
        ▼
  folio-sync-server (cloud::server)     ← identity + short-lived credentials only
        │  Turso token · R2 presign
        ▼
  Turso (CRDT metadata)     R2 (blobs/&lt;blake3&gt;.pdf)
        ▲                         ▲
        └──── cloud::sync ────────┘
              uses core::db for local rows + CRDT log</div>

| Surface | Entry | Purpose |
| --- | --- | --- |
| Library API | `pdf_folio_cloud::{sync, raindrop, server}` | Used by UI and CLI |
| Control plane | bin `pdf-folio-sync-server` | Identity + short-lived credentials |
| Maintenance | `crdt-sync-once`, `ensure-turso-schema` | Ops tooling |

Packaging still builds the control-plane binary as:

```bash
cargo build --release -p pdf-folio-cloud --bin pdf-folio-sync-server
```

The binary name `pdf-folio-sync-server` is intentional; there is no separate crate by that name. Full design: [Cross-device sync](../subsystems/sync.md) · [Raindrop](../subsystems/raindrop.md) · [Packaging](../operations/packaging.md).

**Depends on:** `pdf-folio-core` + HTTP/crypto stack (`axum`, `reqwest`, `jsonwebtoken`, …). **Not** UI.

### iced-widget-patch

**Path:** `crates/iced-widget-patch/` · [Guide](../crates/iced-patch.md) · [API](../api/iced-widget-patch/index.md)

```text
iced-widget-patch/
  Cargo.toml           # package name = iced_widget (shadows crates.io)
  src/
    lib.rs             # pub use iced_widget_upstream::*; + local scrollable
    scrollable.rs      # left-scrollbar placement override
```

Root `Cargo.toml` maps `iced_widget` to this path and pins every iced supporting crate to the **same** git revision. Any `iced::widget::scrollable` in ui/style transparently gets the patched type. Keep the override to scrollable only; bump all iced revs together when upgrading.

---

## Cross-crate feature walks

These walks show how modules **across crates** cooperate for real features. They are maps, not full subsystem designs — follow the linked deep dives for behavior detail.

### Open a PDF from the library

<div class="diagram">User double-clicks a card
  → Message (library open entry)
  → library::update / shell open path
  → viewer::tasks::open_document_task
       ├─ pdf_folio_core::PdfDoc::open(path)     # page count, handle
       ├─ Db reading progress / entry id
       └─ completion Message
  → mode = Viewer, ViewerRuntime filled
  → view_viewer → components/viewer/canvas
       └─ render tasks → PdfDoc::render_page + TileCache
  → Style tokens color toolbar / chrome</div>

| Step | Crate | Module |
| --- | --- | --- |
| Selection / open intent | ui | `library/update`, `library/actions` |
| Async open + raster | ui | `viewer/tasks`, `viewer/update` |
| Bytes → pages/tiles | core | `pdf/document`, `pdf/renderer` |
| Progress / last page | core | `db/metadata` (via ui tasks) |
| Chrome paint | style | tokens + viewer component classes |

See [Rendering · opening a document](../subsystems/rendering.md#opening-a-document).

### Import a folder of PDFs

<div class="diagram">Import message
  → library::tasks::import_folder_with_index
       ├─ core::import_folder / hash_file (BLAKE3 EntryId)
       ├─ core::Db insert rows
       ├─ SearchIndex update (Tantivy)
       └─ thumbnail_path + later thumbnail tasks
  → completion Message refreshes LibraryRuntime lists
  → next auto-sync seeds CRDT ops from db::sync</div>

| Concern | Owner |
| --- | --- |
| Hash, scan, DB rows | [core `db/import`](../api/pdf-folio-core/db/import.md) |
| Full-text index | [core `db/search`](../api/pdf-folio-core/db/search.md) |
| Task orchestration + UI status | [ui `library/tasks`](../api/pdf-folio-ui/library/tasks.md) |
| Sync dirty → remote | [cloud `sync`](../api/pdf-folio-cloud/sync.md) after local write |

### Automatic sync pass

<div class="diagram">subscription timer / StartupBackgroundReady
  → shell::tasks::auto_sync_library_task
  → cloud::sync::run::sync_library_if_needed
       ├─ Session (cloud::sync::session)
       ├─ seed from core::db::sync
       ├─ upload blobs → R2 (cloud::sync::blobs)
       ├─ push/pull CRDT → Turso (cloud::sync::crdt + remote)
       └─ materialize LWW → local Db rows
  → status Message updates chrome / library indicator</div>

Control-plane credential minting is separate from the data path: the server never proxies PDF bytes or SQL. See [Sync · a sync pass](../subsystems/sync.md#a-sync-pass).

### Paint with the style book

<div class="diagram">view(app)
  → tokens = app.appearance.theme.tokens(&style_book)
  → library/viewer views call style::components / classes
  → iced widgets with Class stylesheets
  → optional: FS watch / Reload Styles
       → StyleBook::load again → replace AppearanceRuntime</div>

| Belongs in style | Belongs in UI |
| --- | --- |
| Colors, radii, spacing, class styles | Message routing, selection, drag |
| Reusable chrome with no DB | Import progress, sync auth state |
| KDL under `styles/` | Pdfium / SQLite work |

---

## What was absorbed (for blame / git archaeology)

| Current crate | Former crates / areas |
| --- | --- |
| `pdf-folio-core` | `pdf-folio-core` + `pdf-folio-db` + Raindrop *mapping* tables |
| `pdf-folio-cloud` | `pdf-folio-sync` + `pdf-folio-sync-server` + Raindrop HTTP/import |
| `pdf-folio-ui` | `pdf-folio-ui` + `pdf-folio-ui-components` + `pdf-folio-viewer` |
| `pdf-folio-style` | same crate, re-split into `book` / `classes` / `components` / `borders` |

When `git blame` shows old paths, map them through this table rather than looking for removed crate directories.

## Shared workspace dependencies

Notable centralized deps in root `Cargo.toml`:

| Dependency | Used for |
| --- | --- |
| `iced` 0.14 | UI (ui + style) |
| `pdfium-render` | PDF open/render/text/outline (core) |
| `rusqlite` (bundled) | Library DB (core) |
| `tantivy` | Full-text search (core) |
| `notify` | FS watch for imports and styles (core, ui) |
| `axum` / `jsonwebtoken` / `reqwest` | Sync server and clients (cloud) |
| `kdl` | Style book (style) |
| `blake3` | Content-addressed entry IDs and blob keys (core, cloud) |
| `clap` / `tokio` | CLI and async runtimes (main, cloud, ui) |

Versions are pinned in the workspace so crates do not drift. Iced supporting crates are patched to a single git rev alongside `iced-widget-patch`.

## Where to put new code

| If you are adding… | Put it in… | Avoid… |
| --- | --- | --- |
| Pdfium helpers, tile cache, geometry | `pdf-folio-core/src/pdf/` | UI modules calling Pdfium directly for heavy work |
| Schema, queries, import, search | `pdf-folio-core/src/db/` | Duplicating SQL in ui or cloud |
| iced views, messages, drag/selection | `pdf-folio-ui` (shell / library / viewer / components) | Growing `main.rs` |
| Domain update + async tasks | `library/` or `viewer/` domain modules | Putting DB I/O inside `components/` |
| Pure layout / presentation math | `components/` | Opening `Db` from components |
| Colors, radii, reusable chrome widgets | `pdf-folio-style` (+ KDL under `styles/`) | Hard-coded colors in views |
| OAuth, Turso/R2, CRDT, Raindrop HTTP | `pdf-folio-cloud` | Network code in core |
| CLI flags for the desktop binary | `pdf-folio-main` (thin) + cloud `sync/cli` for sync | Domain logic in main |

**Message placement rule of thumb** (also in [Messages](messages.md#how-to-add-a-new-message)):

1. Add the variant in `shell/messages.rs` near related variants.
2. Handle it in `library/update` or `viewer/update` when it only touches that domain.
3. Handle it in `shell/update` only when it spans modes (auth, global menus, style reload, vault switch).

## Per-crate guides

- [pdf-folio-main](../crates/main.md) · [API](../api/pdf-folio-main/index.md)
- [pdf-folio-core](../crates/core.md) · [API](../api/pdf-folio-core/index.md)
- [pdf-folio-ui](../crates/ui.md) · [API](../api/pdf-folio-ui/index.md)
- [pdf-folio-style](../crates/style.md) · [API](../api/pdf-folio-style/index.md)
- [pdf-folio-cloud](../crates/cloud.md) · [API](../api/pdf-folio-cloud/index.md)
- [iced-widget-patch](../crates/iced-patch.md) · [API](../api/iced-widget-patch/index.md)

The API pages are **rustdoc rendered in this site’s theme** (not the default rustdoc HTML skin). Edit `//!` / `///` in the crates; rebuild docs to refresh.

## Related architecture pages

| Page | When you need it |
| --- | --- |
| [Overview](overview.md) | Elm loop, update routing, task patterns |
| [Startup performance](startup-performance.md) | TTFP vs TTI, staged launch |
| [Application shell](shell.md) | Session, auth, subscriptions, chrome |
| [Message surface](messages.md) | Clusters and how to add events |
| [Runtime state](state.md) | `PDFolioApp` field ownership |
