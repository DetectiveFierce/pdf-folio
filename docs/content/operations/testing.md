---
title: Testing
eyebrow: Operations
lede: Where tests live, how to run them, and what each crate's suite is good for.
order: 34
---

<p class="trail"><strong>Trail</strong> <a href="development.md">Development</a> <span class="sep">·</span> <a href="../crates/core.md">core</a> <span class="sep">·</span> <a href="../crates/ui.md">ui</a> <span class="sep">·</span> <a href="../architecture/workspace.md">Workspace</a></p>

## Run

```bash
cargo test --workspace
cargo test -p pdf-folio-core
cargo test -p pdf-folio-ui
cargo test -p pdf-folio-style
cargo test -p pdf-folio-cloud
```

Add `-- --nocapture` for logging. Filter with the usual cargo test name substring. Day-to-day compile loop: [Development](development.md).

## Map of test modules

| Location | Focus | Related docs |
| --- | --- | --- |
| `pdf-folio-core/src/pdf/tests.rs` | [`PdfDoc`](../api/pdf-folio-core/pdf/document.md), render, text, outline (fixtures) | [Rendering](../subsystems/rendering.md) |
| `pdf-folio-core/src/db/tests.rs` | Schema, entries, folders, trash, snapshots | [Database](../subsystems/database.md) |
| `pdf-folio-core/src/db/import/tests.rs` | Import / hash behavior | [Search & watching](../subsystems/search.md) |
| `pdf-folio-core/src/db/search/tests.rs` | Tantivy index | [Search](../subsystems/search.md) |
| `pdf-folio-ui/src/tests.rs` | Selection, filters, shell helpers, large suite | [UI crate](../crates/ui.md), [bulk action](../subsystems/bulk-action.md) |
| `pdf-folio-style/src/book/tests.rs` | Style parsing | [Style system](../subsystems/style-system.md) |
| `pdf-folio-style/src/classes/tests.rs` | Class resolution | [Style crate](../crates/style.md) |
| `pdf-folio-style` font test in `lib.rs` | Embedded font families | [Style system § Fonts](../subsystems/style-system.md#fonts) |
| `pdf-folio-cloud/src/raindrop/tests.rs` | Raindrop client/import helpers | [Raindrop](../subsystems/raindrop.md) |

Fixtures: `tests/fixtures/*.pdf` at the workspace root (used by [core PDF tests](../crates/core.md#tests)).

## Conventions

- DB tests use temp files (`std::env::temp_dir` + unique names) — do not write into the developer's real [XDG library](data-dirs.md).
- Prefer unit tests next to the module they exercise (`mod tests` or `#[path]` submodules).
- UI tests often construct partial state rather than driving a full iced event loop — see [architecture overview](../architecture/overview.md) for the message/task model under test.
- Sync/server tests may be sparse; exercise critical paths with [CLI](cli.md) against a dev server when changing [CRDT](../subsystems/sync.md#crdt-metadata-model) code. Deploy notes: [Packaging](packaging.md).

## Gates used in consolidation

The crate consolidation checklist treated these as the bar:

1. `cargo fmt --check`
2. `cargo check --workspace`
3. `cargo clippy --workspace --all-targets`
4. `cargo test --workspace`

Crate boundaries under test: [Workspace & crates](../architecture/workspace.md).
