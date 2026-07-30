---
title: pdf-folio-style
eyebrow: Crates
lede: Design system crate — KDL style book, typed tokens, class stylesheets, and reusable iced widgets.
order: 13
---

<p class="trail"><strong>Trail</strong> <a href="../subsystems/style-system.md">Style system</a> <span class="sep">·</span> <a href="../operations/development.md">Development</a> <span class="sep">·</span> <a href="../operations/data-dirs.md">Data dirs</a> <span class="sep">·</span> <a href="../api/pdf-folio-style/index.md">API</a></p>

**Path:** `crates/pdf-folio-style/`  
**Depends on:** `iced`, `kdl` only (see [style system](../subsystems/style-system.md)) (no core/ui/cloud).

Views should describe structure and messages; colors, radii, spacing, and repeated chrome should flow through this crate.

## Tree

```text
pdf-folio-style/src/
  lib.rs           # font bytes, re-exports
  book/            # StyleBook load/parse (KDL)
    parser.rs
    sources.rs     # bundled vs user XDG paths
  tokens.rs        # ThemeTokens, Spacing, FontSize, layout tokens
  classes/         # Class → iced style closures
    core.rs, library.rs, viewer.rs
  components/      # toolbar_button, tag_pill, library_card, …
    core.rs, library.rs, viewer.rs
  borders/         # side_border helpers
  theme.rs         # AppTheme bridge
styles/            # KDL sources (dev hot-reload)
  application.kdl
  themes/{espresso,light}.kdl
  components/…
assets/fonts/      # IBM Plex Sans + Vollkorn (+ optional nerd mono)
```

## Mental model

| Concept | Meaning |
| --- | --- |
| **Theme** | Named palette (`espresso`, `light`) → `ThemeTokens` |
| **Class** | UI role (`LibraryCard`, `Toolbar`, `ViewerFindBar`) |
| **Component state** | `normal`, `hovered`, `pressed`, `selected`, … |
| **Component helper** | Function building an iced widget with styles applied |
| **Layout tokens** | Window size, sidebar widths, card metrics from KDL |

`StyleBook::load()` merges:

1. Bundled KDL (compiled fallback + on-disk `styles/` in dev checkouts)
2. User overrides: `$XDG_CONFIG_HOME/pdf-folio/styles/*.kdl`

Invalid reloads keep the previous book and surface an error.

## What belongs here vs UI

| Belongs in style | Belongs in UI |
| --- | --- |
| Colors, borders, radii, shadows | Message routing |
| Spacing/font size tokens | Selection and drag state |
| Reusable chrome widgets with no DB | Import progress tied to tasks |
| Viewer primitive drawing colors | Pdfium / tile logic |

Helpers must **not** read `PDFolioApp`, `Db`, or document state. They accept labels, tokens, and message callbacks only.

## Hot reload

- Menu: **View → Reload Styles**
- Shortcut: typically Ctrl+Shift+R (see `shell/shortcuts.rs`)
- FS watch on style directories via shell subscriptions

Deep dive: [Style system](../subsystems/style-system.md).

## API reference

- [pdf-folio-style](../api/pdf-folio-style/index.md)
- [book](../api/pdf-folio-style/book.md) · [tokens](../api/pdf-folio-style/tokens.md) · [classes](../api/pdf-folio-style/classes.md) · [components](../api/pdf-folio-style/components.md)

## Related

- [UI crate](ui.md)
- [Development workflow](../operations/development.md)
