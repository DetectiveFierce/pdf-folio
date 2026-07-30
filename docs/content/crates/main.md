---
title: pdf-folio-main
eyebrow: Crates
lede: Thin binary entry — tracing, clap, then UI or sync CLI.
order: 10
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/workspace.md">Workspace</a> <span class="sep">·</span> <a href="../operations/cli.md">CLI</a> <span class="sep">·</span> <a href="ui.md">ui</a> <span class="sep">·</span> <a href="cloud.md">cloud</a> <span class="sep">·</span> <a href="../api/pdf-folio-main/index.md">API</a></p>

**Path:** `crates/pdf-folio-main/`  
**Binary name:** `pdf-folio`  
**API:** [crate root](../api/pdf-folio-main/index.md) · [cli re-export](../api/pdf-folio-main/cli.md)

This crate should stay small. It must not grow domain logic.

## Layout

```text
pdf-folio-main/src/
  main.rs   # Args, tracing, dispatch
  cli.rs    # re-exports pdf_folio_cloud::sync::cli
```

## Responsibilities

1. Initialize `tracing_subscriber` with [`RUST_LOG`](../operations/development.md) / default `info`.
2. Parse CLI:
   - Optional path: open that PDF in the [library UI](ui.md).
   - Subcommand `sync …`: hand off to [`pdf_folio_cloud::sync::cli`](../api/pdf-folio-cloud/sync/cli.md).
3. Call [`pdf_folio_ui::run(file)`](../api/pdf-folio-ui/index.md) for the desktop path.

```text
pdf-folio                 → library UI
pdf-folio book.pdf        → UI, open book.pdf
pdf-folio sync health     → cloud CLI
pdf-folio sync auth       → Google sign-in for sync
pdf-folio sync sync-once  → full manual sync sequence
```

Full flag and subcommand tables: [CLI reference](../operations/cli.md). Sync architecture: [Cross-device sync](../subsystems/sync.md).

## Dependencies

| Crate | Why |
| --- | --- |
| [`pdf-folio-ui`](ui.md) | Desktop app |
| [`pdf-folio-cloud`](cloud.md) | Sync CLI surface only |
| `clap`, `tokio`, `anyhow`, `tracing-subscriber` | Parse, async CLI runtime, errors, logs |

Sync implementation lives under [`pdf-folio-cloud/src/sync/cli.rs`](../api/pdf-folio-cloud/sync/cli.md). Main only re-exports `run_sync_command` and `SyncArgs` via [`cli.rs`](../api/pdf-folio-main/cli.md).

## When to edit this crate

- New top-level flags that affect launch (e.g. open path, logging).
- New **top-level** subcommands that are not sync (rare).

Prefer putting real work in [ui](ui.md) or [cloud](cloud.md). If `main.rs` starts importing database or Pdfium types, the boundary has slipped — see [workspace dependency rules](../architecture/workspace.md#dependency-rules).
