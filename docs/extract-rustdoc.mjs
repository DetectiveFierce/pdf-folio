#!/usr/bin/env node
/**
 * Extract rustdoc (//! and ///) from the workspace crates and emit Markdown
 * pages under content/api/ for the themed docs site.
 *
 * This does not shell out to rustdoc HTML — it presents the same information
 * (module docs, item signatures, doc comments) using the site's layout/theme.
 *
 * Usage:
 *   node extract-rustdoc.mjs
 */

import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  readdirSync,
  statSync,
  existsSync,
  rmSync,
} from "node:fs";
import { join, relative, dirname, basename, sep } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = join(__dirname, "..");
const CRATES = join(REPO, "crates");
const OUT = join(__dirname, "content", "api");

const CRATE_ORDER = [
  "pdf-folio-main",
  "pdf-folio-ui",
  "pdf-folio-core",
  "pdf-folio-cloud",
  "pdf-folio-style",
  "iced-widget-patch",
];

const CRATE_BLURBS = {
  "pdf-folio-main": "Desktop binary entrypoint (`pdf-folio`).",
  "pdf-folio-ui": "Iced application shell, library, viewer, and components.",
  "pdf-folio-core": "PDF rendering, SQLite library, search, and import (UI-free).",
  "pdf-folio-cloud": "Sync client, control-plane server, and Raindrop import.",
  "pdf-folio-style": "KDL style book, tokens, classes, and styled widgets.",
  "iced-widget-patch": "Local `iced_widget` patch (scrollable only).",
};

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

function escapeMd(s) {
  return String(s).replace(/\|/g, "\\|");
}

function slugify(text) {
  return String(text)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");
}

/** Map crate file path → rust module path segments relative to src/. */
function modulePathFromFile(crateName, fileAbs) {
  const srcRoot = join(CRATES, crateName, "src");
  let rel = relative(srcRoot, fileAbs).split(sep).join("/");
  if (rel.endsWith(".rs")) rel = rel.slice(0, -3);
  if (rel === "lib" || rel === "main") return [];
  if (rel.endsWith("/mod")) rel = rel.slice(0, -4);
  return rel.split("/").filter(Boolean);
}

function displayModuleName(crateName, segs) {
  if (segs.length === 0) return crateName.replace(/-/g, "_");
  return `${crateName.replace(/-/g, "_")}::${segs.join("::")}`;
}

function pageOutRel(crateName, segs) {
  if (segs.length === 0) return `api/${crateName}/index.md`;
  return `api/${crateName}/${segs.join("/")}.md`;
}

/** Relative href from one module page to another within the same crate. */
function hrefBetweenModules(fromSegs, toSegs) {
  const fromOut =
    fromSegs.length === 0 ? "index.md" : fromSegs.join("/") + ".md";
  const toOut = toSegs.length === 0 ? "index.md" : toSegs.join("/") + ".md";
  const fromDir = fromOut.includes("/")
    ? fromOut.slice(0, fromOut.lastIndexOf("/"))
    : "";
  const fromParts = fromDir ? fromDir.split("/") : [];
  const toParts = toOut.split("/");
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
  if (!href) href = toParts[toParts.length - 1];
  return href;
}

// ─── rust source parse (lightweight) ────────────────────────────────────────

// `const fn` must be matched before bare `const` so associated const-fns
// are not parsed as a const named `fn`.
const ITEM_KINDS = "const\\s+fn|fn|struct|enum|type|trait|const|static|mod|use";
const ITEM_RE = new RegExp(
  `^(\\s*)((?:pub(?:\\([^)]*\\))?\\s+)?)(?:async\\s+)?(${ITEM_KINDS})\\s+([A-Za-z_][A-Za-z0-9_]*)`
);

function stripDocLine(line) {
  const m = line.match(/^\s*\/\/\/\??\s?(.*)$/);
  if (m) return m[1];
  const m2 = line.match(/^\s*\/\/!\s?(.*)$/);
  if (m2) return m2[1];
  return null;
}

function collectLeadingDocs(lines, index) {
  // Walk upward collecting /// and attrs; return docs joined + start index
  let i = index - 1;
  const docLines = [];
  while (i >= 0) {
    const t = lines[i].trim();
    if (t === "") {
      i--;
      continue;
    }
    if (t.startsWith("#[")) {
      i--;
      continue;
    }
    if (t.startsWith("///") || t.startsWith("//!")) {
      // collect contiguous doc block above
      let j = i;
      const block = [];
      while (j >= 0) {
        const d = stripDocLine(lines[j]);
        if (d === null) {
          if (lines[j].trim() === "" || lines[j].trim().startsWith("#[")) {
            j--;
            continue;
          }
          break;
        }
        block.push(d);
        j--;
      }
      docLines.push(...block.reverse());
      break;
    }
    break;
  }
  return docLines.join("\n").trim();
}

function parseSignature(lines, startIdx) {
  // Capture up to opening `{` or `;` on same/following lines (cap length)
  let sig = lines[startIdx].trim();
  if (sig.includes("{")) {
    sig = sig.slice(0, sig.indexOf("{")).trim();
  }
  if (sig.endsWith(";")) sig = sig.slice(0, -1).trim();
  let i = startIdx;
  // multi-line signatures
  while (
    i + 1 < lines.length &&
    !lines[startIdx].includes("{") &&
    !lines[startIdx].trim().endsWith(";") &&
    !sig.includes("{") &&
    !sig.endsWith(";") &&
    (sig.endsWith(",") ||
      sig.endsWith("(") ||
      sig.endsWith("<") ||
      sig.endsWith("->") ||
      /:\s*$/.test(sig) ||
      sig.endsWith("&") ||
      lines[i + 1].trim().startsWith("//") === false)
  ) {
    const next = lines[i + 1].trim();
    if (next.startsWith("//") || next.startsWith("#[")) break;
    if (next === "" ) break;
    i++;
    let piece = next;
    if (piece.includes("{")) piece = piece.slice(0, piece.indexOf("{")).trim();
    if (piece.endsWith(";")) piece = piece.slice(0, -1).trim();
    sig += " " + piece;
    if (next.includes("{") || next.endsWith(";")) break;
    if (i - startIdx > 12) break;
  }
  // collapse whitespace
  sig = sig.replace(/\s+/g, " ").trim();
  if (sig.length > 220) sig = sig.slice(0, 217) + "…";
  return sig;
}

function parseRustFile(absPath) {
  const raw = readFileSync(absPath, "utf8");
  const lines = raw.split(/\r?\n/);

  // Module docs: //! at top (before first non-doc/attr/blank or after inner attrs)
  const modDocs = [];
  let i = 0;
  // skip shebang
  if (lines[0]?.startsWith("#!")) i = 1;
  while (i < lines.length) {
    const t = lines[i].trim();
    if (t === "" || t.startsWith("#![")) {
      i++;
      continue;
    }
    if (t.startsWith("//!")) {
      while (i < lines.length && lines[i].trim().startsWith("//!")) {
        modDocs.push(stripDocLine(lines[i]) ?? "");
        i++;
      }
      break;
    }
    break;
  }

  const items = [];
  const seen = new Set();

  for (let idx = 0; idx < lines.length; idx++) {
    const line = lines[idx];
    // skip lines inside comments roughly — if we're in a line that is pure comment
    if (line.trim().startsWith("//")) continue;

    const m = line.match(ITEM_RE);
    if (!m) continue;

    const indent = m[1];
    const visRaw = (m[2] || "").trim(); // "pub", "pub(crate)", ""
    // Normalize `const fn` → fn (name is the real function name)
    const kindRaw = m[3].replace(/\s+/g, " ");
    const kind = kindRaw === "const fn" ? "fn" : kindRaw;
    const name = m[4];

    // Skip non-visible private items at module level? Keep crate-visible for maintainers.
    // Include: pub, pub(crate), pub(super), and top-level private modules with docs.
    const isPub = visRaw.startsWith("pub");
    // Skip heavily indented nested private helpers unless public
    const indentLevel = indent.replace(/\t/g, "    ").length;
    if (!isPub && indentLevel > 0) continue;
    if (!isPub && kind !== "mod" && kind !== "struct" && kind !== "enum") {
      // still skip private free functions to reduce noise
      if (kind === "fn" || kind === "const" || kind === "static" || kind === "type") {
        continue;
      }
    }

    // Skip test modules and obvious test fns
    if (name === "tests" || name.startsWith("test_")) continue;

    const docs = collectLeadingDocs(lines, idx);
    const visibility = visRaw || "private";
    const key = `${kind}:${name}:${idx}`;
    if (seen.has(`${kind}:${name}:${visibility}`)) {
      // allow impl methods with same name — use line in key for listing
    }
    seen.add(`${kind}:${name}:${visibility}:${idx}`);

    // Skip `use` re-exports without docs to reduce noise, but keep documented ones
    if (kind === "use" && !docs) continue;

    const sig = parseSignature(lines, idx);
    items.push({
      kind,
      name,
      visibility,
      docs,
      signature: sig,
      line: idx + 1,
      indent: indentLevel,
    });
  }

  return {
    moduleDocs: modDocs.join("\n").trim(),
    items,
    lineCount: lines.length,
  };
}

// ─── emit markdown ──────────────────────────────────────────────────────────

function itemAnchor(item) {
  // Include line so overloaded names (e.g. multiple `new` methods) stay unique.
  return slugify(`${item.kind}-${item.name}-${item.line}`);
}

function visibilityBadge(vis) {
  if (vis === "pub") return "`pub`";
  if (vis.startsWith("pub(")) return "`" + vis + "`";
  return "`" + vis + "`";
}

/** Must match site.js tokenClassForKind — one class per kind site-wide. */
function nameTokenClass(kind) {
  if (kind === "mod" || kind === "use") return "tok-m";
  if (kind === "fn") return "tok-f"; // orange — functions
  if (kind === "const" || kind === "static") return "tok-const"; // purple — constants
  if (
    kind === "struct" ||
    kind === "enum" ||
    kind === "type" ||
    kind === "trait" ||
    kind === "impl"
  ) {
    return "tok-t";
  }
  return "tok-f";
}

/**
 * Demote ATX headings outside fenced code blocks.
 *
 * Rustdoc module/item docs often use `# Section` (h1). Embedded under our page
 * title + "## Module documentation", those become content h1s whose anchor
 * `#` glyphs stay always-visible (CSS only restyles h2–h4 anchors). Shift by
 * `by` levels so embedded docs sit under the page chrome as h2+.
 */
function demoteMarkdownHeadings(md, by = 1) {
  if (!md || by <= 0) return md;
  const lines = String(md).split("\n");
  let inFence = false;
  return lines
    .map((line) => {
      const trimmed = line.trimStart();
      if (/^(`{3,}|~{3,})/.test(trimmed)) {
        inFence = !inFence;
        return line;
      }
      if (inFence) return line;
      const m = line.match(/^(#{1,6})([ \t]+.*)$/);
      if (!m) return line;
      const n = Math.min(6, m[1].length + by);
      return "#".repeat(n) + m[2];
    })
    .join("\n");
}

/** Known third-party crates → docs.rs (or first-page URL). */
const EXTERNAL_DOCS = {
  iced: "https://docs.rs/iced",
  anyhow: "https://docs.rs/anyhow",
  clap: "https://docs.rs/clap",
  tokio: "https://docs.rs/tokio",
  tracing: "https://docs.rs/tracing",
  tracing_subscriber: "https://docs.rs/tracing_subscriber",
  rusqlite: "https://docs.rs/rusqlite",
  tantivy: "https://docs.rs/tantivy",
  pdfium_render: "https://docs.rs/pdfium-render",
  "pdfium-render": "https://docs.rs/pdfium-render",
  reqwest: "https://docs.rs/reqwest",
  axum: "https://docs.rs/axum",
  serde: "https://docs.rs/serde",
  kdl: "https://docs.rs/kdl",
  chrono: "https://docs.rs/chrono",
  notify: "https://docs.rs/notify",
  blake3: "https://docs.rs/blake3",
  jsonwebtoken: "https://docs.rs/jsonwebtoken",
};

/**
 * Build lookup tables so rustdoc intra-doc links can become site-local hrefs.
 * Keys: simple name ("PdfDoc"), module path ("db::types"), full-ish paths.
 */
function buildLinkIndex(modules) {
  /** @type {Map<string, {crateName:string, segs:string[], anchor?:string, kind:string}>} */
  const bySimple = new Map();
  /** @type {Map<string, {crateName:string, segs:string[], anchor?:string, kind:string}>} */
  const byPath = new Map(); // crate::segs::Name or segs::Name within crate

  for (const m of modules) {
    const crateSnake = m.crateName.replace(/-/g, "_");
    const modPath = m.segs.join("::");
    const modKey = modPath || crateSnake;

    // Module itself
    const modTarget = {
      crateName: m.crateName,
      segs: m.segs,
      kind: "mod",
    };
    byPath.set(`${crateSnake}::${modKey}`, modTarget);
    byPath.set(modKey, modTarget);
    if (m.segs.length) {
      bySimple.set(m.segs[m.segs.length - 1], modTarget);
    }

    for (const it of m.items || []) {
      if (!it.visibility?.startsWith?.("pub") && it.kind !== "mod") {
        // Still index private types used in same-crate docs
      }
      if (it.kind === "use") continue;
      const anchor = itemAnchor(it);
      const target = {
        crateName: m.crateName,
        segs: m.segs,
        anchor,
        kind: it.kind,
      };
      // Prefer first public definition for simple name
      const existing = bySimple.get(it.name);
      if (!existing || (it.visibility?.startsWith?.("pub") && existing.kind === "mod")) {
        bySimple.set(it.name, target);
      }
      if (modPath) {
        byPath.set(`${modPath}::${it.name}`, target);
        byPath.set(`${crateSnake}::${modPath}::${it.name}`, target);
      } else {
        byPath.set(`${crateSnake}::${it.name}`, target);
      }
      // Enum variants written as Class::LibraryCard → link to Class
      byPath.set(`${it.name}`, target);
    }
  }

  return { bySimple, byPath };
}

/**
 * Resolve a rustdoc path relative to the current module.
 * @returns {{crateName:string, segs:string[], anchor?:string}|null}
 */
function resolveRustdocPath(path, crateName, segs, index) {
  if (!path) return null;
  let p = path.trim();
  // Strip leading :: 
  p = p.replace(/^::/, "");
  // Enum/path with variant or associated item: Type::Variant → Type
  // Keep module::Type as-is when middle segments look like modules (snake_case)

  // External crate?
  const first = p.split("::")[0];
  if (EXTERNAL_DOCS[first] || EXTERNAL_DOCS[first.replace(/-/g, "_")]) {
    return {
      external:
        EXTERNAL_DOCS[first] || EXTERNAL_DOCS[first.replace(/-/g, "_")],
    };
  }

  const crateSnake = crateName.replace(/-/g, "_");

  if (p.startsWith("crate::")) {
    p = p.slice("crate::".length);
    // Prefer full path under this crate
    const full = `${crateSnake}::${p}`;
    if (index.byPath.has(full)) return index.byPath.get(full);
    // Truncate trailing path segments until match (Class::LibraryCard → Class)
    const parts = p.split("::");
    while (parts.length) {
      const tryPath = `${crateSnake}::${parts.join("::")}`;
      if (index.byPath.has(tryPath)) return index.byPath.get(tryPath);
      const trySimple = parts[parts.length - 1];
      // If last is PascalCase type name
      if (index.bySimple.has(trySimple)) {
        const t = index.bySimple.get(trySimple);
        if (t.crateName === crateName) return t;
      }
      parts.pop();
    }
    if (index.bySimple.has(p)) {
      const t = index.bySimple.get(p);
      if (t.crateName === crateName) return t;
    }
    return null;
  }

  if (p.startsWith("super::")) {
    const rest = p.slice("super::".length);
    if (segs.length === 0) return null;
    const parentSegs = segs.slice(0, -1);
    // super::crdt from sync/blobs → sync/crdt
    const restParts = rest.split("::").filter(Boolean);
    // If rest is a single module sibling under parent
    const candidateSegs = [...parentSegs, ...restParts];
    // Try as module page
    const asMod = index.byPath.get(candidateSegs.join("::"));
    if (asMod && asMod.kind === "mod") return asMod;
    // Try parent module path + item
    const parentPath = parentSegs.join("::");
    if (restParts.length === 1 && index.bySimple.has(restParts[0])) {
      const t = index.bySimple.get(restParts[0]);
      if (t.crateName === crateName) return t;
    }
    if (parentPath && index.byPath.has(`${parentPath}::${rest}`)) {
      return index.byPath.get(`${parentPath}::${rest}`);
    }
    // Sibling module under same parent: super::foo when foo is module
    if (restParts.length === 1) {
      const sib = [...parentSegs, restParts[0]];
      const key = sib.join("::");
      if (index.byPath.has(key)) return index.byPath.get(key);
      // from classes/viewer, super::Class lives on classes module
      if (index.byPath.has(`${parentPath}::${restParts[0]}`)) {
        return index.byPath.get(`${parentPath}::${restParts[0]}`);
      }
      if (index.bySimple.has(restParts[0])) {
        const t = index.bySimple.get(restParts[0]);
        if (t.crateName === crateName) return t;
      }
    }
    return null;
  }

  if (p.startsWith("self::")) {
    p = p.slice("self::".length);
    const local = [...segs, ...p.split("::")].filter(Boolean);
    const key = local.join("::");
    if (index.byPath.has(key)) return index.byPath.get(key);
    if (index.bySimple.has(p.split("::").pop())) {
      const t = index.bySimple.get(p.split("::").pop());
      if (t.crateName === crateName) return t;
    }
    return null;
  }

  // Bare path: Foo, db::types, pdf::PdfDoc
  if (index.byPath.has(p)) return index.byPath.get(p);
  if (index.byPath.has(`${crateSnake}::${p}`)) {
    return index.byPath.get(`${crateSnake}::${p}`);
  }
  // Type::Variant
  if (p.includes("::")) {
    const parts = p.split("::");
    // Prefer longest module/item prefix that exists
    for (let i = parts.length; i >= 1; i--) {
      const sub = parts.slice(0, i).join("::");
      if (index.byPath.has(sub)) return index.byPath.get(sub);
      if (index.byPath.has(`${crateSnake}::${sub}`)) {
        return index.byPath.get(`${crateSnake}::${sub}`);
      }
    }
  }
  if (index.bySimple.has(p)) {
    const t = index.bySimple.get(p);
    // Prefer same crate
    if (t.crateName === crateName) return t;
    return t;
  }
  return null;
}

function hrefForTarget(fromCrate, fromSegs, target) {
  if (target.external) return target.external;
  if (target.crateName !== fromCrate) {
    // Cross-crate: path from content/api/from... to content/api/other...
    const fromOut =
      fromSegs.length === 0
        ? `api/${fromCrate}/index.md`
        : `api/${fromCrate}/${fromSegs.join("/")}.md`;
    const toOut =
      target.segs.length === 0
        ? `api/${target.crateName}/index.md`
        : `api/${target.crateName}/${target.segs.join("/")}.md`;
    // Compute relative from fromOut dir to toOut
    const fromDir = fromOut.includes("/")
      ? fromOut.slice(0, fromOut.lastIndexOf("/"))
      : "";
    const fromParts = fromDir ? fromDir.split("/") : [];
    const toParts = toOut.split("/");
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
    if (!href) href = toParts[toParts.length - 1];
    if (target.anchor) href += `#${target.anchor}`;
    return href;
  }
  let href = hrefBetweenModules(fromSegs, target.segs);
  if (target.anchor) href += `#${target.anchor}`;
  return href;
}

/**
 * Rewrite rustdoc-style links in markdown to site-local (or docs.rs) hrefs.
 * Handles: [text](crate::…), [text](super::…), bare [`Type`], reference defs.
 */
function rewriteRustdocLinks(md, crateName, segs, index) {
  if (!md) return md;

  const lines = String(md).split("\n");
  let inFence = false;
  const out = [];

  // Collect reference-style link defs to leave alone / resolve
  // [pdfium-render]: https://...

  for (let line of lines) {
    const trimmed = line.trimStart();
    if (/^(`{3,}|~{3,})/.test(trimmed)) {
      inFence = !inFence;
      out.push(line);
      continue;
    }
    if (inFence) {
      out.push(line);
      continue;
    }

    // Resolve reference definitions: [name]: crate::path or URL
    const refDef = line.match(
      /^(\s*)\[([^\]]+)\]:\s+(\S+)(.*)$/
    );
    if (refDef) {
      const [, indent, name, target, rest] = refDef;
      if (/^https?:\/\//.test(target)) {
        out.push(line);
        continue;
      }
      const resolved = resolveRustdocPath(target, crateName, segs, index);
      if (resolved) {
        const href = hrefForTarget(crateName, segs, resolved);
        out.push(`${indent}[${name}]: ${href}${rest}`);
      } else {
        out.push(line);
      }
      continue;
    }

    // [label](path) — inline links that are rustdoc paths (not http/md/relative site)
    line = line.replace(
      /\[([^\]]+)\]\(([^)]+)\)/g,
      (full, label, dest) => {
        const d = dest.trim();
        if (
          /^https?:\/\//.test(d) ||
          d.startsWith("#") ||
          d.endsWith(".md") ||
          d.endsWith(".html") ||
          d.startsWith("/") ||
          d.startsWith("mailto:")
        ) {
          return full;
        }
        // Relative ./ paths kept
        if (d.startsWith("./") || d.startsWith("../")) return full;
        // Keyword-only targets are not useful links
        if (d === "self" || d === "super" || d === "crate") {
          return label; // drop broken href, keep label text
        }

        const resolved = resolveRustdocPath(d, crateName, segs, index);
        if (!resolved) {
          // Drop dead rustdoc-style hrefs so the site never ships crate::… links
          if (
            d.includes("::") ||
            d.startsWith("crate") ||
            d.startsWith("super") ||
            d.startsWith("self")
          ) {
            return label;
          }
          return full;
        }
        const href = hrefForTarget(crateName, segs, resolved);
        return `[${label}](${href})`;
      }
    );

    // Bare rustdoc links: [`Type`] or [Type] not already followed by (
    // Also [`mod::path`] and hyphenated crate names (`pdfium-render`).
    line = line.replace(
      /\[`([A-Za-z_][A-Za-z0-9_:\-]*)`\](?!\()/g,
      (full, name) => {
        if (name === "self" || name === "super" || name === "crate") return full;
        const resolved = resolveRustdocPath(name, crateName, segs, index);
        if (!resolved) return full;
        const href = hrefForTarget(crateName, segs, resolved);
        return `[\`${name}\`](${href})`;
      }
    );
    line = line.replace(
      /\[([A-Za-z_][A-Za-z0-9_:\-]*)\](?!\()/g,
      (full, name) => {
        // skip if looks like a footnote, keyword, or empty
        if (name.length < 2) return full;
        if (name === "self" || name === "super" || name === "crate") return full;
        const resolved = resolveRustdocPath(name, crateName, segs, index);
        if (!resolved) return full;
        const href = hrefForTarget(crateName, segs, resolved);
        return `[\`${name}\`](${href})`;
      }
    );

    out.push(line);
  }

  return out.join("\n");
}

/** Human section titles for kind groups (never bare `fn` / `struct` as the H2). */
function kindSectionTitle(kind) {
  return (
    {
      mod: "Modules",
      struct: "Structs",
      enum: "Enums",
      type: "Type aliases",
      trait: "Traits",
      const: "Constants",
      static: "Statics",
      fn: "Functions",
      use: "Imports",
      impl: "Implementations",
    }[kind] || kind
  );
}

/**
 * Turn ATX headings inside item rustdoc into non-heading markup so they do not
 * pollute the page TOC (e.g. dozens of "## Errors" entries).
 */
function flattenItemDocHeadings(md) {
  if (!md) return md;
  const lines = String(md).split("\n");
  let inFence = false;
  return lines
    .map((line) => {
      const trimmed = line.trimStart();
      if (/^(`{3,}|~{3,})/.test(trimmed)) {
        inFence = !inFence;
        return line;
      }
      if (inFence) return line;
      const m = line.match(/^(#{1,6})\s+(.*)$/);
      if (!m) return line;
      const title = m[2].trim();
      if (!title) return line;
      return `<p class="api-doc-heading"><strong>${title}</strong></p>\n`;
    })
    .join("\n");
}

function renderItem(item, crateName, segs, linkIndex) {
  const parts = [];
  const anchor = itemAnchor(item);
  const nameCls = nameTokenClass(item.kind);
  // Name is the title; kind is a secondary badge (not the other way around).
  // data-api-name lets the builder add real function names to the page TOC.
  parts.push(
    `<h3 class="api-item-title" id="${anchor}" data-api-name="${escapeMd(item.name).replace(/"/g, "&quot;")}" data-api-kind="${item.kind}"><span class="${nameCls} api-item-name">${item.name}</span> <span class="tok-k api-item-kind">${item.kind}</span></h3>`
  );
  parts.push("");
  parts.push(
    `<p class="api-item-meta"><code class="api-vis">${item.visibility || "private"}</code> · line ${item.line}</p>`
  );
  parts.push("");
  parts.push("```rust");
  parts.push(item.signature);
  parts.push("```");
  parts.push("");
  if (item.docs) {
    let docs = demoteMarkdownHeadings(item.docs, 1);
    docs = rewriteRustdocLinks(docs, crateName, segs, linkIndex);
    // Flatten remaining ATX headings so "# Errors" does not become TOC noise.
    docs = flattenItemDocHeadings(docs);
    parts.push(`<div class="api-item-docs">\n\n${docs}\n\n</div>`);
  } else {
    parts.push(
      `<p class="api-item-docs dim"><em>No rustdoc comment on this item yet.</em></p>`
    );
  }
  parts.push("");
  return parts.join("\n");
}

function renderModulePage({
  crateName,
  segs,
  moduleName,
  moduleDocs,
  items,
  sourceRel,
  children,
  guideLinks,
  linkIndex,
}) {
  const title = segs.length === 0 ? crateName : moduleName;
  const eyebrow =
    segs.length === 0 ? "API · Crate" : `API · ${crateName}`;
  const lede =
    moduleDocs.split("\n").find((l) => l.trim()) ||
    `Rust module \`${moduleName}\`.`;

  const lines = [];
  const fm = (s) => String(s).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  lines.push("---");
  lines.push(`title: "${fm(title)}"`);
  lines.push(`eyebrow: "${fm(eyebrow)}"`);
  const ledeOne = lede.replace(/\s+/g, " ").trim().slice(0, 200);
  lines.push(`lede: "${fm(ledeOne)}"`);
  lines.push(`order: 100`);
  lines.push("---");
  lines.push("");

  lines.push(
    `Source: \`${sourceRel}\` · Module: \`${moduleName}\``
  );
  lines.push("");

  if (guideLinks?.length) {
    lines.push("**Guides:** " + guideLinks.map((g) => `[${g.title}](${g.href})`).join(" · "));
    lines.push("");
  }

  if (moduleDocs) {
    lines.push("## Module documentation");
    lines.push("");
    // Demote rustdoc `# Section` → `## Section` so it sits under the page title
    // without producing content h1s (which left always-visible `#` anchors).
    let docs = demoteMarkdownHeadings(moduleDocs, 1);
    docs = rewriteRustdocLinks(docs, crateName, segs, linkIndex);
    lines.push(docs);
    lines.push("");
  } else {
    lines.push("## Module documentation");
    lines.push("");
    lines.push("*This module has no `//!` rustdoc comment yet.*");
    lines.push("");
  }

  if (children?.length) {
    lines.push("## Submodules");
    lines.push("");
    lines.push("| Module | Summary |");
    lines.push("| --- | --- |");
    for (const ch of children) {
      const summary = (ch.moduleDocs || "")
        .split("\n")
        .find((l) => l.trim())
        ?.replace(/\s+/g, " ")
        .trim()
        .slice(0, 120) || "—";
      const relHref = hrefBetweenModules(segs, ch.segs);
      lines.push(
        `| [\`${ch.moduleName}\`](${relHref}) | ${escapeMd(summary)} |`
      );
    }
    lines.push("");
  }

  // Group items
  const byKind = new Map();
  for (const it of items) {
    // skip nested deep private noise
    if (it.indent > 0 && !it.visibility.startsWith("pub")) continue;
    const list = byKind.get(it.kind) || [];
    list.push(it);
    byKind.set(it.kind, list);
  }

  const kindOrder = [
    "mod",
    "struct",
    "enum",
    "type",
    "trait",
    "const",
    "static",
    "fn",
    "use",
  ];

  const pubItems = items.filter((it) => it.visibility.startsWith("pub"));
  const documented = pubItems.filter((it) => it.docs).length;

  lines.push("## Items");
  lines.push("");
  lines.push(
    `${items.length} extracted item(s); ${documented}/${pubItems.length} \`pub\` item(s) have rustdoc.`
  );
  lines.push("");

  // TOC of items
  if (items.length > 0) {
    lines.push("| Kind | Name | Visibility | Docs |");
    lines.push("| --- | --- | --- | --- |");
    for (const kind of kindOrder) {
      for (const it of byKind.get(kind) || []) {
        if (it.indent > 0 && !it.visibility.startsWith("pub")) continue;
        const mark = it.docs ? "yes" : "—";
        lines.push(
          `| \`${it.kind}\` | [\`${it.name}\`](#${itemAnchor(it)}) | ${visibilityBadge(it.visibility)} | ${mark} |`
        );
      }
    }
    lines.push("");
  }

  for (const kind of kindOrder) {
    const list = byKind.get(kind) || [];
    const filtered = list.filter(
      (it) => !(it.indent > 0 && !it.visibility.startsWith("pub"))
    );
    if (!filtered.length) continue;
    // Human titles: "Functions", not bare "fn"
    lines.push(`## ${kindSectionTitle(kind)}`);
    lines.push("");
    for (const it of filtered) {
      lines.push(renderItem(it, crateName, segs, linkIndex));
    }
  }

  lines.push("## See also");
  lines.push("");
  const rootUp = contentRootPrefix(segs);

  // Parent module (when nested). Skip virtual parents that have no page
  // (e.g. `bin/` binaries — only leaf pages exist, no bin.md).
  if (segs.length > 0) {
    let parentSegs = segs.slice(0, -1);
    // `src/bin/foo.rs` → segs ["bin","foo"]; there is no bin module page.
    if (parentSegs.length === 1 && parentSegs[0] === "bin") {
      parentSegs = [];
    }
    const parentName =
      parentSegs.length === 0
        ? crateName.replace(/-/g, "_")
        : `${crateName.replace(/-/g, "_")}::${parentSegs.join("::")}`;
    lines.push(
      `- Parent: [\`${parentName}\`](${hrefBetweenModules(segs, parentSegs)})`
    );
  }

  // Child modules (when this page is a parent)
  if (children?.length) {
    const childLinks = children
      .slice(0, 10)
      .map((ch) => {
        const short = ch.segs[ch.segs.length - 1] || ch.moduleName;
        return `[\`${short}\`](${hrefBetweenModules(segs, ch.segs)})`;
      })
      .join(" · ");
    if (childLinks) lines.push(`- Submodules: ${childLinks}`);
  }

  if (guideLinks?.length) {
    lines.push(
      "- Guides: " +
        guideLinks.map((g) => `[${g.title}](${g.href})`).join(" · ")
    );
  }

  lines.push(`- [API index](${rootUp}api/index.md)`);
  if (segs.length > 0) {
    lines.push(
      `- [Crate root \`${crateName}\`](${hrefBetweenModules(segs, [])})`
    );
  }
  lines.push(`- [Workspace map](${rootUp}architecture/workspace.md)`);
  lines.push(`- [Glossary](${rootUp}reference/glossary.md)`);
  lines.push("");

  return lines.join("\n");
}

/** How many `../` steps from an API module page up to `content/`. */
function contentRootPrefix(segs) {
  // api/crate/index.md or api/crate/foo.md → 2; api/crate/a/b.md → 3; …
  const ups = segs.length <= 1 ? 2 : segs.length + 1;
  return "../".repeat(ups);
}

// Guide cross-links by crate/module prefix (shown under the module title)
function guideLinksFor(crateName, segs) {
  const links = [];
  const seen = new Set();
  const add = (title, href) => {
    const key = `${title}|${href}`;
    if (seen.has(key)) return;
    seen.add(key);
    links.push({ title, href });
  };

  const up = contentRootPrefix(segs);

  if (crateName === "pdf-folio-core") {
    if (segs.length === 0) {
      add("Rendering pipeline", `${up}subsystems/rendering.md`);
      add("Library database", `${up}subsystems/database.md`);
      add("Search & watching", `${up}subsystems/search.md`);
    }
    if (segs[0] === "pdf") {
      add("Rendering pipeline", `${up}subsystems/rendering.md`);
      if (segs[1] === "document" || segs[1] === "renderer" || segs[1] === "geometry") {
        add("Viewer UI map", `${up}crates/ui.md#viewer-viewer-mode-domain`);
      }
    }
    if (segs[0] === "db") {
      add("Library database", `${up}subsystems/database.md`);
      if (segs[1] === "search" || segs[1] === "import")
        add("Search & watching", `${up}subsystems/search.md`);
      if (segs[1] === "sync")
        add("Cross-device sync", `${up}subsystems/sync.md`);
      if (segs[1] === "raindrop")
        add("Raindrop import", `${up}subsystems/raindrop.md`);
      if (segs[1] === "organization" || segs[1] === "library")
        add("Bulk action walkthrough", `${up}subsystems/bulk-action.md`);
      if (segs[1] === "types" || segs[1] === "schema")
        add("Glossary", `${up}reference/glossary.md`);
    }
  }
  if (crateName === "pdf-folio-ui") {
    if (segs.length === 0) {
      add("Architecture overview", `${up}architecture/overview.md`);
      add("Application shell", `${up}architecture/shell.md`);
      add("Runtime state", `${up}architecture/state.md`);
    }
    if (segs[0] === "shell") {
      add("Application shell", `${up}architecture/shell.md`);
      if (segs[1] === "messages")
        add("Message surface", `${up}architecture/messages.md`);
      if (segs[1] === "app" || segs[1] === "session")
        add("Runtime state", `${up}architecture/state.md`);
      if (segs[1] === "update")
        add("Update routing", `${up}architecture/overview.md#update-routing`);
    }
    if (segs[0] === "viewer" || (segs[0] === "components" && segs[1] === "viewer")) {
      add("Rendering pipeline", `${up}subsystems/rendering.md`);
      add("Runtime state", `${up}architecture/state.md#viewer-viewerruntime`);
    }
    if (segs[0] === "library" || (segs[0] === "components" && segs[1] === "library")) {
      add("UI crate map", `${up}crates/ui.md`);
      add("Bulk action walkthrough", `${up}subsystems/bulk-action.md`);
      add("Library database", `${up}subsystems/database.md`);
      if (segs[1] === "registry" || segs.includes("registry"))
        add("Multi-library registry", `${up}subsystems/multi-library.md`);
    }
    if (segs[0] === "components" && segs[1] === "shared") {
      add("Application shell", `${up}architecture/shell.md`);
      add("Style system", `${up}subsystems/style-system.md`);
    }
  }
  if (crateName === "pdf-folio-cloud") {
    if (segs.length === 0) {
      add("Cross-device sync", `${up}subsystems/sync.md`);
      add("Raindrop import", `${up}subsystems/raindrop.md`);
    }
    if (segs[0] === "sync" || segs[0] === "bin") {
      add("Cross-device sync", `${up}subsystems/sync.md`);
      add("CLI reference", `${up}operations/cli.md`);
      if (segs[1] === "crdt" || segs[1] === "run")
        add("Database sync tables", `${up}subsystems/database.md#sync-tables-local`);
    }
    if (segs[0] === "raindrop") {
      add("Raindrop import", `${up}subsystems/raindrop.md`);
      add("Library database", `${up}subsystems/database.md`);
    }
    if (segs[0] === "server") {
      add("Packaging", `${up}operations/packaging.md`);
      add("Cross-device sync", `${up}subsystems/sync.md`);
      add("Data directories", `${up}operations/data-dirs.md`);
    }
  }
  if (crateName === "pdf-folio-style") {
    add("Style system", `${up}subsystems/style-system.md`);
    add("Development (style iteration)", `${up}operations/development.md#style-iteration`);
    add("Data directories", `${up}operations/data-dirs.md`);
  }
  if (crateName === "pdf-folio-main") {
    add("CLI reference", `${up}operations/cli.md`);
    add("Development", `${up}operations/development.md`);
  }
  if (crateName === "iced-widget-patch") {
    add("iced-widget-patch guide", `${up}crates/iced-patch.md`);
    add("Workspace map", `${up}architecture/workspace.md`);
  }

  // Always link crate guide when we have one
  const crateGuide = {
    "pdf-folio-core": "crates/core.md",
    "pdf-folio-ui": "crates/ui.md",
    "pdf-folio-cloud": "crates/cloud.md",
    "pdf-folio-style": "crates/style.md",
    "pdf-folio-main": "crates/main.md",
    "iced-widget-patch": "crates/iced-patch.md",
  }[crateName];
  if (crateGuide) add("Crate guide", `${up}${crateGuide}`);

  return links;
}

// ─── main ───────────────────────────────────────────────────────────────────

function extract() {
  console.log("Extracting rustdoc → content/api/ …");

  // Clean previous generated API pages (keep structure)
  if (existsSync(OUT)) {
    rmSync(OUT, { recursive: true, force: true });
  }
  ensureDir(OUT);

  const modules = []; // { crateName, segs, moduleName, ... }

  const crateDirs = readdirSync(CRATES).filter((n) =>
    statSync(join(CRATES, n)).isDirectory()
  );

  for (const crateName of crateDirs) {
    const src = join(CRATES, crateName, "src");
    if (!existsSync(src)) continue;

    const files = walk(src).filter((f) => f.endsWith(".rs"));
    for (const abs of files) {
      const base = basename(abs);
      if (base === "tests.rs") continue;
      // skip nested tests modules files named tests under subdirs — still parse
      if (abs.includes(`${sep}tests${sep}`)) continue;

      const segs = modulePathFromFile(crateName, abs);
      // Skip pure test file modules like foo/tests.rs already handled
      if (segs[segs.length - 1] === "tests") continue;

      const parsed = parseRustFile(abs);
      const sourceRel = relative(REPO, abs).split(sep).join("/");
      const moduleName = displayModuleName(crateName, segs);

      modules.push({
        crateName,
        segs,
        moduleName,
        sourceRel,
        abs,
        ...parsed,
      });
    }
  }

  // Index by crate + path key
  const byKey = new Map();
  for (const m of modules) {
    byKey.set(`${m.crateName}::${m.segs.join("::")}`, m);
  }

  const linkIndex = buildLinkIndex(modules);

  // Children relationships
  function childrenOf(crateName, segs) {
    return modules.filter((m) => {
      if (m.crateName !== crateName) return false;
      if (m.segs.length !== segs.length + 1) return false;
      return segs.every((s, i) => m.segs[i] === s);
    });
  }

  // Emit pages
  let pageCount = 0;
  for (const m of modules) {
    const children = childrenOf(m.crateName, m.segs).sort((a, b) =>
      a.moduleName.localeCompare(b.moduleName)
    );
    const md = renderModulePage({
      crateName: m.crateName,
      segs: m.segs,
      moduleName: m.moduleName,
      moduleDocs: m.moduleDocs,
      items: m.items,
      sourceRel: m.sourceRel,
      children,
      guideLinks: guideLinksFor(m.crateName, m.segs),
      linkIndex,
    });

    const outRel = pageOutRel(m.crateName, m.segs);
    const outAbs = join(__dirname, "content", outRel);
    ensureDir(dirname(outAbs));
    writeFileSync(outAbs, md, "utf8");
    pageCount++;
  }

  // Crate index pages are emitted as segs=[] from lib.rs/main.rs
  // Ensure each crate has an index even if only main.rs
  for (const crateName of crateDirs) {
    const src = join(CRATES, crateName, "src");
    if (!existsSync(src)) continue;
    const indexPath = join(OUT, crateName, "index.md");
    if (!existsSync(indexPath)) {
      // synthesize from first module
      const crateMods = modules.filter((m) => m.crateName === crateName);
      if (!crateMods.length) continue;
    }
  }

  // Top-level API index
  const orderedCrates = [
    ...CRATE_ORDER.filter((c) => crateDirs.includes(c)),
    ...crateDirs.filter((c) => !CRATE_ORDER.includes(c)).sort(),
  ];

  const indexLines = [];
  indexLines.push("---");
  indexLines.push("title: API Reference");
  indexLines.push("eyebrow: Rustdoc");
  indexLines.push(
    "lede: In-code rustdoc extracted from the workspace and rendered with this site's theme."
  );
  indexLines.push("order: 90");
  indexLines.push("---");
  indexLines.push("");
  indexLines.push(
    "These pages are generated from `//!` module docs and `///` item docs in the Rust sources. They are **not** a second rustdoc HTML tree — the same espresso theme, sidebar, and search apply."
  );
  indexLines.push("");
  indexLines.push(
    "Rebuild with `pnpm build` (runs `extract-rustdoc.mjs` then the site builder). Edit documentation in the `.rs` files; do not hand-edit `content/api/`."
  );
  indexLines.push("");
  indexLines.push("## Crates");
  indexLines.push("");
  indexLines.push('<div class="card-grid">');
  for (const crateName of orderedCrates) {
    const crateMods = modules.filter((m) => m.crateName === crateName);
    if (!crateMods.length) continue;
    const root =
      crateMods.find((m) => m.segs.length === 0) || crateMods[0];
    const blurb = CRATE_BLURBS[crateName] || "";
    const modCount = crateMods.length;
    const pubItems = crateMods.reduce(
      (n, m) => n + m.items.filter((i) => i.visibility === "pub").length,
      0
    );
    const pubDoc = crateMods.reduce(
      (n, m) =>
        n +
        m.items.filter((i) => i.visibility === "pub" && i.docs).length,
      0
    );
    indexLines.push(`  <a class="card-link" href="${crateName}/index.md">`);
    indexLines.push(`    <div class="card-title">${crateName}</div>`);
    indexLines.push(
      `    <p class="card-desc">${blurb || (root.moduleDocs || "").split("\n")[0] || ""}</p>`
    );
    indexLines.push(
      `    <div class="card-meta">${modCount} modules · ${pubDoc}/${pubItems} pub docs</div>`
    );
    indexLines.push(`  </a>`);
  }
  indexLines.push("</div>");
  indexLines.push("");
  indexLines.push("## How this relates to the guides");
  indexLines.push("");
  indexLines.push(
    "Narrative architecture and design notes live under [Architecture](../architecture/overview.md), [Crates](../crates/core.md), and [Subsystems](../subsystems/rendering.md). API pages list what the code exports and what rustdoc says about each item. Prefer linking from guides to API modules when pointing maintainers at a concrete type or function."
  );
  indexLines.push("");
  indexLines.push("## Coverage summary");
  indexLines.push("");
  indexLines.push("| Crate | Modules | Pub items | Pub with docs |");
  indexLines.push("| --- | ---: | ---: | ---: |");
  for (const crateName of orderedCrates) {
    const crateMods = modules.filter((m) => m.crateName === crateName);
    if (!crateMods.length) continue;
    const pubItems = crateMods.reduce(
      (n, m) => n + m.items.filter((i) => i.visibility === "pub").length,
      0
    );
    const pubDoc = crateMods.reduce(
      (n, m) =>
        n +
        m.items.filter((i) => i.visibility === "pub" && i.docs).length,
      0
    );
    indexLines.push(
      `| [${crateName}](${crateName}/index.md) | ${crateMods.length} | ${pubItems} | ${pubDoc} |`
    );
  }
  indexLines.push("");

  writeFileSync(join(OUT, "index.md"), indexLines.join("\n"), "utf8");
  pageCount++;

  // Emit nav fragment helper as JSON for build.mjs / manual merge
  const navApi = {
    label: "API Reference",
    collapsed: false,
    items: [
      { title: "API Home", path: "api/index.md" },
      ...orderedCrates
        .filter((c) => modules.some((m) => m.crateName === c))
        .map((crateName) => {
          const crateMods = modules
            .filter((m) => m.crateName === crateName)
            .sort((a, b) => a.segs.join("/").localeCompare(b.segs.join("/")));
          const rootPath = `api/${crateName}/index.md`;
          // Flatten important first-level modules as children anchors aren't enough — list as nested items in a flat nav: only root + top-level modules
          const top = crateMods.filter((m) => m.segs.length === 1);
          return {
            title: crateName,
            path: rootPath,
            children: top.map((m) => ({
              title: m.segs[0],
              // use page path via special field — build.mjs only supports anchors on same page
              // So we list top-level modules as separate nav items instead
            })),
          };
        }),
    ],
  };

  // Sidebar: API home + crate roots. Submodules are linked from each crate page body.
  // (nav.json children only support in-page anchors, not nested routes.)
  const navGroupsItems = [{ title: "API Home", path: "api/index.md" }];
  for (const crateName of orderedCrates) {
    const crateMods = modules.filter((m) => m.crateName === crateName);
    if (!crateMods.length) continue;
    navGroupsItems.push({
      title: crateName,
      path: `api/${crateName}/index.md`,
    });
  }

  writeFileSync(
    join(OUT, "_nav_fragment.json"),
    JSON.stringify(
      {
        label: "API Reference",
        collapsed: false,
        items: navGroupsItems,
      },
      null,
      2
    ),
    "utf8"
  );

  // Stats
  let undocMods = modules.filter((m) => !m.moduleDocs).length;
  console.log(
    `  modules: ${modules.length}, pages: ${pageCount}, modules missing //! : ${undocMods}`
  );
  return { modules, pageCount, navGroupsItems };
}

const result = extract();
console.log("Done.");
export { extract, result };
