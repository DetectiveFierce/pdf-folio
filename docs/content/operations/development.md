---
title: Development Workflow
eyebrow: Operations
lede: Build, run, format, lint, and iterate on styles without fighting the workspace.
order: 30
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/workspace.md">Workspace</a> <span class="sep">·</span> <a href="testing.md">Testing</a> <span class="sep">·</span> <a href="cli.md">CLI</a> <span class="sep">·</span> <a href="../reference/glossary.md">Glossary</a> <span class="sep">·</span> <a href="../api/index.md">API</a></p>

## Prerequisites

- Rust stable (edition 2021 workspace) — members listed in [Workspace & crates](../architecture/workspace.md)
- **Pdfium** available to `pdfium-render` (system library or bundled next to the binary — missing Pdfium fails open/render with diagnostics; see [Rendering](../subsystems/rendering.md))
- Linux (Wayland-first; X11 via XWayland)
- For docs: Node + pnpm ([docs site](#docs-site) below)
- For sync server image: Docker (optional) — [Packaging](packaging.md)

### Optional build acceleration

Checked-in `.cargo/config.toml` uses a small `rustc-wrapper` that calls **sccache** when it is on `PATH`, and otherwise runs `rustc` directly — installs stay portable without sccache.

Faster linking on Linux x86_64 (optional; needs `clang` and `lld`):

```bash
CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
RUSTFLAGS="-C link-arg=-fuse-ld=lld" \
cargo build
```

## Common cargo commands

```bash
# Compile everything
cargo check --workspace

# Run the app
cargo run -p pdf-folio-main
cargo run -p pdf-folio-main -- /path/to/file.pdf

# Tests
cargo test --workspace
cargo test -p pdf-folio-core
cargo test -p pdf-folio-ui

# Style / lint
cargo fmt --all
cargo fmt --check
cargo clippy --workspace --all-targets

# Sync server binary
cargo run -p pdf-folio-cloud --bin pdf-folio-sync-server

# Sync CLI (via desktop binary)
cargo run -p pdf-folio-main -- sync health
```

## Logging

Default filter is `info`. Raise detail with:

```bash
RUST_LOG=pdf_folio_ui=debug,pdf_folio_core=debug cargo run -p pdf-folio-main
```

Startup probe (times first update acceptance):

```bash
PDF_FOLIO_STARTUP_PROBE=1 cargo run -p pdf-folio-main
```

## Style iteration

1. Edit KDL under `crates/pdf-folio-style/styles/` — [style system](../subsystems/style-system.md).
2. In a running dev build, styles reload from disk (View → Reload Styles or the bound shortcut).
3. User overrides: `~/.config/pdf-folio/styles/*.kdl` — [data dirs](data-dirs.md).

Rust token/class changes still require recompile ([style crate](../crates/style.md)).

## Where to make changes

| Task | First stop | Docs |
| --- | --- | --- |
| New library bulk op | `ui/library/tasks.rs` + `update.rs` + messages | [Bulk action](../subsystems/bulk-action.md), [messages](../architecture/messages.md) |
| New viewer gesture | `ui/viewer/` + `components/viewer/` | [Rendering](../subsystems/rendering.md), [UI](../crates/ui.md) |
| Schema change | `core/db/schema.rs` + types + callers | [Database](../subsystems/database.md) |
| Sync protocol | `cloud/sync/crdt.rs` + server handlers if auth/storage | [Sync](../subsystems/sync.md), [cloud](../crates/cloud.md) |
| Visual tweak | KDL first; tokens/classes if structure needed | [Style system](../subsystems/style-system.md) |
| iced upgrade | Root patch revs + `iced-widget-patch` together | [iced-widget-patch](../crates/iced-patch.md) |

## Workspace layout reminders

- Do not reintroduce deleted crates (`pdf-folio-db`, `pdf-folio-viewer`, etc.) — [workspace history](../architecture/workspace.md#what-was-absorbed-for-blame-git-archaeology).
- Keep [`pdf-folio-core`](../crates/core.md) free of iced — [dependency rules](../architecture/workspace.md#dependency-rules).
- Prefer small commits that keep `cargo check --workspace` green.

Historical consolidation notes: `scratch/organization.md` (not user-facing docs). Tests: [Testing](testing.md).

## Docs site

```bash
cd docs
pnpm install
pnpm serve    # http://127.0.0.1:4173/
```

`pnpm build` / `pnpm serve` always re-extract **in-code rustdoc** into `content/api/` (gitignored) and render it with the same site theme under **API Reference**.

| Edit this | Updates that |
| --- | --- |
| `//!` at top of a `.rs` file | Module page intro under `api/<crate>/…` |
| `///` on a `pub` / `pub(crate)` item | Item section on that module page |
| `docs/content/**/*.md` (not `api/`) | Narrative guides |

Do not hand-edit `docs/content/api/`. Cross-link freely: guides → `api/…`, API pages → subsystem guides.

## Related

- [API Reference](../api/index.md)
- [Testing](testing.md)
- [CLI](cli.md)
- [Workspace](../architecture/workspace.md)
