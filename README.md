# PDF-Folio

A native **Linux** desktop app for reading PDFs and managing a personal library.

Built in Rust on **[iced](https://github.com/iced-rs/iced) 0.14**, with **Pdfium** for rendering, **SQLite** for library metadata, and **Tantivy** for full-text search. Optional self-hosted sync (control plane + Turso + R2) keeps one user’s libraries and PDF blobs across devices.

> **Platform:** Wayland-first Linux (X11 via XWayland). Not a web app.

## What it does

| Area | Capabilities |
| --- | --- |
| **Library** | Folders, tags, trash, undo snapshots, multi-library vaults, bulk actions, cover thumbnails |
| **Viewer** | Multi-page reading, tile cache, zoom, outline, find-in-document |
| **Search** | Full-text index over library content (Tantivy) |
| **Sync** (optional) | Cross-device metadata (CRDT) + content-addressed PDF blobs for a single user |
| **Import** (optional) | Raindrop.io PDF import with collection mirroring |

**Not in scope:** multi-user collab editing, a full annotation/markup suite, or multi-tenant cloud. The control plane is self-hosted for *your* devices.

## Quick start

**Prerequisites**

- Rust stable (edition 2021)
- **Pdfium** available to `pdfium-render` (system library or bundled next to the binary)
- Linux desktop environment

```bash
# Check the whole workspace
cargo check --workspace

# Run the desktop app
cargo run -p pdf-folio-main

# Open a PDF on launch
cargo run -p pdf-folio-main -- /path/to/file.pdf

# Tests / lint
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
```

**Logging** (default filter is `info`):

```bash
RUST_LOG=pdf_folio_ui=debug,pdf_folio_core=debug cargo run -p pdf-folio-main
```

**Sync CLI** (same binary):

```bash
cargo run -p pdf-folio-main -- sync health
cargo run -p pdf-folio-main -- sync auth
cargo run -p pdf-folio-main -- sync sync-once
```

**Control-plane server** (separate process):

```bash
cargo run -p pdf-folio-cloud --bin pdf-folio-sync-server
```

Deploy assets live under [`packaging/`](packaging/).

## Repository layout

```text
pdf-folio/
├── Cargo.toml              # Workspace members, shared deps, iced [patch]
├── crates/                 # All Rust packages (see below)
├── docs/                   # Maintainer docs site (Markdown → static HTML)
├── packaging/              # Docker / systemd for folio-sync-server
├── tests/fixtures/         # Sample PDFs for tests
└── scratch/                # Historical notes / WIP plans (not product docs)
```

### Crates

| Crate | Path | Role |
| --- | --- | --- |
| **pdf-folio-main** | `crates/pdf-folio-main` | Thin binary (`pdf-folio`): CLI parse, tracing, dispatch to UI or sync |
| **pdf-folio-ui** | `crates/pdf-folio-ui` | iced app — shell, library mode, viewer mode, components |
| **pdf-folio-core** | `crates/pdf-folio-core` | UI-free domain: Pdfium API, SQLite library, Tantivy search, import/watch |
| **pdf-folio-cloud** | `crates/pdf-folio-cloud` | Sync client, control-plane server, Raindrop import |
| **pdf-folio-style** | `crates/pdf-folio-style` | KDL themes/tokens and styled widgets |
| **iced-widget-patch** | `crates/iced-widget-patch` | Local `iced_widget` override (scrollable only) |

### Dependency direction

Dependencies are acyclic on purpose:

```text
pdf-folio-main
    → pdf-folio-ui  → pdf-folio-core
    │               → pdf-folio-cloud → pdf-folio-core
    │               → pdf-folio-style
    → pdf-folio-cloud
```

- **`pdf-folio-core`** has no iced dependency (testable from CLI/tools).
- **`pdf-folio-style`** has no app/domain state (design system only).
- The sync **server** is a binary inside `pdf-folio-cloud`, not the desktop process.

### UI crate map (largest package)

```text
pdf-folio-ui/src/
  shell/        # PDFolioApp, Message, top-level update, session, shortcuts
  library/      # Library mode: update, tasks, views, multi-library registry
  viewer/       # Viewer mode: document state, render tasks, navigation
  components/   # Widgets (shared / library / viewer)
```

Architecture is Elm-style: one `Message` enum, one `update` reducer, side effects as iced `Task`s. Background work never mutates app state directly.

### Style system

Themes and chrome live in KDL under `crates/pdf-folio-style/styles/`. In a running dev build, styles can reload from disk; user overrides go in `~/.config/pdf-folio/styles/*.kdl`.

## Data directories

Paths use `directories::ProjectDirs` for project `dev.pdf-folio.PDF-Folio`. On a typical Linux install:

| Kind | Location |
| --- | --- |
| Data | `~/.local/share/pdf-folio/PDF-Folio/` (`library.db`, search index, sync blobs, …) |
| Config | `~/.config/pdf-folio/` (user styles) |
| Cache | `~/.cache/pdf-folio/PDF-Folio/` (thumbnails) |

## Documentation

This repo ships a **maintainer-oriented** docs site (crate maps, architecture, subsystems, API extracted from rustdoc) — not end-user product docs.

```bash
cd docs
pnpm install
pnpm serve    # http://127.0.0.1:4173/
```

| Start here | Topic |
| --- | --- |
| [`docs/content/architecture/overview.md`](docs/content/architecture/overview.md) | Elm shell, messages, tasks |
| [`docs/content/architecture/workspace.md`](docs/content/architecture/workspace.md) | Full crate map and dependency rules |
| [`docs/content/operations/development.md`](docs/content/operations/development.md) | Day-to-day build / iterate |
| [`docs/content/operations/cli.md`](docs/content/operations/cli.md) | `pdf-folio` and `sync` subcommands |
| [`docs/content/subsystems/`](docs/content/subsystems/) | Rendering, DB, search, sync, style, … |
| [`docs/README.md`](docs/README.md) | How the docs generator works |

Narrative guides live in `docs/content/` (except `api/`). The **API Reference** is generated from `//!` / `///` comments in `crates/**/*.rs` on every docs build — do not hand-edit `docs/content/api/`.

## License

MIT OR Apache-2.0 (see workspace `Cargo.toml`).
