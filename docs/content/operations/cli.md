---
title: CLI Reference
eyebrow: Operations
lede: Desktop launch flags and the pdf-folio sync subcommand surface.
order: 31
---

<p class="trail"><strong>Trail</strong> <a href="../crates/main.md">main</a> <span class="sep">·</span> <a href="../crates/cloud.md">cloud</a> <span class="sep">·</span> <a href="../subsystems/sync.md">Sync</a> <span class="sep">·</span> <a href="packaging.md">Packaging</a> <span class="sep">·</span> <a href="../api/pdf-folio-cloud/sync/cli.md">API · cli</a></p>

## Desktop binary

```text
pdf-folio [FILE]
pdf-folio sync <COMMAND>
```

| Invocation | Behavior |
| --- | --- |
| `pdf-folio` | Open [library UI](../crates/ui.md) ([session restore](../architecture/shell.md#session-and-auth) if present) |
| `pdf-folio path/to/doc.pdf` | Open UI and that document ([rendering pipeline](../subsystems/rendering.md)) |
| `pdf-folio sync …` | Run sync CLI (async Tokio runtime) |

Implementation: [`pdf-folio-main`](../crates/main.md) → [`pdf_folio_ui::run`](../api/pdf-folio-ui/index.md) or [`pdf_folio_cloud::sync::cli::run_sync_command`](../api/pdf-folio-cloud/sync/cli.md).

## Sync subcommands

Global options on `pdf-folio sync`:

| Flag / env | Meaning |
| --- | --- |
| `--server` / `PDF_FOLIO_SYNC_SERVER` | Control plane base URL (default `http://mind-palace:53148`) — see [packaging](packaging.md) |
| `--library-id` | Limit to one [library profile](../subsystems/multi-library.md) id |
| `--device-id` | Stable device id for [checkpoints](../subsystems/sync.md) |
| `--db` | Explicit SQLite path (advanced; usually from [libraries.json](data-dirs.md)) |

| Command | Purpose | Deeper docs |
| --- | --- | --- |
| `health` | GET `{server}/health` | [Server](../api/pdf-folio-cloud/server.md) |
| `auth [--client-id]` | Google sign-in; cache session (`PDF_FOLIO_GOOGLE_CLIENT_ID`) | [Sync auth](../api/pdf-folio-cloud/sync/auth.md) |
| `status` | Print cached session info | [session](../api/pdf-folio-cloud/sync/session.md) |
| `ensure-schema` | Apply remote Turso schema from session | [ensure-turso-schema bin](../api/pdf-folio-cloud/bin/ensure-turso-schema.md) |
| `seed` | Seed local sync metadata from library rows | [db sync](../api/pdf-folio-core/db/sync.md) |
| `plan` | Show how much metadata would push | [CRDT](../subsystems/sync.md#crdt-metadata-model) |
| `push` | Push local metadata / CRDT ops | [sync pass](../subsystems/sync.md#a-sync-pass) |
| `pull` | Pull remote metadata | same |
| `upload-blobs` | Upload missing PDFs to R2 | [blobs](../api/pdf-folio-cloud/sync/blobs.md) |
| `download-blobs` | Download blobs into local cache | [data dirs](data-dirs.md) |
| `sync-once` | Manual full sequence: seed → upload → push → pull → download | [run](../api/pdf-folio-cloud/sync/run.md) |

Code: [`crates/pdf-folio-cloud/src/sync/cli.rs`](../api/pdf-folio-cloud/sync/cli.md). Architecture: [Cross-device sync](../subsystems/sync.md).

## Other binaries

```bash
# Control plane
cargo run -p pdf-folio-cloud --bin pdf-folio-sync-server

# One-shot CRDT helper
cargo run -p pdf-folio-cloud --bin crdt-sync-once

# Remote schema helper
cargo run -p pdf-folio-cloud --bin ensure-turso-schema
```

Details: [cloud crate](../crates/cloud.md) · [packaging](packaging.md) · API bins under [pdf-folio-cloud](../api/pdf-folio-cloud/index.md).
