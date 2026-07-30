---
title: Style System
eyebrow: Subsystems
lede: Views describe structure and messages; visual values live in KDL and typed tokens.
order: 23
---

<p class="trail"><strong>Trail</strong> <a href="../crates/style.md">style crate</a> <span class="sep">·</span> <a href="../architecture/state.md">Runtime state</a> <span class="sep">·</span> <a href="../operations/development.md">Development</a> <span class="sep">·</span> <a href="../api/pdf-folio-style/index.md">API</a></p>

**Code:** [`crates/pdf-folio-style/`](../crates/style.md), KDL under `crates/pdf-folio-style/styles/`  
**API:** [style crate](../api/pdf-folio-style/index.md) · [book](../api/pdf-folio-style/book.md) · [tokens](../api/pdf-folio-style/tokens.md) · [classes](../api/pdf-folio-style/classes.md) · [components](../api/pdf-folio-style/components.md)

## Goals

- One place for colors, radii, borders, shadows, spacing, and repeated widget chrome
- Light and dark themes with the **same token names**
- Dev-time hot reload without recompiling Rust for palette tweaks — [development: style iteration](../operations/development.md#style-iteration)
- User overrides layered on top of bundled styles ([config dir](../operations/data-dirs.md))

## Load pipeline

```text
bundled KDL (+ on-disk styles/ in checkout)
        │
        ▼
  StyleBook::load()
        │
        ├── parse themes → ThemeTokens
        ├── parse components → ClassStyle per state
        └── parse application layout/labels
        │
        ▼
  user: $XDG_CONFIG_HOME/pdf-folio/styles/*.kdl
        │
        ▼
  AppearanceRuntime.style_book: Arc<StyleBook>
```

[`StyleBook::load`](../api/pdf-folio-style/book.md) merges bundled + checkout + user sources ([sources](../api/pdf-folio-style/book/sources.md)). On failed reload, the previous book remains active and the error is reported on [`AppearanceRuntime`](../architecture/state.md#appearance-appearanceruntime) in the [shell](../architecture/shell.md).

## KDL shape

```kdl
theme "espresso" {
    color "background" "#1A1208"
    color "surface" "#0F0A04"
    color "accent" "#D4A853"
}

component "LibraryCard" {
    normal  background="#251A0E" text="#DDD0BA" border="#C8B89A1A" radius=8
    hovered background="#2E2010" border="#C8B89A2E"
    selected background="#30230F" border="#D4A853"
}
```

States: `normal`, `hovered`, `pressed`, `focused`, `disabled`, `selected`, `active`, `error`. Optional `theme="espresso"` on a state scopes it. Parser details: [book/parser](../api/pdf-folio-style/book/parser.md).

Colors: `#RRGGBB`, `#RRGGBBAA`, `rgba(…)`, token refs (`$accent`), blends (`mix($surface, $accent, 0.16)`).

## Rust modules

| Module | Role | API |
| --- | --- | --- |
| `book` | Parse/validate/load | [book](../api/pdf-folio-style/book.md) |
| `tokens` | [`ThemeTokens`](../api/pdf-folio-style/tokens.md), spacing, fonts, layout | [tokens](../api/pdf-folio-style/tokens.md) |
| `classes` | [`Class`](../api/pdf-folio-style/classes.md) → iced style closures | [classes](../api/pdf-folio-style/classes.md) |
| `components` | `toolbar_button`, `tag_pill`, `library_card`, … | [components](../api/pdf-folio-style/components.md) |
| `borders` | Side-border drawing helpers | [borders](../api/pdf-folio-style/borders.md) |
| `theme` | [`AppTheme`](../api/pdf-folio-style/theme.md) enum bridging | [theme](../api/pdf-folio-style/theme.md) |

Class names describe **roles** (`ViewerFindBar`, `Sidebar`), not current paint (`BlueButton`). UI widgets in [pdf-folio-ui components](../crates/ui.md#components-ui-building-blocks) should call style helpers rather than hard-coding colors.

## File map under `styles/`

```text
styles/
  application.kdl
  themes/espresso.kdl
  themes/light.kdl
  components/core.kdl
  components/library/library.kdl
  components/library/sidebar.kdl
  components/viewer/viewer.kdl
```

Viewer-specific classes should stay in viewer KDL so global toolbar/sidebar styles are not accidentally coupled. Left-rail scrollbars: [iced-widget-patch](../crates/iced-patch.md).

## Fonts

IBM Plex Sans (UI) and Vollkorn (display) are embedded as bytes and registered with iced at startup (`BUNDLED_FONT_BYTES` in the [style crate](../crates/style.md)). Prefer `ui_font` / `display_font` helpers over ad-hoc font picks.
