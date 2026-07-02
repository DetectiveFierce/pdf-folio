# PDF-Folio Reorganization Plan

This document tracks the restructuring work for PDF-Folio. The goal is to preserve the full feature set while reducing large modules, removing redundant helper logic, and making the codebase easier to navigate.

## Goals

- [ ] Preserve the current viewer, library, search, tagging, folder, metadata, menu, style, and watcher feature set.
- [ ] Keep crate boundaries clear:
  - `pdf-folio-core`: PDF loading, rendering, tile cache, annotations. No UI dependencies.
  - `pdf-folio-library`: SQLite storage, import, search index, filesystem watching. No UI dependencies.
  - `pdf-folio-ui`: iced application, views, messages, tasks, styling, platform UI integration.
  - `pdf-folio-main`: CLI/bootstrap only.
- [ ] Reduce duplicated helper logic around metadata cleanup, file sizes, thumbnail paths, labels, filtering, platform commands, and drag/reorder behavior.
- [ ] Make each module answer a clear ownership question: state, update, view, task, domain helper, platform helper, or style.
- [x] Keep `crates/pdf-folio-ui/src/` tidy: `app.rs` is the only root source file, and every other source file lives in a responsibility-based module directory.
- [x] Avoid one-directory-per-file organization. Directories should group related modules, such as app shell, library UI, style, and views.

## UI Module Layout Preference

`crates/pdf-folio-ui/src/` should not collect loose root-level files. The preferred layout is:

```text
crates/pdf-folio-ui/src/
  app.rs              # crate root and current composition root
  app/                # app-shell support: messages, platform, shortcuts, subscriptions, menus, tasks
  library/            # library UI state, filtering, metadata, thumbnails, drag/drop, cards, rows, sidebar
  style/              # style system, tokens, classes, theme selection, style book parsing
  views/              # view modules while they remain as separate view scaffolds
```

Rules:

- `src/app.rs` is the only file allowed directly under `src/`.
- New UI support modules should be added to an existing responsibility directory when possible.
- Create a new directory only when it will group a real family of related modules.
- Do not create a directory just to wrap a single former loose file.
- If a former loose file is moved, move it into the directory that owns its responsibility:
  - app messages, shortcuts, platform integration, subscriptions, menu routing, and app tasks belong under `app/`.
  - theme selection belongs with the style system under `style/`.
  - library state and behavior belong under `library/`.
  - viewer-specific state and behavior should eventually belong under `viewer/` once enough related modules exist.

## Phase 1: Protect Behavior

- [x] Confirm the existing test baseline before restructuring.
  - Baseline: `cargo test` passed with 64 tests.
- [x] Run tests after each meaningful extraction.
- [ ] Move tests closer to the modules they cover instead of keeping most UI tests in `app::tests`.
- [ ] Add focused characterization tests for:
  - library filtering/search/sort visibility
  - folder drag/drop target behavior
  - selection and range selection
  - thumbnail cache key/path behavior
  - metadata cleanup and attribution helpers
  - app menu action routing
- [ ] Add a regular verification gate:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets`
  - `cargo test`

## Phase 2: Split `pdf-folio-ui/src/app.rs`

Target structure:

```text
crates/pdf-folio-ui/src/
  app.rs
  app/
    messages.rs
    platform.rs
    state.rs
    update.rs
    subscriptions.rs
    shortcuts.rs
    menu.rs
    tasks.rs
  viewer/
    mod.rs
    state.rs
    update.rs
    view.rs
    canvas.rs
    tasks.rs
    outline.rs
    labels.rs
  library/
    mod.rs
    state.rs
    update.rs
    view.rs
    sidebar.rs
    cards.rs
    rows.rs
    selection.rs
    filters.rs
    folders.rs
    drag.rs
    thumbnails.rs
    metadata.rs
    tasks.rs
    labels.rs
```

Completed:

- [x] Create `crates/pdf-folio-ui/src/library/` as the first feature-oriented UI module group.
- [x] Move selection and manual reorder helpers into `library/selection.rs`.
- [x] Move library metadata display/formatting helpers into `library/metadata.rs`.
- [x] Move library filtering/search/reading-state helpers into `library/filters.rs`.
- [x] Move library drag/drop state, constants, and geometry/order helpers into `library/drag.rs`.
- [x] Move library view state enums into `library/state.rs`.
- [x] Move platform file-manager command helpers into `app/platform.rs`.
- [x] Update `messages.rs` to import library state types from `library/state.rs`.
- [x] Group root UI support files so `crates/pdf-folio-ui/src/` contains only `app.rs` plus module directories.
- [x] Move app-shell messages into `app/messages.rs`.
- [x] Move theme selection into `style/theme.rs`.
- [x] Configure `pdf-folio-ui` so the crate root is `src/app.rs`, avoiding a loose `src/lib.rs`.

Remaining:

- [x] Extract library view rendering from `app.rs`.
- [x] Extract library sidebar rendering from `app.rs`.
- [x] Extract library card/list row rendering from `app.rs`.
- [x] Extract library folder tree and folder-card rendering from `app.rs`.
- [x] Extract thumbnail cache/render/load behavior into `library/thumbnails.rs`.
- [x] Extract library async task helpers into `library/tasks.rs`.
- [x] Extract viewer state and helpers into a dedicated viewer module.
- [x] Extract viewer canvas rendering into `viewer/canvas.rs`.
- [x] Extract viewer outline/sidebar rendering into `viewer/outline.rs`.
- [x] Extract viewer async task helpers into `viewer/tasks.rs`.
- [x] Extract app menu and selection menu rendering/routing into `app/menu.rs`.
- [x] Extract keyboard shortcut handling into `app/shortcuts.rs`.
- [x] Extract subscriptions and style watcher streams into `app/subscriptions.rs`.
- [x] Keep `app.rs` as the thin crate root/composition root for `run`, app construction, and top-level dispatch.

## Phase 3: Group App State

Target root shape:

```rust
pub struct PDFolioApp {
    pub mode: AppMode,
    pub viewer: ViewerState,
    pub library: LibraryState,
    pub ui: UiChromeState,
    pub style: StyleState,
    pub settings: Settings,
    pub db: Arc<Db>,
}
```

- [ ] Create `ViewerState` for document, rendered pages, cache, zoom, scroll, TOC, and annotations.
- [ ] Create `LibraryState` for entries, folders, filters, selection, thumbnails, drag, status, and errors.
- [ ] Create `UiChromeState` for app menus, selection menus, confirmation dialog, and modifiers.
- [ ] Create `StyleState` for theme, style book, and style load errors.
- [ ] Migrate state incrementally with tests after each group.

## Phase 4: Split `Message` By Domain

Target shape:

```rust
pub enum Message {
    App(AppMessage),
    Viewer(ViewerMessage),
    Library(LibraryMessage),
    Style(StyleMessage),
    Menu(MenuMessage),
}
```

- [ ] Create domain message enums.
- [ ] Add `From<T> for Message` implementations during migration.
- [ ] Move update handlers by domain.
- [ ] Keep a small top-level `update` dispatcher.
- [ ] Remove obsolete flat message variants after all callers migrate.

## Phase 5: Split `pdf-folio-library/src/db.rs`

Target structure:

```text
crates/pdf-folio-library/src/db/
  mod.rs
  schema.rs
  models.rs
  entries.rs
  folders.rs
  tags.rs
  preferences.rs
  metadata.rs
  rows.rs
```

- [ ] Keep `Db` as the public facade in `db/mod.rs`.
- [ ] Move ID, entry, folder, sort, layout, and preference types into `db/models.rs`.
- [ ] Move schema creation and migrations into `db/schema.rs`.
- [ ] Move entry CRUD, sorting, missing-file, and relink logic into `db/entries.rs`.
- [ ] Move folder CRUD, hierarchy, ordering, and membership logic into `db/folders.rs`.
- [ ] Move tag operations into `db/tags.rs`.
- [ ] Move preference load/save logic into `db/preferences.rs`.
- [ ] Move display metadata, title sort cleanup, author/page attribution into `db/metadata.rs`.
- [ ] Move row conversion helpers into `db/rows.rs`.
- [ ] Preserve existing `pdf-folio-library` public exports in `lib.rs`.

## Phase 6: Consolidate Shared Domain Helpers

- [ ] Centralize import/title cleanup logic shared by importer and UI.
- [ ] Centralize file-size helpers.
- [ ] Centralize thumbnail path/variant behavior.
- [ ] Keep user-facing label composition in UI modules.
- [ ] Keep storage/domain counting and scope logic in pure, tested helpers.

Partially complete:

- [x] Centralized several UI metadata display helpers in `library/metadata.rs`.
- [x] Centralized library filtering/search helper logic in `library/filters.rs`.
- [x] Centralized platform file-manager command generation in `app/platform.rs`.

## Phase 7: Split The Style System

Target structure:

```text
crates/pdf-folio-ui/src/style/
  book/
    mod.rs
    loader.rs
    raw.rs
    parser.rs
    colors.rs
    classes.rs
    fallback.rs
```

- [ ] Split `style/book.rs` into loader, raw model, parser, color parsing, class parsing, and fallback token modules.
- [ ] Consider splitting `style/classes.rs` by iced widget type if it continues growing:
  - `container.rs`
  - `button.rs`
  - `input.rs`
  - `scrollable.rs`
  - `menu.rs`
- [ ] Keep the public style API stable while moving internals.

## Phase 8: Documentation

- [x] Add this restructuring checklist as `structure.md`.
- [ ] Add or update `ARCHITECTURE.md` with:
  - crate dependency rules
  - module ownership
  - where to add new viewer features
  - where to add new library features
  - where async tasks belong
  - testing expectations
- [ ] Update `README.md` architecture section after the major module moves settle.
- [ ] Update `Plan.md` if it remains an active source of project direction.

## Current Status

The first restructuring pass has been completed. `crates/pdf-folio-ui/src/` now contains only `app.rs` plus grouped module directories. Thumbnail cache/render/load behavior has been moved into `library/thumbnails.rs`, keyboard shortcut mapping has been moved into `app/shortcuts.rs`, platform helpers have been moved into `app/platform.rs`, app messages have been moved into `app/messages.rs`, theme selection has been moved into `style/theme.rs`, and subscriptions/style watcher streams have been moved into `app/subscriptions.rs`. The app message flow and feature behavior have not been changed yet.

Latest verified commands:

```sh
cargo fmt
cargo test
```

Both passed after the first restructuring pass.
