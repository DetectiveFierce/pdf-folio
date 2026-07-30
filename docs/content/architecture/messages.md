---
title: Message Surface
eyebrow: Architecture
lede: The single Message enum is the UI event vocabulary — how variants are clustered, routed, and extended safely.
order: 5
---

<p class="trail"><strong>Trail</strong> <a href="overview.md">Overview</a> <span class="sep">·</span> <a href="shell.md">Shell</a> <span class="sep">·</span> <a href="state.md">State</a> <span class="sep">·</span> <a href="../subsystems/bulk-action.md">Bulk action</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/shell/messages.md">API · messages</a></p>

**Code:** [`crates/pdf-folio-ui/src/shell/messages.rs`](../api/pdf-folio-ui/shell/messages.md)

Every user action, keyboard shortcut, async completion, and subscription tick enters the app as a [`Message`](../api/pdf-folio-ui/shell/messages.md). There is no second channel for “internal events” — completions from tasks are also `Message` variants. That makes the update log conceptually total: if it happened, it was a message.

## Why one enum

| Benefit | Cost |
| --- | --- |
| Exhaustive matching forces new events through review | The enum is large (~messages.rs is hundreds of lines) |
| Domain updaters can ignore foreign variants | Must keep clusters organized |
| Easy to log / probe / test with pure values | Cannot hide side channels |

The team accepted the size of the enum so routing stays explicit. **Do not** invent parallel channels (globals, channels into UI state) for app-visible events.

## Related types living next to Message

| Type | Role |
| --- | --- |
| `ContextMenuTarget` | Which surface opened a context menu |
| `ContextMenuAction` | Actions from right-click menus |
| `ConfirmationAction` | Destructive ops that require a confirm dialog |
| `LibrarySidebarTab` / `ViewerSidebarTab` | Sidebar navigation tabs |
| `CommandId` (in `commands.rs`) | Palette / menu command identifiers mapped into messages or tasks |

## Clusters (mental index)

When searching `messages.rs`, think in clusters rather than alphabetical order. Approximate groups:

### Shell / lifecycle

- Startup probe, `StartupBackgroundReady`
- Window / cursor updates used by drag and menus
- Style reload / theme change
- Session-related restores

### Auth & sync

- Sign-in start/complete/fail
- Auto-sync tick, remote-available, sync progress / result
- Queueing fields on `PDFolioApp` coordinate exclusive passes

### Library browsing

- Sort, layout, grid zoom, metadata density
- Selection (click, range, select-all, clear)
- Folder tree navigation and expansion
- Filters (reading state, tags, search query)
- Drag begin/move/end and drop targets
- Inspector / details edits

### Library mutations (often pair request + result)

- Import file/folder + progress + summary
- Create/rename/delete folder, tags
- Bulk metadata, trash, restore, permanent delete
- Export dialog + progress + summary
- Raindrop preview/import/progress/rollback
- Organization undo/redo

### Multi-library registry

- Create / rename / delete / switch library profiles
- Registry load/sync results

### Viewer / document

- Open document / document opened / document error
- Page rendered, text layer loaded
- Scroll, page jump, spread/scroll mode
- Zoom change, zoom settle generation
- Find bar query, matches, next/prev
- Outline load and navigation
- Text selection copy

### Chrome

- Open/close context menu
- Command palette query / execute
- Confirmation dialog accept/cancel
- Error banner dismiss

Exact variant names evolve; use search in `messages.rs` and the domain `update` match arms as the source of truth.

## Routing ownership

```text
Message arrives
    │
    ├─ library::update matches library cluster? → Some(Task)
    ├─ viewer::update matches viewer cluster?  → Some(Task)
    └─ shell::update match                      → Task (always)
```

| Cluster | Prefer handler in |
| --- | --- |
| Selection, folders, bulk, import UI | `library/update.rs` + `library/tasks.rs` |
| Zoom, find, outline, page renders | `viewer/update.rs` + `viewer/tasks.rs` |
| Auth, auto-sync, menus, style, mode | `shell/update.rs` + `shell/tasks.rs` |
| Registry switcher | library registry modules + shell for mode |

If both library and viewer could claim a message, pick one owner and document it in the match arm. Returning `Some(Task::none())` is valid when the message is fully handled with only state mutations.

## How to add a new message

1. **Define the variant** in `shell/messages.rs` near related variants. Put payload data on the variant; avoid out-of-band lookups when the task already has the data.
2. **Emit it** from a view (`on_press`, etc.), shortcut map, subscription, or task completion.
3. **Handle it** in the domain updater:
   - Synchronous UI-only changes: mutate `app` and return `Some(Task::none())` or a small batch of save tasks.
   - I/O: schedule a task that ends with a *result* message.
4. **Persist if needed** — many library prefs and session fields save via dedicated tasks (`save_library_preferences_task`, `save_app_session_task`).
5. **Test** pure helpers and, when practical, update paths in `pdf-folio-ui` tests without a GPU session.

### Pair request / response

For fallible work, prefer two variants (or one with a `Result` payload):

```text
Message::RenameFolder { id, name }           // optional: only if sync path
// more common: command/task started from existing UI message
Message::FolderRenamed(Result<Folder, String>)
```

Many code paths skip the “request” message and start a `Task` directly from a button message; the important part is that **results re-enter as messages**.

### Generation counters

Search and zoom settle use generations so late results from superseded work are ignored:

```text
app.library.search_generation += 1;
let gen = app.library.search_generation;
// task completes → Message::SearchFinished { generation, … }
// update: if generation != app.library.search_generation { ignore }
```

Reuse this pattern for any high-churn async input.

## Commands vs messages

- **Messages** = events (“this happened”).
- **Commands** (`shell/commands.rs`, `CommandId`) = named user intents from menus/palette that *resolve* into messages and/or tasks.

The palette and menu bar should not special-case business logic; they select a `CommandId` / emit a message that the same update path handles as a toolbar button.

## Debugging tips

| Symptom | Check |
| --- | --- |
| Click does nothing | View emits wrong/missing message; updater returns `None` and shell ignores |
| UI updates then reverts | Completion message overwrites with stale data; missing generation guard |
| Double DB write | Both library and shell handlers processing same message |
| Sync races | `sync_in_progress` / queue fields; overlapping auto-sync ticks |

Enable tracing:

```bash
RUST_LOG=pdf_folio_ui=debug cargo run -p pdf-folio-main
```

## API reference

- [messages module](../api/pdf-folio-ui/shell/messages.md)
- [shell update](../api/pdf-folio-ui/shell/update.md)
- [library update](../api/pdf-folio-ui/library/update.md)
- [viewer update](../api/pdf-folio-ui/viewer/update.md)

## Related

- [Architecture overview](overview.md)
- [Application shell](shell.md)
- [Runtime state](state.md)
- [Bulk action walkthrough](../subsystems/bulk-action.md)
