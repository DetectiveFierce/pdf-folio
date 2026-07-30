---
title: Workspace & Crates
eyebrow: Architecture
lede: Six workspace members after crate consolidation — what each owns, and what must never depend on what.
order: 2
---

<p class="trail"><strong>Trail</strong> <a href="overview.md">Overview</a> <span class="sep">·</span> <a href="../crates/core.md">core</a> <span class="sep">·</span> <a href="../crates/ui.md">ui</a> <span class="sep">·</span> <a href="../crates/cloud.md">cloud</a> <span class="sep">·</span> <a href="../api/index.md">API</a></p>

The workspace was consolidated from twelve crates into **six**. Historical layout notes live under `scratch/organization.md`; this page describes the **current** tree only.

## Members

| Crate | Path | Kind | One-line role |
| --- | --- | --- | --- |
| `iced-widget-patch` | `crates/iced-widget-patch` | lib (patches `iced_widget`) | Local scrollable override |
| `pdf-folio-core` | `crates/pdf-folio-core` | lib | PDF + DB + search + import |
| `pdf-folio-cloud` | `crates/pdf-folio-cloud` | lib + bins | Sync, Raindrop, control plane |
| `pdf-folio-style` | `crates/pdf-folio-style` | lib | KDL themes, tokens, widgets |
| `pdf-folio-ui` | `crates/pdf-folio-ui` | lib | iced application |
| `pdf-folio-main` | `crates/pdf-folio-main` | bin `pdf-folio` | Process entry |

Root `Cargo.toml` also pins iced supporting crates to a fixed git revision and patches `iced_widget` to the local crate.

## Dependency rules

These rules are load-bearing. Breaking them reintroduces the old tangle that consolidation removed.

| Rule | Rationale |
| --- | --- |
| `pdf-folio-core` has **zero** iced / wgpu / winit | Core must stay testable and usable from CLI/server tools |
| `pdf-folio-cloud` depends on `pdf-folio-core`, not UI | Sync CLI and server share the same DB primitives |
| `pdf-folio-style` may use iced + kdl only | Design system stays free of app/domain state |
| `pdf-folio-ui` may depend on core, cloud, style | Orchestration layer |
| `pdf-folio-main` stays thin | Binary = CLI parse + `ui::run` or `sync::cli` |

```text
                    ┌──────────────────┐
                    │  iced-widget-patch│  (workspace [patch])
                    └────────▲─────────┘
                             │
┌──────────────┐    ┌────────┴─────────┐    ┌────────────────┐
│ pdf-folio-   │    │  pdf-folio-ui    │───▶│ pdf-folio-style│
│ main         │───▶│                  │    └────────────────┘
└──────┬───────┘    └────────┬─────────┘
       │                     │
       │                     ├──────────────▶ pdf-folio-core
       │                     │
       └─────────────────────┴──────────────▶ pdf-folio-cloud ──▶ pdf-folio-core
```

## What was absorbed (for blame / git archaeology)

| Current crate | Former crates / areas |
| --- | --- |
| `pdf-folio-core` | `pdf-folio-core` + `pdf-folio-db` + Raindrop *mapping* tables |
| `pdf-folio-cloud` | `pdf-folio-sync` + `pdf-folio-sync-server` + Raindrop HTTP/import |
| `pdf-folio-ui` | `pdf-folio-ui` + `pdf-folio-ui-components` + `pdf-folio-viewer` |
| `pdf-folio-style` | same crate, re-split into `book` / `classes` / `components` / `borders` |

Packaging still builds the control-plane binary as:

```bash
cargo build --release -p pdf-folio-cloud --bin pdf-folio-sync-server
```

The binary name `pdf-folio-sync-server` is intentional; there is no separate crate by that name anymore.

## Shared workspace dependencies

Notable centralized deps in root `Cargo.toml`:

| Dependency | Used for |
| --- | --- |
| `iced` 0.14 | UI |
| `pdfium-render` | PDF open/render/text/outline |
| `rusqlite` (bundled) | Library DB |
| `tantivy` | Full-text search |
| `notify` | FS watch for imports and styles |
| `axum` / `jsonwebtoken` / `reqwest` | Sync server and clients |
| `kdl` | Style book |
| `blake3` | Content-addressed entry IDs and blob keys |

## Where to put new code

| If you are adding… | Put it in… |
| --- | --- |
| Pdfium helpers, tile cache, geometry | `pdf-folio-core/src/pdf/` |
| Schema, queries, import, search | `pdf-folio-core/src/db/` |
| iced views, messages, drag/selection | `pdf-folio-ui` (appropriate subtree) |
| Colors, radii, reusable chrome widgets | `pdf-folio-style` (+ KDL under `styles/`) |
| OAuth, Turso/R2, CRDT, Raindrop HTTP | `pdf-folio-cloud` |
| CLI flags for the desktop binary | `pdf-folio-main` (thin) + cloud `sync/cli` for sync |

## Per-crate guides

- [pdf-folio-main](../crates/main.md) · [API](../api/pdf-folio-main/index.md)
- [pdf-folio-core](../crates/core.md) · [API](../api/pdf-folio-core/index.md)
- [pdf-folio-ui](../crates/ui.md) · [API](../api/pdf-folio-ui/index.md)
- [pdf-folio-style](../crates/style.md) · [API](../api/pdf-folio-style/index.md)
- [pdf-folio-cloud](../crates/cloud.md) · [API](../api/pdf-folio-cloud/index.md)
- [iced-widget-patch](../crates/iced-patch.md) · [API](../api/iced-widget-patch/index.md)

The API pages are **rustdoc rendered in this site’s theme** (not the default rustdoc HTML skin). Edit `//!` / `///` in the crates; rebuild docs to refresh.
