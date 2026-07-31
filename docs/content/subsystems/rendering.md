---
title: Rendering Pipeline
eyebrow: Subsystems
lede: From bytes on disk to pixels on screen — PdfDoc, tile cache, zoom settle, and the text layer.
order: 20
---

<p class="trail"><strong>Trail</strong> <a href="../crates/core.md">core</a> <span class="sep">·</span> <a href="../crates/ui.md">ui</a> <span class="sep">·</span> <a href="../architecture/state.md">Runtime state</a> <span class="sep">·</span> <a href="search.md">Search</a> <span class="sep">·</span> <a href="../api/pdf-folio-core/pdf.md">API · pdf</a></p>

**Code:**  
- Core: `pdf-folio-core/src/pdf/`  
- UI: `pdf-folio-ui/src/viewer/`, `pdf-folio-ui/src/components/viewer/canvas.rs`

**API:** [pdf](../api/pdf-folio-core/pdf.md) · [document](../api/pdf-folio-core/pdf/document.md) · [renderer](../api/pdf-folio-core/pdf/renderer.md) · [viewer](../api/pdf-folio-ui/viewer.md)

## Opening a document

[`PdfDoc`](../api/pdf-folio-core/pdf/document.md) is a UI-agnostic wrapper around `pdfium-render`. Opening a file only stores path + page count — it does **not** keep Pdfium's document handle alive:

```rust
pub struct PdfDoc {
    path: PathBuf,
    page_count: u16,
}
```

Every later operation (`render_page`, `text_layer`, `outline`, metadata) reopens the file through an internal `with_document()` helper. Trade-off: repeated open cost for a `Clone`/`Send`-friendly handle that never holds a non-`Send` Pdfium object across `.await`.

Pdfium is bound **once per process** (`OnceLock`) and every call takes a process-wide mutex — the C API is not safely reentrant across threads even with separate document opens.

The UI never calls Pdfium on the main thread. It builds a task:

```text
viewer/tasks.rs: open_document_task
  spawn_blocking → PdfDoc::open
  → Message::DocumentOpened { path, doc: Arc<PdfDoc> }
  or Message::DocumentError
```

### Failure modes

| Failure | User-visible result |
| --- | --- |
| Missing Pdfium library | Document error; app keeps running |
| Corrupt / unreadable PDF | Document error message |
| Missing file path | Error or library `missing` flag depending on entry path |
| Password-protected (unsupported) | Error from open/render |

## Tiles and the render cache

[`render_page`](../api/pdf-folio-core/pdf/document.md) rasterizes one page at a target pixel width. Height follows the page aspect ratio.

```text
TileKey { page, width_px } → Arc<Vec<u8>> RGBA
```

A page at zoom A is a different tile than the same page at zoom B. [`TileCache`](../api/pdf-folio-core/pdf/renderer.md) is a thread-safe LRU (default 64, from `Settings::tile_cache_pages`). Hits skip Pdfium; misses schedule background render tasks that complete as `Message::PageRendered`.

Visible range computation and pending-render bookkeeping live in the viewer modules (`layout`, `rendering`, `state`). The canvas component draws whatever is already in `rendered_pages` / cache and shows placeholders while pending.

### Memory notes

- RGBA is 4 bytes × width × height per tile — large zooms are expensive.
- Eviction is LRU by tile key; jumping far in a long document reclaims earlier pages.
- Prefer sharing `Arc<Vec<u8>>` into iced images rather than copying buffers.

## Zoom without flicker

Continuous wheel zoom would thrash Pdfium, so zoom is two-phase:

1. **Interim** — GPU-scale the current tile (instant, slightly soft).
2. **Settled re-render** — after input quiets (~140 ms), re-rasterize at the new width.

Implementation pattern: bump a generation counter and schedule a delayed task. When `ZoomRenderSettled(generation)` arrives, drop it if the counter moved (stale). That prevents a burst of superseded renders from contending on the blocking pool.

`ZoomRenderPolicy` in `viewer/rendering.rs` encodes when to use preview width vs committed width. Toolbar, shortcuts, and Ctrl+wheel use **multiplicative** steps (`zoom_in_width` / `zoom_out_width`, ~1.12×) rather than fixed pixel jumps.

Newly arrived tiles that replace a zoom-preview fallback cross-fade for ~140 ms (`viewer_page_fade_ms`).

### Zoom presets

UI zoom controls map to width presets / percent labels (`ZoomPreset`, `zoom_percent` helpers). Changing spread mode (single vs two-page) recomputes layout but reuses tiles when widths match.

## Scroll ownership and wheel routing

Scroll offsets live on `ViewerRuntime` and are mirrored into iced’s scrollable via `scroll_viewer_to_offsets_task`. **Every programmatic pan** (keyboard arrows, page mode, jump, find reveal) must batch that task so the widget does not desync.

Wheel routing on the canvas (`components/viewer/canvas.rs`):

| Input | Handler |
| --- | --- |
| Ctrl + wheel | Cursor-anchored zoom (debounced re-render) |
| Page scroll mode | `scroll_page_mode_by(±1)` |
| Horizontal scroll mode | Horizontal pan + scrollable sync |
| Continuous vertical / wrapped | Left to iced scrollable (`ViewportChanged`) |

## Session and reading progress (debounced)

Continuous scrolling used to write session JSON every frame. That path is now generation-gated:

| Signal | Delay | Message |
| --- | --- | --- |
| Scroll / viewport motion | ~400 ms idle | `SessionSaveSettled` → `save_app_session_task` |
| Page change (library entry open) | ~500 ms idle | `ProgressSaveSettled` → `ProgressUpdated` → DB `last_page` |

Discrete actions (jump, zoom preset, open/close document, Esc → library) still save session immediately via `with_session_save` / `save_app_session_task`. Leaving the viewer **flushes** reading progress so `entries.last_page` is trustworthy for library filters and reopen.

`current_page()` for continuous modes prefers the page under the **viewport center** (better toolbar/progress than “first visible”).

## Text layer and outline

`text_layer(index)` returns character boxes as **normalized** fractions of page width/height (not PDF points or pixels). Selection highlights and find-in-document scale by multiplying by the current on-screen page size — no Pdfium re-query on every zoom.

`outline()` walks bookmarks into `OutlineNode { title, page, children }` pure trees for the TOC sidebar. The Contents UI:

- **Title click** → jump to the node’s page (when present)
- **Chevron** → expand/collapse children only
- **Scroll-spy** accents the deepest outline entry with `page ≤ current_page`

`text_on_page` extracts plain text strings used when building the Tantivy index (see [Search](search.md)).

### Find-in-document

Viewer find is separate from library Tantivy search:

| Feature | Scope | Engine |
| --- | --- | --- |
| Library search | All indexed pages in the vault | Tantivy |
| Find bar | Open document text layers | In-memory match over loaded layers |

Find loads text layers **progressively**: the current viewport (+ margin) first, then additional batches of ~8 pages via `ViewerFindTextLayersContinue` until the document is covered or find closes. The match counter shows a trailing `…` while layers are still pending. Compact option toggles: **All** / **Aa** / **á**. **F3** / **Shift+F3** move next/previous match.

Sidebar **thumbnails** only rasterize a window of pages around the current reading position (~12 on each side), so large documents do not schedule every thumbnail at once.

Text selection supports **double-click word** and **triple-click line** expand (`ViewerTextSelectionStarted.expand`).

## Layout geometry

`viewer/layout.rs` and `viewer/navigation.rs` compute:

- Page rectangles in scroll content space
- Visible page ranges for the viewport
- Prefetch order: visible pages first, then a deeper direction-aware margin (~4 pages)
- Click → page mapping for outline and thumbnails

Keep geometry pure (inputs → rects) so it is unit-testable without Gpu.

## Viewer UI map

| Concern | Location |
| --- | --- |
| Runtime fields | `viewer/document.rs` (`ViewerRuntime`) |
| Messages | `shell/messages.rs` (document/page/zoom/find/progress variants) |
| Update | `viewer/update.rs` |
| Tasks | `viewer/tasks.rs`, debounced session helpers in `lib.rs` |
| Canvas draw | `components/viewer/canvas.rs` |
| Toolbar / zoom / find / outline | `components/viewer/*` |
| Shortcuts | `shell/shortcuts.rs` (Home/End/PageUp/PageDown, F3, Esc → library) |

### Primary keyboard map (viewer)

| Chord | Action |
| --- | --- |
| `+` / `-` / `0` | Zoom in / out / automatic |
| Ctrl+wheel | Zoom toward cursor |
| Space / PageDown · Shift+Space / PageUp | Page or viewport step |
| Home / End | First / last page |
| Ctrl+F | Find; F3 / Shift+F3 next/previous |
| Ctrl+G | Jump to page |
| Esc | Dismiss chrome, then hide TOC, then back to library |

Plan status and remaining P1/P2 work: `scratch/viewer-ux-plan.md`.

## Performance checklist

| Symptom | Likely cause |
| --- | --- |
| Zoom stutter | Settle delay too short; too many concurrent renders |
| Blank pages | Pending keys not scheduled; cache miss + failed render message; shallow prefetch |
| High memory | Tile cache too large; huge width_px |
| UI freezes | Pdfium called on main thread (should never happen) |
| Mutex contention | Many threads rendering — expected to serialize on Pdfium lock |
| Scroll jank + disk thrash | Session save not debounced (should use `schedule_session_save`) |
| Stale library “reading” state | Progress not flushed / `ProgressUpdated` not firing |

## Related

- [Core crate](../crates/core.md)
- [UI crate](../crates/ui.md)
- [Search](search.md)
- [Runtime state](../architecture/state.md)
