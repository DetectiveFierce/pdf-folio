---
title: Architecture Overview
eyebrow: Architecture
lede: One reducer, one Message enum, one direction of data flow — background work proposes changes; only update mutates state.
order: 1
---

<p class="trail"><strong>Trail</strong> <a href="workspace.md">Workspace</a> <span class="sep">·</span> <a href="shell.md">Shell</a> <span class="sep">·</span> <a href="messages.md">Messages</a> <span class="sep">·</span> <a href="state.md">State</a> <span class="sep">·</span> <a href="../reference/glossary.md">Glossary</a></p>

PDF-Folio's UI crate is built on **iced**, which follows the Elm architecture. Every event the app can react to is a variant of a single [`Message`](messages.md) enum. A single `update` function is the only place allowed to change application state. Side effects (Pdfium, SQLite, network, disk) run inside iced `Task`s and report back as more messages.

<div class="diagram">user input / async result
        │
        ▼
  <span class="hl">Message</span>  ──────────►  <span class="hl">update</span>(&amp;mut PDFolioApp, Message) → Task&lt;Message&gt;
        ▲                                  │
        │                                  ▼
   Task&lt;Message&gt; ◄──────────────  side effects (DB, render, network, FS)
        │
        ▼
   view(&amp;PDFolioApp) → Element&lt;Message&gt;</div>

Anything that might block runs on Tokio's blocking pool via `Task::perform` / `spawn_blocking`. Background work never mutates [`PDFolioApp`](state.md) directly — it can only *propose* a change by emitting a message that flows through the same reducer as a mouse click.

This invariant is load-bearing:

| Allowed | Forbidden |
| --- | --- |
| `update` mutates [`PDFolioApp`](../api/pdf-folio-ui/shell/app.md) | Background threads writing `PDFolioApp` fields |
| Tasks return results as [`Message`](messages.md)s | Long-lived threads holding UI state |
| Views read state and emit messages | Views opening SQLite / Pdfium on the main thread for heavy work |
| [Subscriptions](shell.md#subscriptions) emit messages | Subscriptions writing the database |

## Process entry

| Binary / path | Role |
| --- | --- |
| [`pdf-folio`](../crates/main.md) (`pdf-folio-main`) | Desktop app, or [`pdf-folio sync <cmd>`](../operations/cli.md) CLI |
| [`pdf-folio-sync-server`](../api/pdf-folio-cloud/bin/pdf-folio-sync-server.md) | Control-plane HTTP service (identity + short-lived credentials) |
| [`crdt-sync-once`](../api/pdf-folio-cloud/bin/crdt-sync-once.md) / [`ensure-turso-schema`](../api/pdf-folio-cloud/bin/ensure-turso-schema.md) | Maintenance binaries for remote metadata |

Desktop launch path:

1. [`pdf-folio-main`](../crates/main.md) initializes tracing and parses CLI args.
2. With no subcommand, it calls [`pdf_folio_ui::run`](../api/pdf-folio-ui/index.md) with an optional PDF path.
3. `run` loads [session](shell.md#session-and-auth) + [library registry](../subsystems/multi-library.md), constructs [`PDFolioApp`](state.md), then starts `iced::application` with `update` / `view` / `subscription`.
4. First paint is staged: heavy work (thumbnail fan-out, registry network) waits for `StartupBackgroundReady`.

See [Application shell](shell.md) for routing details, [Runtime state](state.md) for the `PDFolioApp` tree, and [Message surface](messages.md) for how events are clustered.

## Modes

[`AppMode`](../api/pdf-folio-ui/shell/app.md) selects which primary surface is active:

| Mode | Meaning | Typical owner |
| --- | --- | --- |
| `SignedOut` | Sync sign-in gate (when auth is required) | [shell + session](shell.md#session-and-auth) |
| `Library` | Library manager: grid/list, folders, tags, inspector | [`library/*`](../crates/ui.md#library-library-mode-domain) |
| `Viewer` | PDF reading surface | [`viewer/*`](../crates/ui.md#viewer-viewer-mode-domain) |
| `LibrarySwitcher` | Multi-library vault picker | [registry](../subsystems/multi-library.md) |

Mode switches are ordinary [messages](messages.md). Opening a PDF usually moves `Library → Viewer`; Back returns to Library without necessarily dropping all viewer caches (see [viewer runtime](state.md#viewer-viewerruntime) fields).

## Update routing

The shell update function ([`shell/update.rs`](../api/pdf-folio-ui/shell/update.md)) does **not** match every message itself. It first offers the message to domain updaters:

```text
shell::update(app, message)
  1. library::update::update(app, &message)  → Some(Task) | None
  2. viewer::update::update(app, &message)   → Some(Task) | None
  3. match remaining shell/chrome/sync/registry messages
```

Library and viewer modules return `None` for messages they do not own, so the shell keeps cross-cutting concerns (startup probes, sync auth, auto-sync, menus, session). This keeps the huge [message surface](messages.md) partitioned without splitting the enum. Details: [how to add a message](messages.md#how-to-add-a-new-message).

**Rule of thumb when adding a message:**

1. Put the variant in [`shell/messages.rs`](../api/pdf-folio-ui/shell/messages.md) near related variants.
2. Handle it in the domain updater that owns that state ([`library/update`](../api/pdf-folio-ui/library/update.md) or [`viewer/update`](../api/pdf-folio-ui/viewer/update.md)) when possible.
3. Only handle in the shell match if it spans modes (auth, global menus, style reload, multi-library switch).

## Side effects as tasks

Heavy or fallible work is packaged as functions that return `Task<Message>` (often in [`library/tasks.rs`](../api/pdf-folio-ui/library/tasks.md), [`viewer/tasks.rs`](../api/pdf-folio-ui/viewer/tasks.md), [`shell/tasks.rs`](../api/pdf-folio-ui/shell/tasks.md)):

```text
user action message
  → update schedules Task
  → spawn_blocking / async work
  → completion Message (Ok payload or error string)
  → update applies result to state
  → view reflects new state
```

Common patterns:

| Pattern | Example |
| --- | --- |
| One-shot I/O | Open PDF, import folder, rename tag |
| Generation counters | Search queries, zoom settle — ignore stale completions |
| Queued exclusive work | Auto-sync (`sync_in_progress` + queue) |
| Snapshot undo | Capture org snapshot before mutation; restore on undo |

## Subscriptions

Background event sources become messages through iced subscriptions ([`shell/subscriptions.rs`](../api/pdf-folio-ui/shell/subscriptions.md)):

- [Keyboard shortcuts](shell.md#shortcuts)
- Filesystem [style-book](../subsystems/style-system.md) watch (KDL hot reload)
- Library folder watches (`notify` → [`LibraryWatchEvent`](../subsystems/search.md#filesystem-watching))
- [Auto-sync](../subsystems/sync.md) timer ticks
- Cursor position / window events used by [drag](../subsystems/bulk-action.md) and menus

Like tasks, subscriptions never write app state; they only emit messages. Startup may delay enabling expensive subscriptions until the first frame is interactive.

## Layers (mental model)

| Layer | Crate(s) | Responsibility |
| --- | --- | --- |
| Binary | [`pdf-folio-main`](../crates/main.md) | CLI, tracing, dispatch |
| Presentation | [`pdf-folio-ui`](../crates/ui.md), [`pdf-folio-style`](../crates/style.md) | State machine, views, chrome, tokens |
| Domain I/O | [`pdf-folio-core`](../crates/core.md) | PDF bytes → tiles/text; SQLite; search index |
| Cloud | [`pdf-folio-cloud`](../crates/cloud.md) | Sync, Raindrop, control-plane server |
| Platform patch | [`iced-widget-patch`](../crates/iced-patch.md) | Scrollable scrollbar placement for sidebars |

Full dependency rules: [Workspace & crates](workspace.md).

## Failure handling philosophy

- Domain functions return `anyhow::Result` / structured errors; UI turns them into banners, status lines, or dialog text.
- Missing Pdfium, missing files, and network failures should **not** abort the process — they become user-visible errors and recoverable state (`missing` flags, document error fields).
- Prefer partial success for bulk ops (import folder accumulates per-file errors).

