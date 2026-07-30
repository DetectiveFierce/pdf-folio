---
title: pdf-folio-cloud
eyebrow: Crates
lede: Sync client, control-plane server, and Raindrop import — everything that talks to remote services.
order: 14
---

<p class="trail"><strong>Trail</strong> <a href="../subsystems/sync.md">Sync</a> <span class="sep">·</span> <a href="../subsystems/raindrop.md">Raindrop</a> <span class="sep">·</span> <a href="../operations/cli.md">CLI</a> <span class="sep">·</span> <a href="../operations/packaging.md">Packaging</a> <span class="sep">·</span> <a href="../api/pdf-folio-cloud/index.md">API</a></p>

**Path:** `crates/pdf-folio-cloud/`  
**Depends on:** [`pdf-folio-core`](core.md) (and HTTP/crypto stack). **Not** UI.

## Tree

```text
pdf-folio-cloud/src/
  lib.rs
  sync/
    mod.rs
    auth.rs       # Google OAuth (PKCE) for desktop
    session.rs    # cached Session on disk
    client.rs     # SyncClient coordinator
    remote.rs     # Turso / Hrana client
    blobs.rs      # R2 client + local BlobCache
    crdt.rs       # CRDT ops, LWW, materialization (~1.4k lines)
    run.rs        # preflight + sync_library_if_needed
    status.rs     # report types, REGISTRY_LIBRARY_ID
    cli.rs        # pdf-folio sync subcommands
  raindrop/
    mod.rs
    auth.rs       # Raindrop OAuth / token cache
    client.rs     # REST client
    types.rs
    import.rs     # import pipeline (~1k lines)
    matching.rs   # match remote items to local entries
  server/
    mod.rs        # run()
    config.rs     # env configuration
    auth.rs       # JWT / Google verify / allow-list
    handlers.rs   # axum routes
    storage.rs    # Turso token + R2 presign helpers
  bin/
    pdf-folio-sync-server.rs
    crdt-sync-once.rs
    ensure-turso-schema.rs
turso_schema.sql
```

## Three products in one crate

| Surface | Entry | Purpose |
| --- | --- | --- |
| Library API | `pdf_folio_cloud::{sync, raindrop, server}` | Used by UI and CLI |
| Control plane | bin `pdf-folio-sync-server` | Identity + short-lived credentials only |
| Maintenance | `crdt-sync-once`, `ensure-turso-schema` | Ops tooling |

## Sync client flow (summary)

1. `Session` from Google sign-in + control plane.
2. `SyncClient` obtains Turso credentials and R2 presigned URLs via the server.
3. Metadata CRDT ops live in local SQLite (`pdf-folio-core` sync tables) and Turso.
4. PDF bytes go to R2 keys `blobs/<blake3>.pdf` via local `BlobCache`.
5. `sync_library_if_needed` preflights, uploads blobs, syncs CRDT, hydrates.

Full design: [Cross-device sync](../subsystems/sync.md).

## Raindrop

HTTP/OAuth/ZIP import lives here. **DB mapping tables** (`raindrop_collections`, `raindrop_entries`) live in `pdf-folio-core::db::raindrop`. Keep that split: cloud talks to Raindrop.io; core persists provenance.

Deep dive: [Raindrop import](../subsystems/raindrop.md).

## Server routes (conceptual)

The control plane binds (default) `0.0.0.0:53148` and exposes health, OAuth completion, session JWT minting, Turso token issuance, and R2 upload/download presign. It does **not** proxy PDF bytes or SQL queries.

Packaging: [Packaging](../operations/packaging.md).

## API reference

- [pdf-folio-cloud](../api/pdf-folio-cloud/index.md)
- [sync](../api/pdf-folio-cloud/sync.md) · [raindrop](../api/pdf-folio-cloud/raindrop.md) · [server](../api/pdf-folio-cloud/server.md)

## Related

- [CLI](../operations/cli.md)
- [pdf-folio-main](main.md)
- [Core DB sync tables](../subsystems/database.md)
