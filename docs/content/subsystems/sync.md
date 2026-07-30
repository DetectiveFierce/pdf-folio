---
title: Cross-Device Sync
eyebrow: Subsystems
lede: One user, several machines — control plane for identity, Turso for CRDT metadata, R2 for PDF blobs.
order: 24
---

<p class="trail"><strong>Trail</strong> <a href="../crates/cloud.md">cloud crate</a> <span class="sep">·</span> <a href="database.md">Database</a> <span class="sep">·</span> <a href="../operations/cli.md">CLI</a> <span class="sep">·</span> <a href="../operations/packaging.md">Packaging</a> <span class="sep">·</span> <a href="../api/pdf-folio-cloud/sync.md">API · sync</a></p>

**Code:**  
- Client: `pdf-folio-cloud/src/sync/`  
- Server: `pdf-folio-cloud/src/server/`  
- Local tables: `pdf-folio-core/src/db/sync.rs`

**API:** [sync](../api/pdf-folio-cloud/sync.md) · [crdt](../api/pdf-folio-cloud/sync/crdt.md) · [server](../api/pdf-folio-cloud/server.md) · [db sync](../api/pdf-folio-core/db/sync.md)

Sync is scoped to **one user's devices** (not multi-user collab — see [glossary](../reference/glossary.md)), not multi-user collaboration. That scope enables a cheap architecture: a self-hosted control plane, serverless SQL for metadata, and object storage for blobs.

## Threat model & trust

| Trusted | Untrusted / limited |
| --- | --- |
| Your devices running the desktop app | Other Google accounts (allow-list) |
| Control plane you host | Long-lived R2/Turso secrets on desktops |
| Short-lived credentials from server | Network observers (use HTTPS in production) |

The server never needs your full PDF library. It only proves who you are and hands out short-lived access to storage you configure.

## Three tiers

| Tier | Component | Job |
| --- | --- | --- |
| Control plane | `pdf-folio-sync-server` (axum) | Prove identity; mint short-lived credentials |
| Metadata | Turso / libSQL | CRDT op log + relational queries |
| Blobs | Cloudflare R2 | Content-addressed PDF bytes |

### Why the server never holds library data

- Single revocable choke point (Google OAuth PKCE + allow-list)
- Session JWT (~30d), Turso token scoped to session, R2 presigned URLs (~15m, single key)
- R2 secret key lives only on the server

Desktop talks **directly** to Turso (Hrana) and R2 after credentials are issued. The server is beside the data path, not inside it.

<div class="diagram">Desktop App                    folio-sync-server                Turso / R2
    │                              │                                   │
    │ ── Google OAuth (PKCE) ────► │                                   │
    │ ◄── session JWT ──────────── │                                   │
    │ ── GET /token/turso ───────► │                                   │
    │ ◄── db url + auth token ──── │                                   │
    │ <span class="hl">── SQL (Hrana) direct ─────────────────────────────────────────►│</span>
    │ ── POST /token/r2/… ───────► │                                   │
    │ ◄── presigned URL ────────── │                                   │
    │ <span class="hl2">── PDF bytes direct ──────────────────────────────────────────►│</span></div>

### Why Turso and R2 both

| | Metadata | Blobs |
| --- | --- | --- |
| Shape | Small structured rows | Large opaque files |
| Access | Frequent small R/W, query by sequence | Whole-object get/put by hash |
| Tool | Turso | R2 (`blobs/<hash>.pdf`) |

Before upload, files are copied into a managed local blob cache and entries relinked so sync always reads a stable content-addressed path.

## CRDT metadata model

Local edits become append-only ops in `sync_crdt_operations`:

```text
op_id, library_id, device_id, logical_time, entity_kind, entity_id, payload, …
```

Entity kinds: `entry`, `folder`, `entry_folder`, plus synthetic `library` for the registry stream (`REGISTRY_LIBRARY_ID = "__pdf_folio_registry__"`).

Conflict rule: last-writer-wins by `(logical_time, device_id, op_id)` — deterministic, no wall-clock trust between devices. Winning ops materialize into `sync_entries` / `sync_folders` / `sync_entry_folders`.

Library **existence** syncs on the registry stream separately from library **contents**, avoiding half-created libraries leaving orphaned remote rows.

### What is not CRDT-merged

| Data | Strategy |
| --- | --- |
| PDF bytes | Content-addressed; identical hash is identical object |
| Search index | Local rebuild from text after hydrate |
| Thumbnails | Local cache; regenerate |
| Window session | Local only |

## A sync pass

1. **Seed** local sync metadata from library rows (`Db::seed_sync_metadata`).
2. **Prepare** local CRDT ops from changed snapshots (`prepare_local_crdt_operations`).
3. **Preflight** remote head sequence vs local cursor; skip if no metadata work and no blobs to upload.
4. **Upload** missing PDF blobs to R2.
5. **Push/pull** CRDT operations; materialize winners.
6. **Hydrate** remote library into local entries/folders when needed (download blobs into cache).

UI auto-sync is driven by timer + remote-available messages; queueing fields on `PDFolioApp` prevent overlapping passes from stomping each other.

```text
tick / manual trigger
  → if sync_in_progress: enqueue library id
  → else start auto_sync_task
  → on complete: start next queued
```

## Module map (client)

| File | Role |
| --- | --- |
| `auth.rs` | Google PKCE sign-in |
| `session.rs` | Persist/load session |
| `client.rs` | High-level client type |
| `remote.rs` | Turso HTTP / Hrana helpers |
| `blobs.rs` | R2 + `BlobCache` |
| `crdt.rs` | Ops, LWW, materialize |
| `run.rs` | Preflight + if-needed pass |
| `status.rs` | Report structs |
| `cli.rs` | `pdf-folio sync …` |

## Module map (server)

| File | Role |
| --- | --- |
| `config.rs` | Env-based server config |
| `auth.rs` | OAuth, JWT session, allow-list |
| `handlers.rs` | Axum routes (health, tokens, …) |
| `storage.rs` | Turso admin / R2 signing helpers |

Deploy: [Packaging](../operations/packaging.md).

## CLI entry points

See [CLI reference](../operations/cli.md) for `health`, `auth`, `status`, `ensure-schema`, `seed`, `plan`, `push`, `pull`, `upload-blobs`, `download-blobs`, `sync-once`.

Useful debug loop on a device:

```bash
cargo run -p pdf-folio-main -- sync health
cargo run -p pdf-folio-main -- sync status
cargo run -p pdf-folio-main -- sync plan
cargo run -p pdf-folio-main -- sync sync-once
```

## Failure modes

| Failure | Expected behavior |
| --- | --- |
| Server down | Auth/token fails; local library still works |
| Turso unreachable | Sync pass errors; retry later |
| R2 upload fail | Op may wait; blob upload table tracks progress |
| Wrong Google account | `WrongAccount` auth state |
| Clock skew | LWW uses logical_time + device_id, not wall clock alone |

## Related

- [Cloud crate](../crates/cloud.md)
- [Database sync tables](database.md)
- [Multi-library registry](multi-library.md)
- [Packaging](../operations/packaging.md)
- [Data directories](../operations/data-dirs.md)
