---
title: Documentation
eyebrow: PDF-Folio
lede: Maintainer guide to the Rust source — how the workspace is shaped, where code lives, and how the major subsystems fit together.
order: 0
---

PDF-Folio is a native Linux desktop app for reading PDFs and managing a personal library. It is a Rust workspace on **iced 0.14**, with **Pdfium** for rendering, **SQLite** for library metadata, **Tantivy** for full-text search, and an optional self-hosted **sync** path (control plane + Turso + R2).

This site is a **source-code guide for maintainers**. It describes crate boundaries, module maps, message flow, persistence layout, and the design decisions that make the larger files navigable. End-user product docs are not the goal; if you are changing code, this is the map.

<div class="card-grid">
  <a class="card-link" href="architecture/overview.md">
    <div class="card-title">Architecture</div>
    <p class="card-desc">Elm-style shell, one Message enum, Task-backed side effects, and the PDFolioApp runtime tree.</p>
    <div class="card-meta">Start here</div>
  </a>
  <a class="card-link" href="architecture/workspace.md">
    <div class="card-title">Workspace &amp; Crates</div>
    <p class="card-desc">Source map of the six crates — module trees, dependency rules, and how they compose at runtime.</p>
    <div class="card-meta">Map</div>
  </a>
  <a class="card-link" href="architecture/messages.md">
    <div class="card-title">Message Surface</div>
    <p class="card-desc">How UI events are clustered, routed through library/viewer/shell, and extended safely.</p>
    <div class="card-meta">Events</div>
  </a>
  <a class="card-link" href="crates/ui.md">
    <div class="card-title">UI Crate</div>
    <p class="card-desc">shell / library / viewer / components — the largest crate and how updates are routed.</p>
    <div class="card-meta">pdf-folio-ui</div>
  </a>
  <a class="card-link" href="subsystems/database.md">
    <div class="card-title">Library Database</div>
    <p class="card-desc">Content-addressed entries, folders and tags, snapshot undo, and sync metadata tables.</p>
    <div class="card-meta">pdf-folio-core</div>
  </a>
  <a class="card-link" href="subsystems/sync.md">
    <div class="card-title">Cross-Device Sync</div>
    <p class="card-desc">Control plane, Turso CRDT metadata, R2 blobs, and the desktop sync pass.</p>
    <div class="card-meta">Cloud</div>
  </a>
  <a class="card-link" href="operations/development.md">
    <div class="card-title">Development</div>
    <p class="card-desc">Build, run, log, iterate on styles, and rebuild this documentation site.</p>
    <div class="card-meta">Ops</div>
  </a>
  <a class="card-link" href="reference/glossary.md">
    <div class="card-title">Glossary</div>
    <p class="card-desc">Shared vocabulary: EntryId, tile, vault, CRDT, StyleBook, and the rest of the map.</p>
    <div class="card-meta">Reference</div>
  </a>
</div>

## How to use this site

| Section | Audience need |
| --- | --- |
| [Architecture](architecture/overview.md) | How the app boots, updates, and renders |
| [Workspace map](architecture/workspace.md) | Which crate owns what; dependency rules |
| [Message surface](architecture/messages.md) | How to add UI events without breaking routing |
| [Per-crate guides](crates/core.md) | File-level maps inside each package |
| [Subsystems](subsystems/rendering.md) | Deep dives: rendering, DB, search, style, sync, Raindrop |
| [Operations](operations/development.md) | Build, test, CLI, XDG paths, packaging |
| [Glossary](reference/glossary.md) | Shared vocabulary (EntryId, CRDT, tile, vault, …) |
| [API Reference](api/index.md) | In-code rustdoc extracted into this theme |

Press **`/`** or **Ctrl/Cmd+K** to search the whole site. The **API Reference** sidebar group is generated from `//!` / `///` comments in the Rust sources on every `pnpm build`.

### Suggested reading paths

| You are… | Read in this order |
| --- | --- |
| New to the repo | Overview → Workspace → UI crate → Runtime state → Development |
| Touching library CRUD | Database → Bulk action → Library tasks API → Multi-library |
| Touching the viewer | Rendering → Viewer crate map → canvas / tasks API |
| Touching sync | Sync subsystem → Cloud crate → CLI → Packaging |
| Changing visuals | Style system → Style crate → KDL under `styles/` |
| Writing docs | Development (Docs site) → edit `//!`/`///` or `content/` → rebuild |

## What PDF-Folio is (and is not)

**Is:**

- A personal PDF library manager with folders, tags, trash, undo, and multi-library vaults
- A multi-page PDF reader with tile cache, zoom settle, outline, find-in-document, and text annotations
- Optional cross-device sync for one user (not multi-user collab editing)
- Optional Raindrop.io PDF import with collection mirroring

**Is not:**

- A full markup suite (freehand ink, sticky notes, or PDF-export of annotations)
- A multi-tenant cloud product — the control plane is self-hosted for *your* devices
- A web app; the desktop shell is iced on Linux (Wayland-first)

## Workspace at a glance

```text
pdf-folio/
├── crates/
│   ├── iced-widget-patch/   # local iced_widget override (scrollable only)
│   ├── pdf-folio-core/      # PDF + SQLite + Tantivy + import/watch
│   ├── pdf-folio-cloud/     # sync client, control-plane server, Raindrop
│   ├── pdf-folio-style/     # KDL themes, tokens, styled widgets
│   ├── pdf-folio-ui/        # iced app: shell, library, viewer, components
│   └── pdf-folio-main/      # binary: pdf-folio (+ sync subcommand)
├── packaging/               # Docker/systemd for folio-sync-server
├── tests/fixtures/          # sample PDFs
└── docs/                    # this site (Markdown → static HTML)
```

Dependency direction is acyclic and intentional:

```text
pdf-folio-main
    → pdf-folio-ui → pdf-folio-core
    │              → pdf-folio-cloud → pdf-folio-core
    │              → pdf-folio-style
    → pdf-folio-cloud
```

`pdf-folio-core` has **no** iced dependency. UI never reaches into cloud server binaries; the sync server is a separate process from the desktop app.

## Building this site

From `docs/`:

```bash
pnpm install
pnpm build         # extract rustdoc + content/ + theme/ → site/
pnpm serve         # build + local server + watch on :4173
```

| Source of truth | Edits go in… |
| --- | --- |
| Narrative guides | `docs/content/**/*.md` (except `api/`) + `nav.json` |
| API Reference pages | `//!` / `///` in `crates/**/*.rs` (then rebuild) |

Do **not** hand-edit `docs/content/api/` — it is regenerated. Open `http://127.0.0.1:4173/` after `pnpm serve`.

## Jump by task

| If you need to… | Go to |
| --- | --- |
| Understand the update loop | [Architecture overview](architecture/overview.md) → [Messages](architecture/messages.md) |
| Find which crate owns a concern | [Workspace map](architecture/workspace.md) → per-crate guides |
| Change library CRUD / folders | [Database](subsystems/database.md) → [Bulk action](subsystems/bulk-action.md) → [UI library](crates/ui.md) |
| Change PDF rendering / zoom | [Rendering](subsystems/rendering.md) → [core pdf API](api/pdf-folio-core/pdf.md) |
| Change sync or deploy the server | [Sync](subsystems/sync.md) → [CLI](operations/cli.md) → [Packaging](operations/packaging.md) |
| Change colors / chrome | [Style system](subsystems/style-system.md) → [style crate](crates/style.md) |
| Look up a term | [Glossary](reference/glossary.md) |
| Find a function or type | [API Reference](api/index.md) (search with `/`) |

## Repo paths (outside this site)

| Path | Notes |
| --- | --- |
| `docs/README.md` | Docs generator authoring notes (`related.json`, extract, theme) |
| `docs/related.json` | Curated See also graph used at build time |
| `scratch/organization.md` | Historical crate-consolidation notes (not user-facing) |
| `packaging/` | Sync server deploy assets |
| Root `Cargo.toml` | Workspace members, iced patch revs, shared deps |
