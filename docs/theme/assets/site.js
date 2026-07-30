/**
 * PDF-Folio docs client: sidebar, search, scrollspy, mobile nav.
 */
(() => {
  "use strict";

  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  // ── root-relative base (assets live next to pages under site/) ──────────
  function assetBase() {
    const script = document.querySelector('script[src*="site.js"]');
    if (!script) return "./";
    const src = script.getAttribute("src") || "";
    // e.g. ./assets/site.js or ../assets/site.js
    return src.replace(/assets\/site\.js.*$/, "");
  }

  // ── collapsible nav groups ──────────────────────────────────────────────
  function initSidebarCollapse() {
    $$(".nav-group-toggle").forEach((btn) => {
      btn.addEventListener("click", () => {
        const group = btn.closest(".nav-group");
        if (!group) return;
        const collapsed = group.classList.toggle("is-collapsed");
        btn.setAttribute("aria-expanded", collapsed ? "false" : "true");
        // Nav height changes when groups open/close — refresh left scrollbar
        window.dispatchEvent(new Event("pdf-folio-sidebar-resize"));
      });
    });
  }

  // ── Themed left scrollbar for the sidebar ───────────────────────────────
  const SIDEBAR_SCROLL_KEY = "pdf-folio-docs-sidebar-scroll";

  function readSidebarScroll() {
    try {
      const raw = sessionStorage.getItem(SIDEBAR_SCROLL_KEY);
      if (raw == null || raw === "") return null;
      const n = Number(raw);
      return Number.isFinite(n) && n >= 0 ? n : null;
    } catch {
      return null;
    }
  }

  function writeSidebarScroll(y) {
    try {
      sessionStorage.setItem(SIDEBAR_SCROLL_KEY, String(Math.max(0, y | 0)));
    } catch {
      /* private mode / quota */
    }
  }

  function initSidebarScrollbar() {
    const sidebar = $("#sidebar");
    if (!sidebar || sidebar.dataset.vscroll === "1") return;
    sidebar.dataset.vscroll = "1";

    // Move existing children into a scrollable body; bar sits on the left.
    const body = document.createElement("div");
    body.className = "sidebar-body";
    while (sidebar.firstChild) {
      body.appendChild(sidebar.firstChild);
    }

    const vscroll = document.createElement("div");
    vscroll.className = "sidebar-vscroll";
    vscroll.setAttribute("aria-hidden", "true");
    const track = document.createElement("div");
    track.className = "sidebar-vscroll-track";
    const thumb = document.createElement("div");
    thumb.className = "sidebar-vscroll-thumb";
    vscroll.appendChild(track);
    vscroll.appendChild(thumb);

    sidebar.appendChild(vscroll);
    sidebar.appendChild(body);

    // Restore scroll before first paint of the thumb so navigation feels sticky.
    const saved = readSidebarScroll();
    if (saved != null) {
      body.scrollTop = saved;
    }

    const trackMetrics = () => {
      const style = getComputedStyle(track);
      const top = parseFloat(style.top) || 10;
      const bottom = parseFloat(style.bottom) || 10;
      const trackH = Math.max(0, vscroll.clientHeight - top - bottom);
      return { top, trackH };
    };

    const update = () => {
      const maxScroll = body.scrollHeight - body.clientHeight;
      const scrollable = maxScroll > 2;
      sidebar.classList.toggle("is-scrollable", scrollable);

      // Clamp restored position if this page's nav is shorter.
      if (body.scrollTop > maxScroll) {
        body.scrollTop = Math.max(0, maxScroll);
      }

      const { top, trackH } = trackMetrics();
      if (!scrollable || trackH <= 0) {
        thumb.style.height = `${Math.max(trackH, 0)}px`;
        thumb.style.transform = `translateY(0)`;
        return;
      }

      const ratio = body.clientHeight / body.scrollHeight;
      const thumbH = Math.max(28, Math.round(trackH * ratio));
      const maxTravel = Math.max(0, trackH - thumbH);
      const y = maxScroll > 0 ? (body.scrollTop / maxScroll) * maxTravel : 0;
      thumb.style.height = `${thumbH}px`;
      // thumb is positioned at track top; translate within the track
      thumb.style.top = `${top}px`;
      thumb.style.transform = `translateY(${y}px)`;
    };

    let persistTimer = 0;
    const persistScroll = () => {
      writeSidebarScroll(body.scrollTop);
    };
    const persistScrollSoon = () => {
      // Coalesce rapid scroll events; still flush on navigation below.
      if (persistTimer) return;
      persistTimer = window.setTimeout(() => {
        persistTimer = 0;
        persistScroll();
      }, 50);
    };

    body.addEventListener(
      "scroll",
      () => {
        update();
        persistScrollSoon();
      },
      { passive: true }
    );
    window.addEventListener("resize", update, { passive: true });
    window.addEventListener("pdf-folio-sidebar-resize", update);

    // Flush on leave so the next page always sees the latest offset.
    window.addEventListener("pagehide", persistScroll);
    window.addEventListener("beforeunload", persistScroll);
    // Sidebar links: save immediately before the document unloads.
    body.querySelectorAll("a[href]").forEach((a) => {
      a.addEventListener("click", persistScroll, { capture: true });
    });

    // Drag thumb
    let dragging = false;
    let startY = 0;
    let startScroll = 0;

    thumb.addEventListener("pointerdown", (e) => {
      dragging = true;
      sidebar.classList.add("is-dragging");
      startY = e.clientY;
      startScroll = body.scrollTop;
      thumb.setPointerCapture?.(e.pointerId);
      e.preventDefault();
      e.stopPropagation();
    });

    const onMove = (clientY) => {
      if (!dragging) return;
      const maxScroll = body.scrollHeight - body.clientHeight;
      if (maxScroll <= 0) return;
      const { trackH } = trackMetrics();
      const thumbH = thumb.offsetHeight;
      const maxTravel = Math.max(0, trackH - thumbH);
      if (maxTravel <= 0) return;
      const dy = clientY - startY;
      body.scrollTop = Math.max(
        0,
        Math.min(maxScroll, startScroll + (dy / maxTravel) * maxScroll)
      );
    };

    thumb.addEventListener("pointermove", (e) => onMove(e.clientY));
    const endDrag = () => {
      dragging = false;
      sidebar.classList.remove("is-dragging");
    };
    thumb.addEventListener("pointerup", endDrag);
    thumb.addEventListener("pointercancel", endDrag);

    // Click track to jump
    vscroll.addEventListener("pointerdown", (e) => {
      if (e.target === thumb) return;
      const maxScroll = body.scrollHeight - body.clientHeight;
      if (maxScroll <= 0) return;
      const { top, trackH } = trackMetrics();
      const rect = vscroll.getBoundingClientRect();
      const thumbH = thumb.offsetHeight;
      const maxTravel = Math.max(0, trackH - thumbH);
      const clickY = e.clientY - rect.top - top - thumbH / 2;
      const t = maxTravel > 0 ? clickY / maxTravel : 0;
      body.scrollTop = Math.max(0, Math.min(maxScroll, t * maxScroll));
      update();
    });

    if (typeof ResizeObserver !== "undefined") {
      const ro = new ResizeObserver(update);
      ro.observe(body);
      ro.observe(sidebar);
    }

    requestAnimationFrame(update);
    setTimeout(update, 50);
    setTimeout(update, 250);
  }

  // ── mobile sidebar ──────────────────────────────────────────────────────
  function initMobileNav() {
    const btn = $("#menuBtn");
    const sidebar = $("#sidebar");
    const backdrop = $("#sidebarBackdrop");
    if (!btn || !sidebar) return;

    const setOpen = (open) => {
      sidebar.classList.toggle("is-open", open);
      backdrop?.classList.toggle("is-open", open);
      if (backdrop) backdrop.hidden = !open;
      btn.setAttribute("aria-expanded", open ? "true" : "false");
      document.body.style.overflow = open ? "hidden" : "";
    };

    btn.addEventListener("click", () => {
      setOpen(!sidebar.classList.contains("is-open"));
    });
    backdrop?.addEventListener("click", () => setOpen(false));
    sidebar.querySelectorAll("a").forEach((a) => {
      a.addEventListener("click", () => setOpen(false));
    });
    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && sidebar.classList.contains("is-open")) {
        setOpen(false);
      }
    });
  }

  // ── scrollspy for page TOC + sidebar sublinks ──────────────────────────
  function initScrollspy() {
    const tocLinks = $$(".page-toc-link");
    const subLinks = $$(".nav-item.is-active .nav-link.sub");
    const links = [...tocLinks, ...subLinks];
    if (!links.length) return;

    const targets = links
      .map((a) => {
        const href = a.getAttribute("href") || "";
        const id = href.includes("#") ? href.split("#").pop() : null;
        if (!id) return null;
        const el = document.getElementById(id);
        return el ? { a, el, id } : null;
      })
      .filter(Boolean);

    if (!targets.length) return;

    // Deduplicate by element for observation
    const byId = new Map();
    targets.forEach((t) => {
      if (!byId.has(t.id)) byId.set(t.id, []);
      byId.get(t.id).push(t.a);
    });

    const setActive = (id) => {
      links.forEach((a) => a.classList.remove("active"));
      (byId.get(id) || []).forEach((a) => a.classList.add("active"));
    };

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) setActive(entry.target.id);
        });
      },
      { rootMargin: "-12% 0px -72% 0px", threshold: 0 }
    );

    byId.forEach((_, id) => {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    });
  }

  // ── full-site search ────────────────────────────────────────────────────
  let searchIndex = null;
  let searchPromise = null;

  async function loadIndex() {
    if (searchIndex) return searchIndex;
    if (searchPromise) return searchPromise;
    const base = assetBase();
    searchPromise = fetch(base + "search-index.json")
      .then((r) => {
        if (!r.ok) throw new Error("search index missing");
        return r.json();
      })
      .then((data) => {
        searchIndex = data;
        return data;
      })
      .catch((err) => {
        console.warn("Search index failed to load:", err);
        searchIndex = [];
        return searchIndex;
      });
    return searchPromise;
  }

  function tokenize(q) {
    return q
      .toLowerCase()
      .split(/[^a-z0-9_./+-]+/)
      .filter((t) => t.length > 1);
  }

  function scoreDoc(doc, tokens) {
    if (!tokens.length) return 0;
    const title = (doc.title || "").toLowerCase();
    const text = (doc.text || "").toLowerCase();
    const eyebrow = (doc.eyebrow || "").toLowerCase();
    let score = 0;
    let matched = 0;

    for (const t of tokens) {
      let hit = false;
      if (title === t) {
        score += 40;
        hit = true;
      } else if (title.startsWith(t)) {
        score += 28;
        hit = true;
      } else if (title.includes(t)) {
        score += 18;
        hit = true;
      }
      if (eyebrow.includes(t)) {
        score += 6;
        hit = true;
      }
      if (text.includes(t)) {
        score += 4;
        hit = true;
        // slight boost for multiple occurrences
        const c = text.split(t).length - 1;
        score += Math.min(c, 5);
      }
      // heading match
      for (const h of doc.headings || []) {
        const ht = (h.text || "").toLowerCase();
        if (ht.includes(t)) {
          score += 12;
          hit = true;
        }
      }
      if (hit) matched++;
    }

    // Prefer docs that match all tokens
    if (matched < tokens.length) score *= matched / tokens.length;
    // Prefer full pages over section stubs when scores close
    if (doc.section) score *= 0.85;
    return score;
  }

  function snippetAround(text, tokens, maxLen = 110) {
    if (!text) return "";
    const lower = text.toLowerCase();
    let idx = -1;
    let term = "";
    for (const t of tokens) {
      const i = lower.indexOf(t);
      if (i >= 0 && (idx < 0 || i < idx)) {
        idx = i;
        term = t;
      }
    }
    if (idx < 0) return text.slice(0, maxLen) + (text.length > maxLen ? "…" : "");
    const start = Math.max(0, idx - 36);
    const end = Math.min(text.length, start + maxLen);
    let snip =
      (start > 0 ? "…" : "") +
      text.slice(start, end) +
      (end < text.length ? "…" : "");
    return snip;
  }

  function highlight(text, tokens) {
    if (!text || !tokens.length) return escape(text);
    let out = escape(text);
    for (const t of tokens) {
      const re = new RegExp(
        `(${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`,
        "gi"
      );
      out = out.replace(re, "<mark>$1</mark>");
    }
    return out;
  }

  function escape(s) {
    return String(s || "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function resolveUrl(url) {
    // url is like "guide/foo.html" or "index.html#x" relative to site root
    const base = assetBase();
    return base + url;
  }

  function initSearch() {
    const modal = $("#searchModal");
    const input = $("#searchInput");
    const results = $("#searchResults");
    const empty = $("#searchEmpty");
    const hint = $("#searchHint");
    const triggers = [$$("#searchTrigger"), $$("[data-search-open]")].flat();
    if (!modal || !input || !results) return;

    let activeIdx = -1;
    let currentHits = [];

    const open = async () => {
      modal.hidden = false;
      document.body.style.overflow = "hidden";
      input.value = "";
      results.innerHTML = "";
      empty.hidden = true;
      if (hint) hint.hidden = false;
      activeIdx = -1;
      currentHits = [];
      input.focus();
      loadIndex();
    };

    const close = () => {
      modal.hidden = true;
      document.body.style.overflow = "";
    };

    triggers.forEach((el) => el?.addEventListener("click", open));

    modal.addEventListener("click", (e) => {
      if (e.target === modal) close();
    });

    const render = (hits, tokens) => {
      results.innerHTML = "";
      currentHits = hits;
      activeIdx = hits.length ? 0 : -1;

      if (!tokens.length) {
        empty.hidden = true;
        if (hint) hint.hidden = false;
        return;
      }
      if (hint) hint.hidden = true;

      if (!hits.length) {
        empty.hidden = false;
        return;
      }
      empty.hidden = true;

      hits.forEach((doc, i) => {
        const a = document.createElement("a");
        a.className = "search-hit" + (i === 0 ? " is-active" : "");
        a.href = resolveUrl(doc.url);
        a.setAttribute("role", "option");
        a.dataset.idx = String(i);

        const eye = doc.eyebrow
          ? `<div class="search-hit-eyebrow">${escape(doc.eyebrow)}</div>`
          : "";
        const snip = snippetAround(doc.text, tokens);
        a.innerHTML =
          eye +
          `<div class="search-hit-title">${highlight(doc.title, tokens)}</div>` +
          (snip
            ? `<div class="search-hit-snippet">${highlight(snip, tokens)}</div>`
            : "");
        results.appendChild(a);
      });
    };

    const setActive = (idx) => {
      const items = $$(".search-hit", results);
      if (!items.length) return;
      activeIdx = ((idx % items.length) + items.length) % items.length;
      items.forEach((el, i) => el.classList.toggle("is-active", i === activeIdx));
      items[activeIdx].scrollIntoView({ block: "nearest" });
    };

    let debounce = null;
    input.addEventListener("input", () => {
      clearTimeout(debounce);
      debounce = setTimeout(async () => {
        const q = input.value.trim();
        const tokens = tokenize(q);
        if (!tokens.length) {
          render([], []);
          return;
        }
        const index = await loadIndex();
        const scored = index
          .map((doc) => ({ doc, score: scoreDoc(doc, tokens) }))
          .filter((x) => x.score > 0)
          .sort((a, b) => b.score - a.score)
          .slice(0, 24)
          .map((x) => x.doc);

        // Dedupe: prefer section hits only if not already covering same page top
        const seen = new Set();
        const deduped = [];
        for (const d of scored) {
          const key = d.url.split("#")[0] + (d.section ? "#" + (d.url.split("#")[1] || "") : "");
          // Allow one page-level + a few section hits
          if (seen.has(d.url)) continue;
          seen.add(d.url);
          deduped.push(d);
          if (deduped.length >= 12) break;
        }
        render(deduped, tokens);
      }, 40);
    });

    input.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActive(activeIdx + 1);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setActive(activeIdx - 1);
      } else if (e.key === "Enter") {
        e.preventDefault();
        const items = $$(".search-hit", results);
        if (activeIdx >= 0 && items[activeIdx]) {
          window.location.href = items[activeIdx].href;
        }
      } else if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    });

    // Global shortcuts: / or Ctrl/Cmd+K
    window.addEventListener("keydown", (e) => {
      const tag = (e.target && e.target.tagName) || "";
      const editable =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        e.target?.isContentEditable;

      if (
        (e.key === "k" || e.key === "K") &&
        (e.metaKey || e.ctrlKey)
      ) {
        e.preventDefault();
        if (modal.hidden) open();
        else close();
        return;
      }
      if (e.key === "/" && !editable && modal.hidden) {
        e.preventDefault();
        open();
      }
      if (e.key === "Escape" && !modal.hidden) {
        close();
      }
    });
  }

  // ── Monokai-inspired syntax highlighting (zero deps) ────────────────────
  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  /**
   * Canonical name-token roles (must match CSS .tok-* and extract-rustdoc):
   *   tok-k      keywords (pub, mod, fn, const, …)
   *   tok-f      function names & calls                    — orange
   *   tok-const  const / static / SCREAMING_SNAKE          — purple
   *   tok-m      module / use path segments                — green
   *   tok-t      struct / enum / type / trait / PascalCase — cyan
   */
  function isScreamingSnake(s) {
    return /^[A-Z][A-Z0-9_]*$/.test(s) && s.length > 1;
  }
  function isPascalTypeName(s) {
    return /^[A-Z][A-Za-z0-9]*$/.test(s) && /[a-z]/.test(s);
  }
  function tokenClassForKind(kind) {
    switch (kind) {
      case "fn":
        return "tok-f";
      case "const":
      case "static":
        return "tok-const";
      case "mod":
      case "use":
        return "tok-m";
      case "struct":
      case "enum":
      case "type":
      case "trait":
      case "impl":
        return "tok-t";
      default:
        return "tok-f";
    }
  }

  function highlightRust(src, opts = {}) {
    // Stateful scan so `mod name` / `struct Name` / `fn name` get correct roles.
    // `quiet` (tables): only keywords + types + path separators; no green path spam.
    const quiet = !!opts.quiet;
    const KEYWORDS = new Set([
      "as", "async", "await", "break", "const", "continue", "crate", "dyn",
      "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
      "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
      "self", "Self", "static", "struct", "super", "trait", "true", "type",
      "unsafe", "use", "where", "while", "yield", "box", "try",
    ]);
    const BUILTINS = new Set([
      "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize",
      "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "String",
      "PathBuf", "Path", "Vec", "Option", "Result", "Ok", "Err", "Some",
      "None", "Box", "Arc", "Rc", "HashMap", "HashSet", "BTreeMap",
      "BTreeSet", "Cow", "Duration", "Instant", "Task", "Message",
      "Element", "Color", "Length",
    ]);
    const DEF_KW = new Set([
      "mod", "struct", "enum", "trait", "type", "fn", "const", "static",
      "use", "impl",
    ]);

    // Hyphen allowed in idents so crate names like pdf-folio-core stay whole.
    const re = new RegExp(
      [
        "(//[^\\n]*|/\\*[\\s\\S]*?\\*/)",
        '(r#*"(?:\\\\.|[^"\\\\])*"#*|b?"(?:\\\\.|[^"\\\\])*"|\'(?:\\\\.|[^\'\\\\])\')',
        "\\b(0x[0-9a-fA-F_]+|0b[01_]+|0o[0-7_]+|\\d[\\d_]*(?:\\.\\d[\\d_]*)?(?:[eE][+-]?\\d[\\d_]*)?)\\b",
        "('[a-z_][A-Za-z0-9_]*)",
        "(#\\[|#\\!\\[)",
        "\\b([A-Za-z_][A-Za-z0-9_-]*)\\b",
        "(::|->|=>)",
        "([{}()\\[\\];,.<>:=|&+*/%!?^~@\\\\]+)",
        "(-)",
        "(\\s+)",
        "(.)",
      ].join("|"),
      "g"
    );

    let out = "";
    let m;
    /** @type {string|null} */
    let afterDefKw = null;
    let expectModPath = false;

    while ((m = re.exec(src)) !== null) {
      const full = m[0];
      const comment = m[1];
      const string = m[2];
      const number = m[3];
      const lifetime = m[4];
      const attr = m[5];
      const ident = m[6];
      const multiOp = m[7];
      const punct = m[8];
      const hyphen = m[9];
      const space = m[10];
      const other = m[11];

      if (comment != null) {
        out += `<span class="tok-c">${escapeHtml(comment)}</span>`;
        afterDefKw = null;
      } else if (string != null) {
        out += `<span class="tok-s">${escapeHtml(string)}</span>`;
        afterDefKw = null;
      } else if (number != null) {
        out += `<span class="tok-n">${escapeHtml(number)}</span>`;
        afterDefKw = null;
      } else if (lifetime != null) {
        out += `<span class="tok-a">${escapeHtml(lifetime)}</span>`;
      } else if (attr != null) {
        out += `<span class="tok-a">${escapeHtml(attr)}</span>`;
        afterDefKw = null;
      } else if (ident != null) {
        if (afterDefKw === "mod") {
          out += quiet
            ? escapeHtml(ident)
            : `<span class="tok-m">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
          expectModPath = false;
        } else if (afterDefKw === "fn") {
          // Function definition name — orange
          out += `<span class="tok-f">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else if (afterDefKw === "const" || afterDefKw === "static") {
          // Const / static definition name — purple
          out += `<span class="tok-const">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else if (
          afterDefKw === "struct" ||
          afterDefKw === "enum" ||
          afterDefKw === "trait" ||
          afterDefKw === "type" ||
          afterDefKw === "impl"
        ) {
          out += `<span class="tok-t">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else if (afterDefKw === "use" || expectModPath) {
          if (BUILTINS.has(ident) || isPascalTypeName(ident)) {
            out += `<span class="tok-t">${escapeHtml(ident)}</span>`;
          } else if (isScreamingSnake(ident)) {
            out += `<span class="tok-const">${escapeHtml(ident)}</span>`;
          } else if (quiet) {
            out += escapeHtml(ident);
          } else {
            out += `<span class="tok-m">${escapeHtml(ident)}</span>`;
          }
          afterDefKw = null;
          expectModPath = true;
        } else if (KEYWORDS.has(ident)) {
          out += `<span class="tok-k">${escapeHtml(ident)}</span>`;
          if (DEF_KW.has(ident)) {
            afterDefKw = ident;
            expectModPath = ident === "use";
          } else {
            afterDefKw = null;
          }
        } else if (BUILTINS.has(ident)) {
          out += `<span class="tok-t">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else if (isScreamingSnake(ident)) {
          // CONST_NAMES — purple
          out += `<span class="tok-const">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else if (isPascalTypeName(ident) || /^[A-Z]/.test(ident)) {
          out += `<span class="tok-t">${escapeHtml(ident)}</span>`;
          afterDefKw = null;
        } else {
          const rest = src.slice(m.index + full.length);
          const call = /^\s*(?:!|\()/.test(rest);
          // Calls stay orange (function role); bare idents inherit body color
          if (call) out += `<span class="tok-f">${escapeHtml(ident)}</span>`;
          else out += escapeHtml(ident);
          afterDefKw = null;
        }
      } else if (multiOp != null) {
        out += `<span class="tok-p">${escapeHtml(multiOp)}</span>`;
        if (multiOp === "::") expectModPath = true;
        else {
          expectModPath = false;
          afterDefKw = null;
        }
      } else if (punct != null) {
        out += `<span class="tok-p">${escapeHtml(punct)}</span>`;
        if (/^[,;{}()=]/.test(punct) || punct === ":") {
          expectModPath = false;
          afterDefKw = null;
        }
      } else if (hyphen != null) {
        // Standalone minus (not part of kebab ident)
        out += `<span class="tok-p">${escapeHtml(hyphen)}</span>`;
      } else if (space != null) {
        out += space;
      } else {
        out += escapeHtml(other || full);
        afterDefKw = null;
        expectModPath = false;
      }
    }
    return out;
  }

  function highlightShell(src) {
    const re =
      /(#[^\n]*)|('(?:\\.|[^'])*'|"(?:\\.|[^"\\])*")|(\$\w+|\$\{[^}]+\})|\b(if|then|else|elif|fi|for|while|do|done|case|esac|in|function|return|export|local|source|cd|echo|cargo|pnpm|node|rustc|git)\b|([|&;<>()\[\]{}=]+)|(\s+)|([^\s#'"$|&;<>()\[\]{}=]+)/g;
    let out = "";
    let m;
    while ((m = re.exec(src)) !== null) {
      const [, comment, string, variable, keyword, punct, space, word] = m;
      if (comment != null) out += `<span class="tok-c">${escapeHtml(comment)}</span>`;
      else if (string != null) out += `<span class="tok-s">${escapeHtml(string)}</span>`;
      else if (variable != null) out += `<span class="tok-n">${escapeHtml(variable)}</span>`;
      else if (keyword != null) out += `<span class="tok-k">${escapeHtml(keyword)}</span>`;
      else if (punct != null) out += `<span class="tok-p">${escapeHtml(punct)}</span>`;
      else if (space != null) out += space;
      else if (word != null) {
        // flags
        if (word.startsWith("-")) out += `<span class="tok-a">${escapeHtml(word)}</span>`;
        else out += escapeHtml(word);
      }
    }
    return out;
  }

  function highlightGeneric(src) {
    // Lightweight: comments + strings + numbers for kdl/text/toml-ish blocks
    const re =
      /(\/\/[^\n]*|#[^\n]*|\/\*[\s\S]*?\*\/)|("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|\b(\d[\d_]*(?:\.\d+)?)\b|([{}()\[\];,.:=<>|&+\-*/%!]+)|(\s+)|([^\s"'#\/{}()\[\];,.:=<>|&+\-*/%!]+)/g;
    let out = "";
    let m;
    while ((m = re.exec(src)) !== null) {
      const [, comment, string, number, punct, space, word] = m;
      if (comment != null) out += `<span class="tok-c">${escapeHtml(comment)}</span>`;
      else if (string != null) out += `<span class="tok-s">${escapeHtml(string)}</span>`;
      else if (number != null) out += `<span class="tok-n">${escapeHtml(number)}</span>`;
      else if (punct != null) out += `<span class="tok-p">${escapeHtml(punct)}</span>`;
      else if (space != null) out += space;
      else if (word != null) {
        if (/^(true|false|null|component|theme|color|normal|hovered|selected)$/i.test(word)) {
          out += `<span class="tok-k">${escapeHtml(word)}</span>`;
        } else if (/^[A-Z]/.test(word)) {
          out += `<span class="tok-t">${escapeHtml(word)}</span>`;
        } else {
          out += escapeHtml(word);
        }
      }
    }
    return out;
  }

  function langOf(el) {
    const cls = el.className || "";
    const m = cls.match(/language-([\w+-]+)/);
    return (m && m[1].toLowerCase()) || "";
  }

  /** Filesystem / crate path — only dim `/` separators; keep names+extensions whole. */
  function highlightPathQuiet(s) {
    // Don't split on `.` — extensions stay with the filename (lib.rs, Cargo.toml).
    return s
      .split(/(\/)/)
      .map((part) => {
        if (part === "/") return `<span class="tok-p">${escapeHtml(part)}</span>`;
        if (!part) return "";
        return escapeHtml(part);
      })
      .join("");
  }

  /** Guess highlighter for a short inline `code` snippet. */
  function highlightInlineSnippet(src, { quiet = false } = {}) {
    const s = src.trim();
    if (!s) return null;

    // Shell / env / CLI-ish
    if (
      /^[~$]/.test(s) ||
      /^export\s|^cd\s|^cargo\s|^pnpm\s|^RUST_|^PDF_FOLIO_/.test(s) ||
      (/^--?[a-z]/.test(s) && !/::/.test(s))
    ) {
      return highlightShell(s);
    }

    // Filesystem / crate paths (not Rust paths with ::)
    if (
      (/^(\.\.?\/|\/|[A-Za-z]:\\)/.test(s) ||
        /^[\w.-]+\/[\w./-]+$/.test(s) ||
        /\.(rs|toml|md|kdl|json|sql|html|css|js)$/i.test(s)) &&
      !/::/.test(s) &&
      !/\b(fn|let|pub|struct|enum|mod|impl|use)\b/.test(s)
    ) {
      return highlightPathQuiet(s);
    }

    // Simple kebab/snake crate or module names — leave uncolored (inherit)
    if (/^[a-z][a-z0-9_-]*$/.test(s) && !KEYWORDS_INLINE.has(s)) {
      return escapeHtml(s);
    }

    return highlightRust(s, { quiet });
  }

  const KEYWORDS_INLINE = new Set([
    "as", "async", "await", "break", "const", "continue", "crate", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
    "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield", "box", "try",
  ]);

  /** Rust item kinds that appear in API "Kind" columns. */
  const RUST_KINDS = new Set([
    "mod", "fn", "struct", "enum", "type", "trait", "const", "static", "use", "impl",
  ]);

  function hasTokenSpans(el) {
    return !!(el && el.querySelector(".tok-k, .tok-t, .tok-f, .tok-const, .tok-m, .tok-s, .tok-a, .tok-n, .k, .s"));
  }

  /** Color a Name-column identifier using Kind — kind always wins over shape heuristics. */
  function highlightNameForKind(name, kind) {
    const t = name.trim();
    if (!t) return escapeHtml(t);
    if (t.includes("::")) return highlightRust(t, { quiet: false });
    if (/!$/.test(t)) return `<span class="tok-f">${escapeHtml(t)}</span>`;
    if (isScreamingSnake(t) && (kind === "const" || kind === "static" || !kind)) {
      return `<span class="tok-const">${escapeHtml(t)}</span>`;
    }
    const cls = tokenClassForKind(kind);
    return `<span class="${cls}">${escapeHtml(t)}</span>`;
  }

  function applyCodeHighlight(code, { quiet = false, forceKind = null } = {}) {
    const src = code.textContent || "";
    const t = src.trim();
    if (!t) return;

    if (code.classList.contains("api-vis")) {
      // visibility values like pub / pub(crate)
      code.innerHTML = highlightRust(t, { quiet: false });
      code.dataset.highlighted = "1";
      code.classList.add("code-inline-hl", "kind-token");
      return;
    }

    // Kind / single keyword
    if (RUST_KINDS.has(t) || (KEYWORDS_INLINE.has(t) && !t.includes("_") && t.length <= 12)) {
      code.innerHTML = `<span class="tok-k">${escapeHtml(t)}</span>`;
      code.dataset.highlighted = "1";
      code.classList.add("code-inline-hl", "kind-token");
      return;
    }

    // Name column with known kind from same row
    if (forceKind && RUST_KINDS.has(forceKind)) {
      code.innerHTML = highlightNameForKind(t, forceKind);
      code.dataset.highlighted = "1";
      code.classList.add("code-inline-hl");
      return;
    }

    const html = highlightInlineSnippet(src, { quiet });
    if (html == null || html === escapeHtml(t)) {
      if (/!$/.test(t)) {
        code.innerHTML = `<span class="tok-f">${escapeHtml(t)}</span>`;
      } else if (isScreamingSnake(t)) {
        code.innerHTML = `<span class="tok-const">${escapeHtml(t)}</span>`;
      } else if (isPascalTypeName(t) || (/^[A-Z]/.test(t) && !isScreamingSnake(t))) {
        code.innerHTML = `<span class="tok-t">${escapeHtml(t)}</span>`;
      } else {
        code.textContent = t;
      }
    } else {
      code.innerHTML = html;
    }
    code.dataset.highlighted = "1";
    code.classList.add("code-inline-hl");
  }

  function initInlineCodeHighlight() {
    // Pass 1: normal inline code (outside tables + first pass for tables)
    $$(".content code").forEach((code) => {
      if (code.closest("pre")) return;
      if (code.dataset.highlighted === "1") return;
      if (hasTokenSpans(code)) {
        code.dataset.highlighted = "1";
        code.classList.add("code-inline-hl");
        const only = (code.textContent || "").trim();
        if (RUST_KINDS.has(only) || KEYWORDS_INLINE.has(only)) {
          code.classList.add("kind-token");
        }
        return;
      }
      applyCodeHighlight(code, { quiet: !!code.closest("table") });
    });

    // Pass 2: per-table consistency
    // If any cell is syntax-highlighted, every <code> in that table must be too.
    // Name cells use Kind from the same row when available.
    $$(".content table").forEach((table) => {
      const headers = $$("th", table).map((th) =>
        (th.textContent || "").trim().toLowerCase()
      );
      const kindIdx = headers.indexOf("kind");
      const nameIdx = headers.indexOf("name");
      const visIdx = headers.findIndex(
        (h) => h === "visibility" || h === "vis"
      );

      // Wrap bare Kind cells
      if (kindIdx >= 0) {
        $$("tbody tr", table).forEach((tr) => {
          const cell = tr.cells[kindIdx];
          if (!cell || cell.querySelector("code")) return;
          const t = (cell.textContent || "").trim();
          if (!RUST_KINDS.has(t) && !KEYWORDS_INLINE.has(t)) return;
          cell.innerHTML = `<code class="code-inline-hl kind-token"><span class="tok-k">${escapeHtml(t)}</span></code>`;
        });
      }

      const tableHasHighlight =
        hasTokenSpans(table) ||
        kindIdx >= 0 ||
        visIdx >= 0 ||
        !!table.querySelector("code.kind-token, code .tok-k");

      if (!tableHasHighlight) return;

      $$("tbody tr", table).forEach((tr) => {
        const kindText =
          kindIdx >= 0 && tr.cells[kindIdx]
            ? (tr.cells[kindIdx].textContent || "").trim()
            : null;

        $$("code", tr).forEach((code) => {
          if (code.closest("pre")) return;

          // Re-apply so plain Name cells get tokens when Kind/Vis are highlighted
          const needs =
            !hasTokenSpans(code) ||
            (nameIdx >= 0 &&
              tr.cells[nameIdx] &&
              tr.cells[nameIdx].contains(code) &&
              kindText &&
              RUST_KINDS.has(kindText));

          if (!needs && hasTokenSpans(code)) return;

          // Reset and re-highlight
          const plain = code.textContent || "";
          code.textContent = plain;
          delete code.dataset.highlighted;
          code.classList.remove("code-inline-hl", "kind-token");

          const inNameCol =
            nameIdx >= 0 &&
            tr.cells[nameIdx] &&
            tr.cells[nameIdx].contains(code);

          applyCodeHighlight(code, {
            quiet: false,
            forceKind: inNameCol && kindText ? kindText : null,
          });
        });
      });
    });
  }

  function initSyntaxHighlight() {
    $$("pre code").forEach((code) => {
      if (code.dataset.highlighted === "1") return;
      // Already hand-highlighted (legacy spans)
      if (code.querySelector(".k, .s, .c, .tok-k, .tok-s")) {
        code.dataset.highlighted = "1";
        code.parentElement?.classList.add("has-highlight");
        return;
      }

      const lang = langOf(code);
      const src = code.textContent || "";
      let html = null;

      if (lang === "rust" || lang === "rs") html = highlightRust(src);
      else if (
        lang === "bash" ||
        lang === "sh" ||
        lang === "shell" ||
        lang === "zsh" ||
        lang === "console"
      ) {
        html = highlightShell(src);
      } else if (
        lang === "kdl" ||
        lang === "toml" ||
        lang === "json" ||
        lang === "text" ||
        lang === "txt" ||
        lang === ""
      ) {
        // Skip pure ASCII diagrams that are mostly box-drawing — keep plain
        const boxHeavy =
          (src.match(/[│─┌┐└┘├┤┬┴┼▼▲►◄]/g) || []).length > 4 ||
          (src.match(/[┃━┏┓┗┛]/g) || []).length > 2;
        if (!boxHeavy) html = highlightGeneric(src);
      }

      if (html != null) {
        code.innerHTML = html;
        code.dataset.highlighted = "1";
        code.parentElement?.classList.add("has-highlight", `lang-${lang || "plain"}`);
      }
    });

    // Diagram blocks (.diagram) with optional hand spans already work via CSS
    $$(".diagram").forEach((el) => el.classList.add("code-panel"));

    // In-prose `code` mentions (not inside pre)
    initInlineCodeHighlight();
  }

  // ── Custom thin scrollbar for overflowing code blocks ───────────────────
  function initCodeScrollbars() {
    const targets = [
      ...$$(".content pre"),
      ...$$(".content .diagram"),
    ];

    targets.forEach((el) => {
      if (el.closest(".code-scroll")) return;
      const parent = el.parentNode;
      if (!parent) return;

      const wrap = document.createElement("div");
      wrap.className = "code-scroll";
      parent.insertBefore(wrap, el);
      wrap.appendChild(el);

      const bar = document.createElement("div");
      bar.className = "code-scroll-bar";
      bar.setAttribute("aria-hidden", "true");
      const thumb = document.createElement("div");
      thumb.className = "code-scroll-thumb";
      bar.appendChild(thumb);
      wrap.appendChild(bar);

      const update = () => {
        const maxScroll = el.scrollWidth - el.clientWidth;
        const scrollable = maxScroll > 2;
        wrap.classList.toggle("is-scrollable", scrollable);
        if (!scrollable) {
          thumb.style.width = "100%";
          thumb.style.transform = "translateX(0)";
          return;
        }
        const ratio = el.clientWidth / el.scrollWidth;
        const thumbW = Math.max(28, Math.round(bar.clientWidth * ratio));
        const maxThumbTravel = Math.max(0, bar.clientWidth - thumbW);
        const x =
          maxScroll > 0
            ? (el.scrollLeft / maxScroll) * maxThumbTravel
            : 0;
        thumb.style.width = `${thumbW}px`;
        thumb.style.transform = `translateX(${x}px)`;
      };

      el.addEventListener("scroll", update, { passive: true });
      window.addEventListener("resize", update, { passive: true });

      // Recalculate after fonts/highlight layout
      requestAnimationFrame(update);
      setTimeout(update, 50);

      // Drag thumb to scroll
      let dragging = false;
      let startX = 0;
      let startScroll = 0;

      const onMove = (clientX) => {
        if (!dragging) return;
        const maxScroll = el.scrollWidth - el.clientWidth;
        if (maxScroll <= 0) return;
        const thumbW = thumb.offsetWidth;
        const maxThumbTravel = Math.max(0, bar.clientWidth - thumbW);
        if (maxThumbTravel <= 0) return;
        const dx = clientX - startX;
        const next =
          startScroll + (dx / maxThumbTravel) * maxScroll;
        el.scrollLeft = Math.max(0, Math.min(maxScroll, next));
      };

      thumb.addEventListener("pointerdown", (e) => {
        dragging = true;
        wrap.classList.add("is-dragging");
        startX = e.clientX;
        startScroll = el.scrollLeft;
        thumb.setPointerCapture?.(e.pointerId);
        e.preventDefault();
      });

      thumb.addEventListener("pointermove", (e) => {
        if (dragging) onMove(e.clientX);
      });

      const endDrag = () => {
        dragging = false;
        wrap.classList.remove("is-dragging");
      };
      thumb.addEventListener("pointerup", endDrag);
      thumb.addEventListener("pointercancel", endDrag);

      // Click track to jump
      bar.addEventListener("pointerdown", (e) => {
        if (e.target === thumb) return;
        const maxScroll = el.scrollWidth - el.clientWidth;
        if (maxScroll <= 0) return;
        const rect = bar.getBoundingClientRect();
        const thumbW = thumb.offsetWidth;
        const maxThumbTravel = Math.max(0, bar.clientWidth - thumbW);
        const clickX = e.clientX - rect.left - thumbW / 2;
        const t = maxThumbTravel > 0 ? clickX / maxThumbTravel : 0;
        el.scrollLeft = Math.max(0, Math.min(maxScroll, t * maxScroll));
        update();
      });

      // Observe size changes (highlight / font load)
      if (typeof ResizeObserver !== "undefined") {
        const ro = new ResizeObserver(update);
        ro.observe(el);
        ro.observe(bar);
      }
    });
  }

  // ── boot ────────────────────────────────────────────────────────────────
  document.addEventListener("DOMContentLoaded", () => {
    initSidebarScrollbar(); // wrap body first so collapse/mobile still find nodes
    initSidebarCollapse();
    initMobileNav();
    initScrollspy();
    initSearch();
    initSyntaxHighlight();
    initCodeScrollbars();
  });
})();
