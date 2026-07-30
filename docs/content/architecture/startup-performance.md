---
title: Startup Performance
eyebrow: Architecture
lede: How PDF-Folio becomes visible first, then accepts input while thumbnail and cloud work continue in the background.
order: 2
---

<p class="trail"><strong>Trail</strong> <a href="overview.md">Overview</a> <span class="sep">·</span> <a href="shell.md">Application shell</a> <span class="sep">·</span> <a href="../subsystems/sync.md">Cross-device sync</a> <span class="sep">·</span> <a href="../operations/development.md">Development</a></p>

PDF-Folio starts from its persisted **local snapshot**: the session, active-library registry, SQLite library state, and on-disk thumbnail cache. It deliberately does not wait for the cloud or for a full thumbnail render before showing the library.

This page distinguishes two user-visible milestones:

| Milestone | Meaning | What must be complete |
| --- | --- | --- |
| **Time to first paint (TTFP)** | A usable library or viewer surface can be drawn. | Local session/registry/database state and styles are available; iced has built the first view. |
| **Time to first interaction (TTI)** | The event loop accepts an update, so input can be handled without startup work monopolizing it. | First paint plus no queued synchronous work that prevents the reducer from running. |

TTFP is the visual promise; TTI is the responsiveness promise. They often overlap, but they are not interchangeable in a desktop app that continues loading visible images and remote changes after the window appears.

The startup probe reports TTI directly. It also reports local construction and view-tree durations, which are useful contributors to TTFP, but it does not claim to timestamp the compositor presenting pixels on screen. Treat TTFP as the point at which the initial view has been built and submitted to iced; use compositor-level tooling when an end-to-end present timestamp is required.

## Launch timeline

With no PDF passed on the command line, the desktop path is:

```text
process entry
  → tracing + CLI parsing
  → load session.json and the active library registry
  → open local SQLite database and read library/folder state
  → load style book and construct PDFolioApp
  → iced creates the window and draws the local library snapshot       ← TTFP
  → cached visible thumbnails are read from disk (no PDF open)
  → missing covers, sync, and registry refresh start in background
  → event loop processes a normal message                              ← TTI
```

The binary captures its startup clock at process entry, before tracing and CLI parsing. The [application shell](shell.md#launch-sequence) owns the rest of this staged launch, while the [architecture overview](overview.md#process-entry) explains why all side effects return through the `Message` reducer rather than mutate UI state directly.

Passing a PDF path changes the intent: the supplied file wins over session restore and the app opens the viewer. That is a document-opening workflow, not the snapshot-first library launch described here; see [the rendering pipeline](../subsystems/rendering.md#opening-a-document).

## Time to first paint

The first frame is built from local, bounded work:

1. Read the small session snapshot for window size, selected mode, and library presentation state.
2. Read the local multi-library registry and open only the active library database.
3. Load preferences, entries, folders, trash state, authentication state, and the style book.
4. Construct [`PDFolioApp`](state.md) and let iced draw its library/viewer surface.

The first frame does **not** wait for a remote request, a library-wide PDF scan, or every cover image. The first visible image pass reads already-generated RGBA cover variants from the local cache. A cache miss is converted into a background render task after the snapshot has been shown. The cache location and lifetime are described in [Data directories](../operations/data-dirs.md#cache-directory); the underlying document/render costs are described in [Rendering pipeline](../subsystems/rendering.md).

This means a first paint may initially contain cover placeholders or a small subset of cached covers. That is intentional: the library structure, selection, navigation, and controls are already present, and each arriving thumbnail updates only the affected card.

## Time to first interaction

For a measurable definition of TTI, PDF-Folio has an opt-in startup probe. It schedules an ordinary message after iced has started; when the reducer processes it, the app logs the elapsed time from binary entry:

```bash
PDF_FOLIO_STARTUP_PROBE=1 RUST_LOG=info cargo run -p pdf-folio-main
```

The key log is `PDF-Folio startup responsiveness probe processed`:

| Field | Interpretation |
| --- | --- |
| `total_ms` | Process entry → first normal message accepted by the UI reducer. This is the TTI metric. |
| `probe_wait_ms` | Timer delay requested by the probe; it provides the event loop an opportunity to start. |
| `update_queue_ms` | Time from the probe being emitted to it being processed. A large value signals startup work delaying interaction. |

With the same environment and persisted profile, use the median of several cold process launches rather than a single number. Record the library size, whether the session starts in Library or Viewer mode, and whether the thumbnail cache is warm. This makes regressions comparable rather than accidental. General logging and development commands are in [Development](../operations/development.md#logging), and test gates are in [Testing](../operations/testing.md).

When the probe is enabled, synchronous startup phase timings are also logged for registry load, database open, preferences, styles, auth, entries, and folders. It also samples root and library view-tree construction. Those numbers locate which **local construction** phase grew; TTI catches any later task or main-thread work that still prevents interaction.

## Why first paint and interaction differ here

After the first frame, PDF-Folio intentionally has work still in flight:

| After-paint work | Why it is deferred | How the UI stays coherent |
| --- | --- | --- |
| Cached thumbnail hydration | Disk reads are cheaper than PDF rendering, but still unnecessary before the library shell exists. | Each result is a `ThumbnailReady` message that replaces one placeholder. |
| Missing-thumbnail rendering | Opening/rendering PDFs is CPU and I/O intensive. | A bounded worker pool renders only the virtualized visible set. |
| Cloud/registry sync | Network availability must not gate local access. | A preflight compares the local revision/snapshot with the remote head, then applies only needed changes through messages. |
| Watches and other expensive subscriptions | They are useful after startup but do not define the first usable frame. | They emit messages after readiness rather than writing UI state directly. |

The message/task design matters here. A task can finish at any time, but only the normal update path applies its result, so the UI remains responsive and a remote change is merged into the local state in the same way as a local action. See [side effects as tasks](overview.md#side-effects-as-tasks), [message routing](messages.md#routing-ownership), and [runtime persistence](state.md#persistence-map).

The sync preflight is especially important for repeat launches. It persists a local change revision and the revision represented by the last CRDT snapshot. If they match, it can reuse that snapshot rather than rescanning every document before deciding whether remote work exists. The full protocol and failure behavior are documented in [Cross-device sync](../subsystems/sync.md#a-sync-pass).

## The library-mode restore rule

A saved session retains the last viewer document path so the user can return to it. That path must not be treated as a command to open the document when the saved surface is **Library**. Parsing a large PDF only to restore the library afterward creates a long period in which the window cannot accept messages.

At launch, PDF-Folio now restores a saved document only when the saved mode is Viewer. A Library-mode session draws its stored library state immediately and leaves the previous document unopened. This is the distinction that protects snapshot-first TTI while preserving viewer continuity when the user actually ended the prior session in the viewer.

## Performance checklist

- Keep the initial database queries bounded to state required to draw the active surface.
- Treat thumbnail files as a persistent cache: load visible variants first; render only misses in the background.
- Never put remote sync, bulk thumbnail work, or PDF parsing on the startup critical path for a Library session.
- Preserve the probe and phase logs when altering launch code; benchmark several launches before and after a change.
- When changing session semantics, test both Library-mode and Viewer-mode restores, plus a CLI PDF path.

Source ownership and entry points are mapped in [pdf-folio-main](../crates/main.md), [pdf-folio-ui](../crates/ui.md), [pdf-folio-core](../crates/core.md), and [pdf-folio-cloud](../crates/cloud.md).
