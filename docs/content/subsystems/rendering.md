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

`ZoomRenderPolicy` in `viewer/rendering.rs` encodes when to use preview width vs committed width.

### Zoom presets

UI zoom controls map to width presets / percent labels (`ZoomPreset`, `zoom_percent` helpers). Changing spread mode (single vs two-page) recomputes layout but reuses tiles when widths match.

## Text layer and outline

`text_layer(index)` returns character boxes as **normalized** fractions of page width/height (not PDF points or pixels). Selection highlights and find-in-document scale by multiplying by the current on-screen page size — no Pdfium re-query on every zoom.

`outline()` walks bookmarks into `OutlineNode { title, page, children }` pure trees for the TOC sidebar.

`text_on_page` extracts plain text strings used when building the Tantivy index (see [Search](search.md)).

### Find-in-document

Viewer find is separate from library Tantivy search:

| Feature | Scope | Engine |
| --- | --- | --- |
| Library search | All indexed pages in the vault | Tantivy |
| Find bar | Open document text layers | In-memory match over loaded layers |

Load text layers lazily for visible/nearby pages when find needs them.

## Layout geometry

`viewer/layout.rs` and `viewer/navigation.rs` compute:

- Page rectangles in scroll content space
- Visible page ranges for the viewport
- Click → page mapping for outline and thumbnails

Keep geometry pure (inputs → rects) so it is unit-testable without Gpu.

## Viewer UI map

| Concern | Location |
| --- | --- |
| Runtime fields | `viewer/document.rs` (`ViewerRuntime`) |
| Messages | `shell/messages.rs` (document/page/zoom/find variants) |
| Update | `viewer/update.rs` |
| Tasks | `viewer/tasks.rs` |
| Canvas draw | `components/viewer/canvas.rs` |
| Toolbar / zoom / find / outline | `components/viewer/*` |

## Performance checklist

| Symptom | Likely cause |
| --- | --- |
| Zoom stutter | Settle delay too short; too many concurrent renders |
| Blank pages | Pending keys not scheduled; cache miss + failed render message |
| High memory | Tile cache too large; huge width_px |
| UI freezes | Pdfium called on main thread (should never happen) |
| Mutex contention | Many threads rendering — expected to serialize on Pdfium lock |

## Related

- [Core crate](../crates/core.md)
- [UI crate](../crates/ui.md)
- [Search](search.md)
- [Runtime state](../architecture/state.md)
