---
title: Packaging & Deployment
eyebrow: Operations
lede: Docker, compose, and systemd assets for the self-hosted sync control plane.
order: 33
---

<p class="trail"><strong>Trail</strong> <a href="../subsystems/sync.md">Sync</a> <span class="sep">·</span> <a href="../crates/cloud.md">cloud crate</a> <span class="sep">·</span> <a href="cli.md">CLI</a> <span class="sep">·</span> <a href="../api/pdf-folio-cloud/server.md">API · server</a></p>

**Path:** `packaging/`

Desktop packaging (Flatpak/AppStream) may appear in older plans under `scratch/`; the **checked-in** packaging tree today focuses on the sync server. Protocol and trust model: [Cross-device sync](../subsystems/sync.md).

## Artifacts

| File | Role |
| --- | --- |
| `folio-sync-server.Dockerfile` | Image build for [`pdf-folio-sync-server`](../api/pdf-folio-cloud/bin/pdf-folio-sync-server.md) |
| `folio-sync-server.compose.yml` | Compose stack example |
| `folio-sync-server.env.example` | Env template (OAuth, Turso, R2, allow-list) |
| `folio-sync-server.service` | systemd unit sketch |

Build the binary:

```bash
cargo build --release -p pdf-folio-cloud --bin pdf-folio-sync-server
```

Crate layout for server modules: [pdf-folio-cloud](../crates/cloud.md) · API [server](../api/pdf-folio-cloud/server.md) · [handlers](../api/pdf-folio-cloud/server/handlers.md) · [auth](../api/pdf-folio-cloud/server/auth.md) · [config](../api/pdf-folio-cloud/server/config.md) · [storage](../api/pdf-folio-cloud/server/storage.md).

## What the server is responsible for

The control plane **must not** store library PDF bytes. It:

1. Completes Google OAuth (PKCE) and issues session JWTs
2. Enforces an email allow-list
3. Mints short-lived Turso credentials and R2 presigned URLs

Desktops then talk to Turso/R2 **directly**. See the [three-tier diagram](../subsystems/sync.md#three-tiers).

## Client configuration after deploy

Point the desktop / CLI at your server:

```bash
export PDF_FOLIO_SYNC_SERVER=https://your-host.example
cargo run -p pdf-folio-main -- sync health
cargo run -p pdf-folio-main -- sync auth
```

More commands: [CLI reference](cli.md). Session cache path: [Data directories](data-dirs.md).

## Security checklist

- Keep R2 secret keys only on the server ([storage](../api/pdf-folio-cloud/server/storage.md))
- Restrict Google client and allow-list to your accounts
- Prefer HTTPS reverse proxy in front of axum
- Rotate session secrets when re-deploying production

Local development without Docker: [Development](development.md) · `cargo run -p pdf-folio-cloud --bin pdf-folio-sync-server`.
