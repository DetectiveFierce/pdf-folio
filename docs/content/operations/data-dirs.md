---
title: Data Directories
eyebrow: Operations
lede: XDG paths used by the desktop app, sync client, search index, and caches.
order: 32
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/state.md">Runtime state</a> <span class="sep">·</span> <a href="../subsystems/multi-library.md">Multi-library</a> <span class="sep">·</span> <a href="../subsystems/sync.md">Sync</a> <span class="sep">·</span> <a href="development.md">Development</a></p>

PDF-Folio resolves paths via `directories::ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")`.

On a typical Linux install:

| Kind | Path pattern |
| --- | --- |
| Data | `~/.local/share/pdf-folio/PDF-Folio/` |
| Config | `~/.config/pdf-folio/` (styles under `styles/`) |
| Cache | `~/.cache/pdf-folio/PDF-Folio/` |

Exact base can follow `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, and `XDG_CACHE_HOME`.

## Data directory contents

| Path (under data dir) | Written by | Purpose |
| --- | --- | --- |
| `library.db` | core [`Db::open_default`](../api/pdf-folio-core/db/schema.md) | Default [library SQLite](../subsystems/database.md) |
| `libraries.json` | UI [registry](../subsystems/multi-library.md) / sync CLI | Multi-library profiles |
| `session.json` | UI [shell session](../architecture/shell.md#session-and-auth) | Window/mode/document restore |
| `search-index/` | core [`SearchIndex`](../api/pdf-folio-core/db/search.md) | Tantivy index (default) — [search](../subsystems/search.md) |
| `sync/session.json` | cloud [sync session](../api/pdf-folio-cloud/sync/session.md) | Google/control-plane session cache |
| `sync/blobs/` | cloud [`BlobCache`](../api/pdf-folio-cloud/sync/blobs.md) | Content-addressed managed PDFs |
| `raindrop/token.json` | [raindrop auth](../subsystems/raindrop.md) | OAuth access token |

Additional per-library DB files live alongside when users create vaults (paths recorded in `libraries.json`). Runtime field map: [Runtime state](../architecture/state.md#persistence-map).

## Cache directory

| Path | Purpose |
| --- | --- |
| `thumbs/<entry_id>.rgba` | Cover thumbnail cache ([`thumbnail_path`](../api/pdf-folio-core/db/import.md)) |

## Config directory

| Path | Purpose |
| --- | --- |
| `styles/*.kdl` | User style overrides layered on [bundled themes](../subsystems/style-system.md) |

## Environment variables (selected)

| Variable | Area |
| --- | --- |
| `RUST_LOG` | Tracing filter — [development](development.md) |
| `PDF_FOLIO_STARTUP_PROBE` | UI startup timing |
| `PDF_FOLIO_SYNC_SERVER` | Sync CLI server URL — [CLI](cli.md) |
| `PDF_FOLIO_GOOGLE_CLIENT_ID` | OAuth client id — [sync](../subsystems/sync.md) |
| `PDF_FOLIO_RAINDROP_TOKEN` | Raindrop API token bypass — [Raindrop](../subsystems/raindrop.md) |
| `PDF_FOLIO_*` (server) | Control plane config — see [packaging](packaging.md) env example |

Glossary of path-related terms: [Glossary](../reference/glossary.md).
