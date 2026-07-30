---
title: Raindrop Import
eyebrow: Subsystems
lede: Import PDFs from Raindrop.io with collection mirroring, progress, and rollback-friendly provenance.
order: 25
---

<p class="trail"><strong>Trail</strong> <a href="../crates/cloud.md">cloud</a> <span class="sep">·</span> <a href="database.md">Database</a> <span class="sep">·</span> <a href="bulk-action.md">Bulk action</a> <span class="sep">·</span> <a href="../api/pdf-folio-cloud/raindrop.md">API · raindrop</a></p>

**Code:**  
- HTTP / OAuth / import pipeline: [`pdf-folio-cloud/src/raindrop/`](../api/pdf-folio-cloud/raindrop.md)  
- Mapping tables: [`pdf-folio-core/src/db/raindrop.rs`](../api/pdf-folio-core/db/raindrop.md)  
- UI tasks/dialogs: [`pdf-folio-ui` library tasks](../api/pdf-folio-ui/library/tasks.md) + [dialogs](../api/pdf-folio-ui/components/library/dialogs.md)

## Split of concerns

| Layer | Owns | API |
| --- | --- | --- |
| Cloud `raindrop::client` | REST pagination, downloads, ZIP bulk path | [client](../api/pdf-folio-cloud/raindrop/client.md) |
| Cloud `raindrop::auth` | OAuth and cached access token | [auth](../api/pdf-folio-cloud/raindrop/auth.md) |
| Cloud `raindrop::import` | Preview, select, progress phases, write into [`Db`](../api/pdf-folio-core/db.md) | [import](../api/pdf-folio-cloud/raindrop/import.md) |
| Cloud `raindrop::matching` | Match remote items to existing local entries | [matching](../api/pdf-folio-cloud/raindrop/matching.md) |
| Core `db::raindrop` | `import_sources`, `raindrop_collections`, `raindrop_entries` | [db raindrop](../api/pdf-folio-core/db/raindrop.md) |
| UI | Dialogs, destination folder choices, cancel/rollback prompts | [UI library](../crates/ui.md) |

Do not put `reqwest` or ZIP handling in [core](../crates/core.md). Do not put SQLite schema knowledge deep in UI. Identity of imported PDFs still follows [BLAKE3 content hash](database.md#pdf-identity-is-its-bytes).

## Auth modes

Import can proceed when any of:

- `PDF_FOLIO_RAINDROP_TOKEN` is set ([env reference](../operations/data-dirs.md#environment-variables-selected))
- A cached OAuth token exists under the XDG data dir (`…/raindrop/token.json` — [data dirs](../operations/data-dirs.md))
- Bundled/env OAuth client config is available

`can_import_without_prompt()` encodes that check for the UI.

## Import flow (high level)

1. **Preview** — fetch collections and PDF candidates (`import_preview*` APIs on [import](../api/pdf-folio-cloud/raindrop/import.md)).
2. **User selects** destination (`RaindropImportDestination`) and which items to pull.
3. **Download** — individual files or ZIP bulk when count exceeds `ZIP_IMPORT_THRESHOLD` (12).
4. **Hash + import** — reuse core [`import_pdf`](../api/pdf-folio-core/db/import.md) / folder creation; write mapping rows.
5. **Progress** — phase enum (`RaindropImportPhase`) and basis-point progress for ZIP stages; UI messages are ordinary [library messages](../architecture/messages.md).
6. **Rollback** — pending rollback metadata can undo a partial import if the user cancels (snapshot-like, related to [org undo](database.md#undo-as-snapshots)).

Provenance tables make rollback and re-import matching possible without guessing from titles alone. End-to-end orchestration patterns: [Life of a bulk action](bulk-action.md).

## Constants maintainers may tune

In [`raindrop/mod.rs`](../api/pdf-folio-cloud/raindrop.md):

- `API_BASE` — Raindrop REST root
- `MAX_PER_PAGE` — pagination size (50)
- `ZIP_IMPORT_THRESHOLD` — when to switch to ZIP bulk download
- ZIP progress basis-point splits for preparing / downloaded / extracted phases

## UI integration

Library tasks expose:

- `raindrop_import_task` / destination helpers — [tasks API](../api/pdf-folio-ui/library/tasks.md)
- `pending_raindrop_rollback_check_task`
- `rollback_pending_raindrop_import_task`
- Thumbnail helpers for Raindrop previews where applicable

Messages carry `RaindropImportPreview`, `RaindropImportProgress`, and completion/error variants ([message surface](../architecture/messages.md)).
