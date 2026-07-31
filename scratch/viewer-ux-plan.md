# Viewer UX refinement — implementation plan

> Status: **P0 + P1 complete** (2026-07-31).  
> Source evaluation: conversation analysis of `viewer/`, `components/viewer/`, shortcuts, session, progress.  
> Goal: smoother scrolling/zoom, trustworthy progress, discoverable chrome, maintainable code + docs.

### Landed in this pass

| ID | Status |
| --- | --- |
| P0.1 Page-mode wheel | Done |
| P0.2 Scrollable sync on pan | Done (`pan_horizontally_by` → Task) |
| P0.3 Debounced session save | Done (`SessionSaveSettled`, ~400ms) |
| P0.4 Debounced reading progress | Done (`ProgressSaveSettled` + flush on leave) |
| P0.5 Esc + context menu labels | Done |
| P1.1 Multiplicative zoom | Done (~1.12×) |
| P1.2 Deeper prefetch | Done (~4 page margin) |
| P1.3 Longer page fade | Done (140ms) |
| P1.4 Home/End/Page keys | Done |
| P1.5 Toolbar Find + Contents | Done |
| P1.6 Center-based `current_page` | Done |
| P1.7 Outline jump + scroll-spy | Done (basic) |
| P1.8 Thumbnail windowed render | Done (±12 pages) |
| P1.9 Progressive find + compact bar + F3 | Done |
| P1.10 Word/line selection | Done (double/triple click) |
| P2.* | Pending |

---

## Principles

1. **One write path for scroll** — every programmatic offset change ends in `scroll_viewer_to_offsets_task()`.
2. **Debounce hot-path I/O** — continuous scroll must not thrash session disk writes or progress DB updates.
3. **Generation counters** — stale delayed tasks (zoom settle, session save, progress save) are ignored.
4. **Reader-first chrome** — primary actions (find, TOC, page, zoom) beat theme/open in the toolbar.
5. **Docs track behavior** — update `docs/content/subsystems/rendering.md` and module rustdocs with each phase.

---

## Success metrics

| Metric | Target |
| --- | --- |
| Page-mode wheel | One page/spread per wheel notch |
| Continuous scroll blanks | ≤1 blank page under normal wheel after warm cache |
| Zoom settle | No full-blank flash; re-render within ~150–200ms of last input |
| Session writes while scrolling | ≤ ~2/s (debounced) |
| Reopen library entry | Page within ±1 of last reading position (`entries.last_page`) |
| Shortcut labels | Match real bindings in context menu |

---

## PR / phase DAG

```text
P0 ──► P1a ──► P1b ──► P2
 │       │       │
 │       │       └─ outline/thumbs/find polish
 │       └─ zoom math, prefetch, keyboard, toolbar
 └─ correctness: wheel, progress, session, scroll sync, Esc
```

Phases can land as sequential commits on `main` or stacked PRs; dependencies flow left→right.

---

## P0 — Correctness & trust (ship first)

### P0.1 Page-mode wheel navigation

| | |
| --- | --- |
| **Problem** | Canvas only emits `ViewportWheelScrolled` for Ctrl or Horizontal mode; Page mode wheel never turns pages. |
| **Change** | Capture wheel when `ViewerScrollMode::Page` (and keep Horizontal/Ctrl paths). |
| **Files** | `components/viewer/canvas.rs`, tests if useful |
| **Done when** | Page mode: wheel/trackpad advances `scroll_page_mode_by(±1)`. |

### P0.2 Scrollable sync on keyboard pan

| | |
| --- | --- |
| **Problem** | `pan_horizontally_by` / some shortcut paths update offsets without iced `scroll_to`. |
| **Change** | `pan_horizontally_by` → `Task` batching `request_visible_pages` + `scroll_viewer_to_offsets_task`. All shortcut pans use it. |
| **Files** | `viewer/navigation.rs`, `shell/shortcuts.rs` |

### P0.3 Debounced session save on scroll

| | |
| --- | --- |
| **Problem** | `ViewportChanged` calls `save_app_session_task` every frame. |
| **Change** | `session_save_generation` + `SessionSaveSettled` (~400ms). Discrete actions (jump, zoom preset, mode, open) keep immediate save. |
| **Files** | `shell/app.rs`, `shell/messages.rs`, `lib.rs` helpers, `viewer/update.rs`, `shell/update.rs` |

### P0.4 Debounced reading progress

| | |
| --- | --- |
| **Problem** | `ProgressUpdated` is never emitted from the viewer; `last_page` freezes after open. |
| **Change** | On page change (viewport/jump/page keys), schedule debounced progress (~500ms). Flush immediately on leave viewer. Update DB + in-memory `library_entries[].last_page`. |
| **Files** | `viewer/document.rs`, `viewer/state.rs` or `tasks.rs`, `viewer/update.rs`, `library/update.rs`, `shell/update.rs` (BackToLibrary) |

### P0.5 Escape + context-menu labels

| | |
| --- | --- |
| **Problem** | Menu claims Esc → library; Esc only collapses chrome/TOC. Zoom menu shows Ctrl++ but shortcuts are bare `+`/`-`/`0`. |
| **Change** | Esc ladder: overlays → find → selection → close TOC → **BackToLibrary**. Fix context menu accelerator strings. |
| **Files** | `shell/shortcuts.rs`, `components/shared/context_menu.rs` |

---

## P1 — Daily reading feel

### P1a Interaction & performance

| ID | Work | Files (primary) |
| --- | --- | --- |
| P1.1 | Multiplicative zoom steps (~1.12×) for buttons, shortcuts, Ctrl+wheel | `viewer/rendering.rs`, `viewer/update.rs`, `shell/shortcuts.rs` |
| P1.2 | Deeper direction-aware prefetch (more neighbors) | `viewer/layout.rs`, tests |
| P1.3 | Longer page fade-in (~120–160ms) | style tokens / KDL layout metric |
| P1.4 | Named keys: PageUp/PageDown/Home/End | `shell/shortcuts.rs`, `shell/messages.rs` |
| P1.5 | Toolbar: Find control; keep layout tidy | `components/viewer/toolbar.rs` |
| P1.6 | `current_page()` prefer page containing viewport center | `viewer/state.rs` |

### P1b Chrome & navigation polish

| ID | Work | Files (primary) |
| --- | --- | --- |
| P1.7 | Outline: click jumps; chevron expands; current-section highlight | `components/viewer/outline.rs`, state helpers |
| P1.8 | Thumbnails: scroll-to-current; avoid full-doc render fan-out (visible window) | `components/viewer/sidebar.rs`, `viewer/state.rs` |
| P1.9 | Find: F3/Shift+F3; progressive layer request; compact option affordances | `find_bar`, `shortcuts`, `state` |
| P1.10 | Word double-click / line triple-click selection (if time) | `canvas.rs`, selection state |

---

## P2 — Expand product surface (later)

- Hand tool / middle-drag pan; Space-drag
- Immersive chrome auto-hide
- PDF link hit-testing
- Sub-page tiles or max render width at high zoom
- Annotations (existing product plan)
- Trackpad pinch if iced/platform exposes it

---

## Maintainability conventions

| Concern | Rule |
| --- | --- |
| Debounce helpers | Live in `viewer/tasks.rs` (viewer) or `lib.rs` / `session` (app-wide session) with generation pattern mirroring `schedule_zoom_render` |
| Progress | Single helper `schedule_reading_progress_save` / `flush_reading_progress` on `PDFolioApp` |
| Scroll | Never set `scroll_offset` / `horizontal_offset` from shortcuts without scrollable sync task |
| Docs | After each phase: rustdoc on new messages/helpers; `docs/content/subsystems/rendering.md` UX notes; this plan status line |

---

## Test plan

| Area | Test |
| --- | --- |
| Prefetch | Existing + expanded neighbor counts |
| Page mode layout | Existing multipage fixture tests |
| Progress | Unit: scheduling only when entry_id + page changes; flush on return_to_library path (logic-level) |
| Session debounce | Generation invalidation (mirror zoom settle test) |
| Shortcuts | Esc ladder / menu labels (unit where pure) |

---

## Implementation order (this branch)

1. Plan doc (this file) + subsystem doc scaffold  
2. P0.1–P0.5 code  
3. P1.1–P1.5 as capacity allows  
4. Docs + `cargo test -p pdf-folio-ui` smoke  

P1.7–P1.10 and P2 follow in subsequent work.
