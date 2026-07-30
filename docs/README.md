# PDF-Folio docs

Static documentation site built from Markdown by a small in-house generator (no heavy framework). Content is a **maintainer guide to the source code**: crate maps, architecture, subsystems, and operations.

## Layout

```
docs/
  content/          Markdown sources (edit these)
    architecture/   Shell, workspace, messages, runtime state
    crates/         Per-crate file maps
    subsystems/     Rendering, DB, sync, Raindrop, …
    operations/     Dev, CLI, paths, packaging, tests
    reference/      Glossary and other non-narrative reference
    api/            GENERATED from rustdoc (gitignored)
  extract-rustdoc.mjs  Walks crates/**/*.rs → content/api/
  nav.json          Sidebar structure (API group merged at build)
  related.json      Curated See also graph (cross-links + ledes)
  theme/            HTML layout + CSS/JS
  build.mjs         Runs extract, then content/ + theme/ → site/
  site/             Generated output (gitignored)
  docs.html         Legacy single-page guide (kept as-is)
```

## Commands

```bash
cd docs
pnpm install
pnpm extract      # crates → content/api/ only
pnpm build        # extract + build site/
pnpm serve        # build, serve on :4173, rebuild on change (incl. .rs)
pnpm watch        # rebuild only
```

## Rustdoc integration

In-code `//!` module docs and `///` item docs are the source of truth for the **API Reference** section. `extract-rustdoc.mjs` turns them into Markdown that uses the same site theme (no stock rustdoc HTML). Guide pages link into `api/…`; API pages link back to guides.

Edit documentation in the `.rs` files, not under `content/api/`.

Open `http://127.0.0.1:4173/` after `pnpm serve`, or open `site/index.html` from any static file server.

## Authoring

1. Add a page under `content/` (or a subdirectory).
2. Frontmatter fields: `title`, `eyebrow`, `lede`, `description`, `order`.
3. Register the page in `nav.json` so it appears in the sidebar.
4. Add curated cross-links in `related.json` (path → list of other `content/` paths).
5. Link to other pages with relative `.md` paths; the builder rewrites them to `.html`.
6. Optional top-of-page trail: `<p class="trail"><strong>Trail</strong> <a href="…">…</a> …</p>`.

```markdown
---
title: My Page
eyebrow: Guide
lede: One-line summary shown under the title.
---

See the [database](../subsystems/database.md#schema-anatomy) section.
```

Heading anchors are slugified from the heading text (e.g. `## Schema anatomy` → `#schema-anatomy`). When adding `children` anchors in `nav.json`, match those slugs.

### See also (automatic)

For **guide** pages (everything outside `content/api/`), the builder:

1. Strips any trailing `## See also` / `## Related` / `## Next` / `## Connections` section from the Markdown body (so you do not maintain two footers).
2. Injects a **See also** block with:
   - **Also in \<section\>** — sibling pages from the same `nav.json` group
   - **Related topics** — entries from `related.json`, with eyebrow + lede snippets

API module pages still get their own See also from `extract-rustdoc.mjs` (parent module, submodules, guide links, glossary).

## Features

- Collapsible sidebar groups
- Client-side full-site search (`/` or `Ctrl/Cmd+K`)
- Internal Markdown links rewritten at build time
- Auto heading anchors + right-rail “On this page” TOC
- Prev/next page navigation from `nav.json` order
- Mobile drawer navigation
- Legacy `docs.html` left untouched (linked from the sidebar footer)

## Design notes

The builder is intentionally small: parse frontmatter, run Markdown through `marked`, apply a single HTML layout, emit a JSON search index, copy assets. No React, no MDX, no plugin ecosystem. Theme CSS reuses the espresso palette from the legacy guide.
