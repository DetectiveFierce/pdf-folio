---
title: iced-widget-patch
eyebrow: Crates
lede: Minimal workspace patch that overrides iced's scrollable for sidebar scrollbar placement.
order: 15
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/workspace.md">Workspace</a> <span class="sep">·</span> <a href="ui.md">UI crate</a> <span class="sep">·</span> <a href="../operations/development.md">Development</a> <span class="sep">·</span> <a href="../api/iced-widget-patch/index.md">API</a></p>

**Path:** `crates/iced-widget-patch/`  
**Package name:** `iced_widget` (intentionally shadows crates.io / git iced_widget)  
**API:** [crate](../api/iced-widget-patch/index.md) · [scrollable](../api/iced-widget-patch/scrollable.md)

## Why it exists

PDF-Folio needs a **left scrollbar** on sidebars without reversing vertical scroll semantics. Upstream `iced_widget::scrollable` does not provide that combination for the app's layout, so the workspace vendors a narrow fork. Used heavily by [library](ui.md#library-library-mode-domain) and [viewer](ui.md#viewer-viewer-mode-domain) sidebars and the [style system](../subsystems/style-system.md) chrome.

## How patching works

Root `Cargo.toml`:

```toml
[patch.crates-io]
iced_widget = { path = "crates/iced-widget-patch" }
# plus matching git revs for iced_core, iced_runtime, …
```

All iced supporting crates are pinned to the **same git revision** as the upstream widget dependency inside the patch crate. That keeps a single coherent iced type universe with [pdf-folio-ui](ui.md) and [pdf-folio-style](style.md).

The patch crate:

1. Depends on `iced_widget` from git as package `iced_widget_upstream`.
2. `pub use iced_widget_upstream::*;`
3. Replaces only [`scrollable`](../api/iced-widget-patch/scrollable.md) with the local module (`src/scrollable.rs`, large).

Any `iced::widget::scrollable` usage in the app transparently gets the patched type. See [workspace map](../architecture/workspace.md) for the full iced patch set.

## Rules for maintainers

| Do | Don't |
| --- | --- |
| Keep the override to scrollable only | Fork more widgets “while you're here” |
| Bump **all** iced git revs together | Mix revs between patch and workspace |
| Document behavior differences in PRs | Rely on unreleased iced APIs without pinning |

When upgrading iced:

1. Choose a new upstream commit.
2. Update every `rev = "…"` in root `Cargo.toml` and the patch crate's `Cargo.toml`.
3. Rebase `scrollable.rs` against upstream scrollable changes.
4. `cargo check --workspace` and smoke-test library + viewer sidebars ([development workflow](../operations/development.md)).
