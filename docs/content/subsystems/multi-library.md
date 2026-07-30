---
title: Multi-Library Registry
eyebrow: Subsystems
lede: Discrete libraries (vaults) — each with its own SQLite file — coordinated by libraries.json and an optional CRDT registry stream.
order: 26
---

<p class="trail"><strong>Trail</strong> <a href="sync.md">Sync</a> <span class="sep">·</span> <a href="database.md">Database</a> <span class="sep">·</span> <a href="../architecture/state.md">Runtime state</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/library/registry.md">API · registry</a></p>

**Code:** [`pdf-folio-ui/src/library/registry/`](../api/pdf-folio-ui/library/registry.md), sync registry pieces in [pdf-folio-cloud](../crates/cloud.md)

## Model

A **library profile** is roughly:

- Stable `id`
- Display `name`
- Filesystem `db_path` to that library's SQLite file ([`Db`](../api/pdf-folio-core/db.md))

Profiles live in:

```text
$XDG_DATA_HOME/pdf-folio/PDF-Folio/libraries.json
```

(`ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")` — [data directories](../operations/data-dirs.md).)

The default library uses the classic `library.db` path; additional libraries get separate DB files under the data directory. Schema per file: [Library database](database.md).

## Runtime

[`LibraryRegistryRuntime`](../architecture/state.md) on [`PDFolioApp`](../api/pdf-folio-ui/shell/app.md) tracks:

- Profile list and `active_library_id`
- Switcher UI state (menus, rename dialogs)
- Preview thumbnails for the switcher

Switching (`select_library`):

1. Open new [`Db`](../api/pdf-folio-core/db/schema.md) at profile path.
2. Persist registry ([session helpers](../api/pdf-folio-ui/library/registry/session.md)).
3. Replace `app.db`.
4. Reset viewer + library transient state from preferences in the new DB ([runtime state](../architecture/state.md)).
5. Refresh folders/entries, attribute metadata, save [app session](../architecture/shell.md#session-and-auth).

## UI surfaces

- [`AppMode::LibrarySwitcher`](../architecture/overview.md#modes) — vault picker
- Library menus for create / rename / delete (delete blocked when only one profile remains)
- Messages routed via [library update](../api/pdf-folio-ui/library/update.md) / shell — [message surface](../architecture/messages.md)

## Sync interaction

Library **existence and names** sync on a synthetic CRDT stream:

```text
REGISTRY_LIBRARY_ID = "__pdf_folio_registry__"
entity_kind = "library"
```

Content sync for a library only makes sense once the registry agrees the library exists on both devices. Shell tasks include `sync_library_registry_*` and can fan out per-library sync after registry catch-up. Full model: [Cross-device sync](sync.md) · [CRDT](sync.md#crdt-metadata-model).

CLI sync accepts `--library-id` or iterates all profiles from the same `libraries.json` — [CLI reference](../operations/cli.md).

## Module map

| File | Role | API |
| --- | --- | --- |
| `registry/state.rs` | Types: profiles, previews, dialogs | [state](../api/pdf-folio-ui/library/registry/state.md) |
| `registry/session.rs` | Load/save registry, create/rename/delete files | [session](../api/pdf-folio-ui/library/registry/session.md) |
| `registry/preview.rs` | Switcher cover previews | [preview](../api/pdf-folio-ui/library/registry/preview.md) |
| `registry/tasks.rs` | Async registry operations | [tasks](../api/pdf-folio-ui/library/registry/tasks.md) |
| `registry/mod.rs` | Methods on `PDFolioApp` | [registry](../api/pdf-folio-ui/library/registry.md) |
