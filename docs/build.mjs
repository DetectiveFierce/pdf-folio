#!/usr/bin/env node
/**
 * PDF-Folio docs builder — minimal static site generator.
 *
 * Inputs:
 *   content/          — pages (YAML frontmatter + Markdown)
 *   nav.json          — sidebar structure
 *   theme/            — layout.html + assets/
 *
 * Output:
 *   site/             — generated HTML, assets, search-index.json
 *
 * Usage:
 *   node build.mjs            # one-shot build
 *   node build.mjs --watch    # rebuild on change
 *   node build.mjs --serve    # build, serve site/, rebuild on change
 */

import { marked } from "marked";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  readdirSync,
  statSync,
  cpSync,
  existsSync,
  watch,
} from "node:fs";
import { join, relative, dirname, extname, basename, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:http";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = __dirname;
const CONTENT = join(ROOT, "content");
const THEME = join(ROOT, "theme");
const OUT = join(ROOT, "site");
const NAV_PATH = join(ROOT, "nav.json");
const RELATED_PATH = join(ROOT, "related.json");
const EXTRACT_SCRIPT = join(ROOT, "extract-rustdoc.mjs");
const API_NAV_FRAGMENT = join(CONTENT, "api", "_nav_fragment.json");

// ─── helpers ────────────────────────────────────────────────────────────────

function walk(dir, acc = []) {
  if (!existsSync(dir)) return acc;
  for (const name of readdirSync(dir)) {
    if (name.startsWith(".")) continue;
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, acc);
    else acc.push(p);
  }
  return acc;
}

function ensureDir(p) {
  mkdirSync(p, { recursive: true });
}

function slugify(text) {
  return String(text)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/&/g, "and")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");
}

/** Minimal frontmatter parser: YAML-ish key: value between --- fences. */
function parseFrontmatter(raw) {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!m) return { data: {}, body: raw };
  const data = {};
  for (const line of m[1].split(/\r?\n/)) {
    const kv = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!kv) continue;
    let v = kv[2].trim();
    if (
      (v.startsWith('"') && v.endsWith('"')) ||
      (v.startsWith("'") && v.endsWith("'"))
    ) {
      v = v.slice(1, -1);
    }
    data[kv[1]] = v;
  }
  return { data, body: m[2] };
}

function mdPathToOut(mdRel) {
  // content/index.md -> index.html
  // content/guide/foo.md -> guide/foo.html
  const noExt = mdRel.replace(/\.md$/i, "");
  return noExt + ".html";
}

function mdPathToUrl(mdRel) {
  return mdPathToOut(mdRel).split(sep).join("/");
}

function rootPrefix(outRel) {
  const depth = outRel.split(/[/\\]/).length - 1;
  return depth === 0 ? "./" : "../".repeat(depth);
}

function hrefBetween(fromOutRel, toOutRel, anchor) {
  // Simple relative path from fromOutRel's directory to toOutRel
  const fromDir = dirname(fromOutRel) === "." ? "" : dirname(fromOutRel);
  const toParts = toOutRel.split(/[/\\]/);
  const fromParts = fromDir ? fromDir.split(/[/\\]/) : [];

  let i = 0;
  while (
    i < fromParts.length &&
    i < toParts.length - 1 &&
    fromParts[i] === toParts[i]
  ) {
    i++;
  }
  const ups = fromParts.length - i;
  const down = toParts.slice(i);
  let href = (ups ? "../".repeat(ups) : "") + down.join("/");
  if (!href) href = basename(toOutRel);
  if (anchor) href += "#" + anchor;
  return href;
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function stripHtml(html) {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/\s+/g, " ")
    .trim();
}

// ─── markdown rendering with heading ids ────────────────────────────────────

function createRenderer() {
  const headings = [];
  const used = new Map();

  const renderer = new marked.Renderer();
  const baseHeading = renderer.heading.bind(renderer);

  renderer.heading = function ({ tokens, depth, text }) {
    // `text` may include inline HTML; strip tags for slug/id
    const plain = stripHtml(this.parser.parseInline(tokens));
    let id = slugify(plain);
    if (!id) id = "section";
    const n = used.get(id) || 0;
    used.set(id, n + 1);
    if (n > 0) id = `${id}-${n + 1}`;

    if (depth >= 2 && depth <= 3) {
      headings.push({ depth, text: plain, id });
    }

    const inner = this.parser.parseInline(tokens);
    return `<h${depth} id="${id}"><a class="anchor" href="#${id}" aria-hidden="true">#</a>${inner}</h${depth}>\n`;
  };

  // Keep default for everything else; allow raw HTML
  return { renderer, headings };
}

function rewriteInternalLinks(html, pageOutRel, pageMap) {
  // Rewrite href="foo.md" and href="./guide/bar.md#x" to built paths
  return html.replace(
    /href="([^"]+\.md)(#[^"]*)?"/g,
    (full, mdHref, hash) => {
      // Resolve relative to current page's content path
      const pageMdRel = [...pageMap.entries()].find(
        ([, v]) => v.outRel === pageOutRel
      )?.[0];
      if (!pageMdRel) return full;

      const fromDir = dirname(pageMdRel);
      // normalize ./ and ../
      const parts = (fromDir === "." ? mdHref : join(fromDir, mdHref))
        .split(/[/\\]/)
        .filter(Boolean);
      const stack = [];
      for (const p of parts) {
        if (p === ".") continue;
        if (p === "..") stack.pop();
        else stack.push(p);
      }
      const targetMd = stack.join("/");
      const target = pageMap.get(targetMd);
      if (!target) {
        console.warn(`  warn: unresolved link ${mdHref} from ${pageMdRel}`);
        return full;
      }
      const anchor = hash ? hash.slice(1) : null;
      const href = hrefBetween(pageOutRel, target.outRel, anchor);
      return `href="${href}"`;
    }
  );
}

// ─── tiny mustache-ish templating ───────────────────────────────────────────

function renderTemplate(tpl, ctx) {
  // {{#key}}...{{/key}} — show if truthy; supports nested {{inner}}
  // {{^key}}...{{/key}} — show if falsy
  // {{key}} — escape; raw HTML keys are marked with !_raw
  let out = tpl;

  // sections
  out = out.replace(
    /\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g,
    (_, key, body) => {
      const val = ctx[key];
      if (!val) return "";
      if (typeof val === "object" && !Array.isArray(val)) {
        return renderTemplate(body, { ...ctx, ...val });
      }
      return renderTemplate(body, ctx);
    }
  );

  out = out.replace(
    /\{\{\^(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g,
    (_, key, body) => (ctx[key] ? "" : body)
  );

  out = out.replace(/\{\{(\w+)\}\}/g, (_, key) => {
    const val = ctx[key];
    if (val == null) return "";
    // content/sidebar/page_toc are trusted HTML
    if (
      key === "content" ||
      key === "sidebar" ||
      key === "page_toc" ||
      key === "root" ||
      key === "site_tagline"
    ) {
      return String(val);
    }
    return escapeHtml(String(val));
  });

  return out;
}

// ─── sidebar ────────────────────────────────────────────────────────────────

/**
 * Whether any nav node under `item` (path or anchor child) matches the current page.
 * Used to expand nested API module trees (including private modules) when active.
 */
function navSubtreeContainsOutRel(item, pageMap, currentOutRel) {
  if (item.path) {
    const page = pageMap.get(item.path);
    if (page && page.outRel === currentOutRel) return true;
  }
  for (const child of item.children || []) {
    if (navSubtreeContainsOutRel(child, pageMap, currentOutRel)) return true;
  }
  return false;
}

/**
 * Render nav children. Supports:
 * - legacy in-page anchors: `{ title, anchor }` (href on parent page)
 * - nested routes: `{ title, path, children? }` (API module tree, public + private)
 */
function renderNavChildTree(children, parentPage, pageMap, currentOutRel, depth = 1) {
  const expand = children.some((c) =>
    navSubtreeContainsOutRel(c, pageMap, currentOutRel)
  );
  const parentActive = parentPage && parentPage.outRel === currentOutRel;
  const open = expand || parentActive;
  const parts = [];
  parts.push(
    `<div class="nav-children depth-${depth}${open ? "" : " is-collapsed"}">`
  );

  for (const child of children) {
    if (child.path) {
      const childPage = pageMap.get(child.path);
      if (!childPage) {
        console.warn(`  warn: nav child missing content: ${child.path}`);
        continue;
      }
      const childHref = hrefBetween(currentOutRel, childPage.outRel);
      const childActive = childPage.outRel === currentOutRel;
      const hasGrand = Boolean(child.children?.length);
      const hasActiveDesc =
        !childActive &&
        hasGrand &&
        navSubtreeContainsOutRel(child, pageMap, currentOutRel);
      parts.push(
        `<div class="nav-item nav-nested${childActive ? " is-active" : ""}${hasGrand ? " has-children" : ""}${hasActiveDesc ? " has-active-desc" : ""}">`
      );
      parts.push(
        `<a class="nav-link sub${childActive ? " active" : ""}" href="${childHref}">${escapeHtml(child.title)}</a>`
      );
      if (hasGrand) {
        parts.push(
          renderNavChildTree(
            child.children,
            childPage,
            pageMap,
            currentOutRel,
            depth + 1
          )
        );
      }
      parts.push(`</div>`);
    } else if (child.anchor && parentPage) {
      const childHref = hrefBetween(
        currentOutRel,
        parentPage.outRel,
        child.anchor
      );
      parts.push(
        `<a class="nav-link sub" href="${childHref}" data-anchor="${escapeHtml(child.anchor)}">${escapeHtml(child.title)}</a>`
      );
    }
  }

  parts.push(`</div>`);
  return parts.join("\n");
}

function buildSidebar(nav, pageMap, currentOutRel) {
  const parts = [];

  for (const group of nav.groups) {
    const groupId = group.label
      ? "g-" + slugify(group.label)
      : "g-root";
    const collapsed = group.collapsed === true;

    parts.push(`<div class="nav-group${collapsed ? " is-collapsed" : ""}" data-group="${groupId}">`);
    if (group.label) {
      parts.push(
        `<button type="button" class="nav-group-toggle" aria-expanded="${collapsed ? "false" : "true"}">` +
          `<span class="nav-chevron" aria-hidden="true"></span>` +
          `<span class="nav-group-label">${escapeHtml(group.label)}</span>` +
          `</button>`
      );
    }
    parts.push(`<div class="nav-group-body">`);

    for (const item of group.items || []) {
      const page = pageMap.get(item.path);
      if (!page) {
        console.warn(`  warn: nav item missing content: ${item.path}`);
        continue;
      }
      const href = hrefBetween(currentOutRel, page.outRel);
      const isActive = page.outRel === currentOutRel;
      const hasChildren = item.children?.length;
      const subtreeActive =
        isActive ||
        (hasChildren &&
          navSubtreeContainsOutRel(item, pageMap, currentOutRel));

      parts.push(
        `<div class="nav-item${isActive ? " is-active" : ""}${hasChildren ? " has-children" : ""}${subtreeActive && !isActive ? " has-active-desc" : ""}">`
      );
      parts.push(
        `<a class="nav-link top${isActive ? " active" : ""}" href="${href}">${escapeHtml(item.title)}</a>`
      );

      if (hasChildren) {
        parts.push(
          renderNavChildTree(
            item.children,
            page,
            pageMap,
            currentOutRel,
            1
          )
        );
      }
      parts.push(`</div>`);
    }

    parts.push(`</div></div>`);
  }

  return parts.join("\n");
}

/**
 * Build a document-order TOC for API pages from the final HTML:
 * markdown h2 sections + h3.api-item-title names (function/struct titles).
 */
function collectApiPageToc(html) {
  const out = [];
  const re =
    /<h([23])\b([^>]*)>([\s\S]*?)<\/h\1>/gi;
  let m;
  while ((m = re.exec(html)) !== null) {
    const depth = Number(m[1]);
    const attrs = m[2] || "";
    const inner = m[3] || "";
    const idM = attrs.match(/\bid="([^"]+)"/i);
    if (!idM) continue;
    const id = idM[1];
    const isApiItem = /\bapi-item-title\b/.test(attrs);

    if (isApiItem) {
      let name = "";
      const dataName = attrs.match(/\bdata-api-name="([^"]*)"/i);
      if (dataName) {
        name = dataName[1]
          .replace(/&quot;/g, '"')
          .replace(/&amp;/g, "&")
          .replace(/&lt;/g, "<")
          .replace(/&gt;/g, ">");
      }
      if (!name) {
        const span = inner.match(
          /class="[^"]*\bapi-item-name\b[^"]*"[^>]*>([^<]+)/i
        );
        if (span) name = span[1];
      }
      if (!name) name = stripHtml(inner).trim();
      if (!name) continue;
      out.push({ depth: 3, text: name, id });
      continue;
    }

    if (depth === 2) {
      const text = stripHtml(inner).replace(/^#\s*/, "").trim();
      if (!text) continue;
      const base = id.replace(/-\d+$/, "");
      // Skip leftover rustdoc subsection noise
      if (["errors", "examples", "panics", "safety"].includes(base)) continue;
      // Dedupe repeated See also
      const prev = out[out.length - 1];
      if (prev && prev.id.startsWith("see-also") && id.startsWith("see-also")) {
        continue;
      }
      out.push({ depth: 2, text, id });
    }
  }
  return out;
}

function buildPageToc(headings) {
  // Drop noisy auto-slugs from rustdoc leftovers if any remain
  const skip = new Set(["errors", "examples", "panics", "safety"]);
  const filtered = headings.filter((h) => {
    if (h.depth !== 2 && h.depth !== 3) return false;
    const base = String(h.id || "").replace(/-\d+$/, "");
    if (h.depth === 2 && skip.has(base)) return false;
    return true;
  });

  // Dedupe consecutive identical see-also
  const deduped = [];
  for (const h of filtered) {
    const prev = deduped[deduped.length - 1];
    if (prev && prev.id.startsWith("see-also") && h.id.startsWith("see-also")) {
      continue;
    }
    deduped.push(h);
  }

  if (deduped.length < 2) return "";
  const items = deduped
    .map(
      (h) =>
        `<a class="page-toc-link depth-${h.depth}" href="#${h.id}">${escapeHtml(h.text)}</a>`
    )
    .join("\n");
  return `<nav class="page-toc-nav">${items}</nav>`;
}

/** Load curated related.json map (ignore _comment and missing files). */
function loadRelatedMap() {
  if (!existsSync(RELATED_PATH)) return new Map();
  try {
    const raw = JSON.parse(readFileSync(RELATED_PATH, "utf8"));
    const map = new Map();
    for (const [k, v] of Object.entries(raw)) {
      if (k.startsWith("_")) continue;
      if (!Array.isArray(v)) continue;
      map.set(
        k,
        v.filter((p) => typeof p === "string" && p.endsWith(".md"))
      );
    }
    return map;
  } catch (err) {
    console.warn(`  warn: could not parse related.json: ${err.message}`);
    return new Map();
  }
}

/**
 * Find the nav group containing `mdRel` and return sibling pages
 * (same group, excluding self).
 */
function navSiblingsFor(nav, pageMap, mdRel) {
  for (const group of nav.groups || []) {
    const items = (group.items || []).filter((it) => pageMap.has(it.path));
    if (!items.some((it) => it.path === mdRel)) continue;
    return {
      label: group.label || "Docs",
      siblings: items
        .filter((it) => it.path !== mdRel)
        .map((it) => {
          const page = pageMap.get(it.path);
          return page
            ? { title: it.title || page.title, mdRel: it.path, page }
            : null;
        })
        .filter(Boolean),
    };
  }
  return null;
}

/**
 * Strip trailing maintainer "Related / See also / Next / Connections" H2
 * sections from markdown so the build-injected block is the single footer.
 * API pages keep their generated See also (no related.json entry).
 */
function stripTrailingSeeAlso(body) {
  const re =
    /\n##[ \t]+(See also|Related|Next|Connections|Related entry points in the repo)\b[\s\S]*$/i;
  return body.replace(re, "\n").replace(/\s+$/, "\n");
}

/**
 * Build the HTML "See also" block: section siblings + curated related topics.
 */
function buildSeeAlsoHtml(page, pageMap, nav, relatedMap) {
  // API pages already include a generated See also; skip injection there.
  if (page.mdRel.startsWith("api/")) return "";

  const groups = [];
  const used = new Set([page.mdRel]);

  const sib = navSiblingsFor(nav, pageMap, page.mdRel);
  if (sib?.siblings?.length) {
    const links = [];
    for (const s of sib.siblings) {
      used.add(s.mdRel);
      const href = hrefBetween(page.outRel, s.page.outRel);
      links.push(
        `<li><a href="${href}">${escapeHtml(s.title)}</a></li>`
      );
    }
    groups.push({
      label: `Also in ${sib.label}`,
      html: `<ul class="see-also-list">${links.join("")}</ul>`,
    });
  }

  const curated = relatedMap.get(page.mdRel) || [];
  const relatedLinks = [];
  for (const md of curated) {
    if (used.has(md)) continue;
    const target = pageMap.get(md);
    if (!target) {
      console.warn(`  warn: related.json target missing: ${md} (from ${page.mdRel})`);
      continue;
    }
    used.add(md);
    const href = hrefBetween(page.outRel, target.outRel);
    const area = target.eyebrow || target.mdRel.split("/")[0] || "";
    relatedLinks.push(
      `<li>` +
        (area
          ? `<span class="see-also-area">${escapeHtml(area)}</span> `
          : "") +
        `<a href="${href}">${escapeHtml(target.title)}</a>` +
        (target.lede
          ? `<span class="see-also-desc">${escapeHtml(
              String(target.lede).slice(0, 110)
            )}</span>`
          : "") +
        `</li>`
    );
  }
  if (relatedLinks.length) {
    groups.push({
      label: "Related topics",
      html: `<ul class="see-also-list related">${relatedLinks.join("")}</ul>`,
    });
  }

  if (!groups.length) return "";

  const body = groups
    .map(
      (g) =>
        `<div class="see-also-group">` +
        `<div class="see-also-label">${escapeHtml(g.label)}</div>` +
        g.html +
        `</div>`
    )
    .join("");

  return (
    `<section class="see-also" aria-label="See also">` +
    `<h2 class="see-also-heading" id="see-also">` +
    `<a class="anchor" href="#see-also" aria-hidden="true">#</a>See also</h2>` +
    `<div class="see-also-grid">${body}</div>` +
    `</section>`
  );
}

// ─── page discovery ─────────────────────────────────────────────────────────

function loadPages() {
  const files = walk(CONTENT).filter((f) => f.endsWith(".md"));
  const pageMap = new Map(); // mdRel -> page

  for (const abs of files) {
    const mdRel = relative(CONTENT, abs).split(sep).join("/");
    const raw = readFileSync(abs, "utf8");
    const { data, body } = parseFrontmatter(raw);
    const outRel = mdPathToOut(mdRel);

    pageMap.set(mdRel, {
      mdRel,
      abs,
      outRel,
      url: mdPathToUrl(mdRel),
      data,
      body,
      title: data.title || basename(mdRel, ".md"),
      description: data.description || data.lede || "",
      eyebrow: data.eyebrow || "",
      lede: data.lede || "",
      order: Number(data.order) || 999,
    });
  }

  return pageMap;
}

function walkNavItems(items, pageMap, order) {
  for (const item of items || []) {
    if (item.path) {
      const page = pageMap.get(item.path);
      if (page) order.push(page);
    }
    if (item.children?.length) {
      walkNavItems(item.children, pageMap, order);
    }
  }
}

function flatNavOrder(nav, pageMap) {
  const order = [];
  for (const group of nav.groups) {
    walkNavItems(group.items || [], pageMap, order);
  }
  return order;
}

// ─── search index ───────────────────────────────────────────────────────────

function buildSearchIndex(pages) {
  // Chunk by heading for better hit granularity
  const docs = [];
  for (const page of pages) {
    const plain = stripHtml(page.html);
    docs.push({
      id: page.url,
      title: page.title,
      url: page.url,
      eyebrow: page.eyebrow || "",
      text: plain.slice(0, 12000),
      headings: page.headings.map((h) => ({ text: h.text, id: h.id })),
    });

    // Also index each h2/h3 section as its own hit target
    for (const h of page.headings) {
      docs.push({
        id: `${page.url}#${h.id}`,
        title: `${h.text} · ${page.title}`,
        url: `${page.url}#${h.id}`,
        eyebrow: page.title,
        text: h.text,
        headings: [],
        section: true,
      });
    }
  }
  return docs;
}

// ─── rustdoc extraction ─────────────────────────────────────────────────────

function extractRustdoc() {
  if (!existsSync(EXTRACT_SCRIPT)) {
    console.warn("  warn: extract-rustdoc.mjs missing; skipping API extraction");
    return;
  }
  const r = spawnSync(process.execPath, [EXTRACT_SCRIPT], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (r.stdout) process.stdout.write(r.stdout);
  if (r.stderr) process.stderr.write(r.stderr);
  if (r.status !== 0) {
    console.error("Rustdoc extraction failed");
    process.exit(r.status || 1);
  }
}

/** Merge generated API nav group into nav.json structure (replace prior API group). */
function mergeApiNav(nav) {
  if (!existsSync(API_NAV_FRAGMENT)) return nav;
  const fragment = JSON.parse(readFileSync(API_NAV_FRAGMENT, "utf8"));
  const groups = (nav.groups || []).filter(
    (g) => g.label !== "API Reference" && g.id !== "api"
  );
  // Keep guide groups first; API before Operations if present
  const opsIdx = groups.findIndex((g) => g.label === "Operations");
  if (opsIdx >= 0) {
    groups.splice(opsIdx, 0, fragment);
  } else {
    groups.push(fragment);
  }
  return { ...nav, groups };
}

// ─── main build ─────────────────────────────────────────────────────────────

function build() {
  const t0 = Date.now();
  console.log("Building docs…");

  if (!existsSync(CONTENT)) {
    console.error("Missing content/ directory");
    process.exit(1);
  }

  // Generate content/api/* from in-tree rustdoc before loading pages
  extractRustdoc();

  const nav = mergeApiNav(JSON.parse(readFileSync(NAV_PATH, "utf8")));
  const layout = readFileSync(join(THEME, "layout.html"), "utf8");
  const pageMap = loadPages();
  // Do not emit the nav fragment JSON as a page
  pageMap.delete("api/_nav_fragment.json");
  for (const key of [...pageMap.keys()]) {
    if (key.endsWith("_nav_fragment.json") || key.includes("_nav_fragment")) {
      pageMap.delete(key);
    }
  }

  if (pageMap.size === 0) {
    console.error("No markdown pages found in content/");
    process.exit(1);
  }

  const relatedMap = loadRelatedMap();

  // Render markdown for each page
  for (const page of pageMap.values()) {
    const { renderer, headings } = createRenderer();
    marked.setOptions({
      gfm: true,
      breaks: false,
      renderer,
    });
    // Guide pages: drop trailing Related/See also so the injected block is unique.
    // API pages keep the extractor-generated See also section in the body.
    const body = page.mdRel.startsWith("api/")
      ? page.body
      : stripTrailingSeeAlso(page.body);
    let html = marked.parse(body);
    html = rewriteInternalLinks(html, page.outRel, pageMap);
    const seeAlso = buildSeeAlsoHtml(page, pageMap, nav, relatedMap);
    if (seeAlso) {
      html += "\n" + seeAlso;
      // Ensure the right-rail TOC includes See also when we inject it.
      if (!headings.some((h) => h.id === "see-also")) {
        headings.push({ depth: 2, text: "See also", id: "see-also" });
      }
    }
    // API pages: rebuild TOC in document order with real item names
    // (raw HTML h3.api-item-title is invisible to the markdown heading collector).
    if (page.mdRel.startsWith("api/")) {
      const apiToc = collectApiPageToc(html);
      page.headings = apiToc.length >= 2 ? apiToc : headings;
    } else {
      page.headings = headings;
    }
    page.html = html;
  }

  const ordered = flatNavOrder(nav, pageMap);
  // Fall back to any pages not in nav, sorted by order then path
  const inNav = new Set(ordered.map((p) => p.mdRel));
  const extras = [...pageMap.values()]
    .filter((p) => !inNav.has(p.mdRel))
    .sort((a, b) => a.order - b.order || a.mdRel.localeCompare(b.mdRel));
  const allOrdered = [...ordered, ...extras];

  // Clean / prepare output (keep structure, rewrite files)
  ensureDir(OUT);

  // Copy theme assets
  const assetsSrc = join(THEME, "assets");
  const assetsDst = join(OUT, "assets");
  if (existsSync(assetsSrc)) {
    cpSync(assetsSrc, assetsDst, { recursive: true });
  }

  // Emit pages
  for (let i = 0; i < allOrdered.length; i++) {
    const page = allOrdered[i];
    const prev = i > 0 ? allOrdered[i - 1] : null;
    const next = i < allOrdered.length - 1 ? allOrdered[i + 1] : null;
    const root = rootPrefix(page.outRel);
    const sidebar = buildSidebar(nav, pageMap, page.outRel);
    const page_toc = buildPageToc(page.headings);

    const html = renderTemplate(layout, {
      title: page.title,
      description: page.description || page.lede || page.title,
      eyebrow: page.eyebrow,
      lede: page.lede,
      content: page.html,
      sidebar,
      page_toc,
      root,
      site_tagline: nav.tagline || "",
      prev: prev
        ? {
            title: prev.title,
            href: hrefBetween(page.outRel, prev.outRel),
          }
        : null,
      next: next
        ? {
            title: next.title,
            href: hrefBetween(page.outRel, next.outRel),
          }
        : null,
    });

    const outPath = join(OUT, page.outRel);
    ensureDir(dirname(outPath));
    writeFileSync(outPath, html, "utf8");
    console.log(`  ✓ ${page.outRel}`);
  }

  // Search index
  const index = buildSearchIndex(allOrdered);
  writeFileSync(
    join(OUT, "search-index.json"),
    JSON.stringify(index),
    "utf8"
  );
  console.log(`  ✓ search-index.json (${index.length} entries)`);

  // Fingerprint for cache-bust awareness (optional stamp)
  const stamp = createHash("sha1")
    .update(JSON.stringify(index.map((d) => d.id)))
    .digest("hex")
    .slice(0, 8);
  writeFileSync(join(OUT, "build-stamp.txt"), stamp + "\n", "utf8");

  console.log(`Done in ${Date.now() - t0}ms → ${relative(ROOT, OUT)}/`);
}

// ─── watch / serve ──────────────────────────────────────────────────────────

function watchAndRebuild() {
  let timer = null;
  const rebuild = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      try {
        build();
      } catch (e) {
        console.error("Build failed:", e);
      }
    }, 120);
  };

  const cratesDir = join(ROOT, "..", "crates");
  for (const dir of [CONTENT, THEME, ROOT, cratesDir]) {
    if (!existsSync(dir)) continue;
    try {
      watch(dir, { recursive: true }, (_ev, file) => {
        if (!file) return;
        if (file.includes("site" + sep) || file.includes("node_modules")) return;
        if (String(file).includes(`${sep}content${sep}api${sep}`)) return;
        if (
          file.endsWith(".md") ||
          file.endsWith(".html") ||
          file.endsWith(".css") ||
          file.endsWith(".js") ||
          file.endsWith(".json") ||
          file.endsWith(".rs")
        ) {
          console.log(`change: ${file}`);
          rebuild();
        }
      });
    } catch {
      // recursive watch may not work on all platforms; ignore
    }
  }
  console.log("Watching content/, theme/, nav.json, crates/ …");
}

function serve(port = 4173) {
  const mime = {
    ".html": "text/html; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".woff2": "font/woff2",
    ".txt": "text/plain; charset=utf-8",
  };

  createServer((req, res) => {
    let urlPath = decodeURIComponent((req.url || "/").split("?")[0]);
    if (urlPath === "/") urlPath = "/index.html";
    const filePath = join(OUT, urlPath);
    if (!filePath.startsWith(OUT) || !existsSync(filePath) || statSync(filePath).isDirectory()) {
      res.writeHead(404, { "Content-Type": "text/plain" });
      res.end("Not found");
      return;
    }
    const ext = extname(filePath);
    res.writeHead(200, { "Content-Type": mime[ext] || "application/octet-stream" });
    res.end(readFileSync(filePath));
  }).listen(port, () => {
    console.log(`Serving site/ at http://127.0.0.1:${port}/`);
  });
}

// ─── entry ──────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const doWatch = args.includes("--watch") || args.includes("--serve");
const doServe = args.includes("--serve");

build();
if (doWatch) watchAndRebuild();
if (doServe) serve(Number(process.env.PORT) || 4173);
