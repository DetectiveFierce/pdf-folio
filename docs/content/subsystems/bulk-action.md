---
title: Life of a Bulk Action
eyebrow: Subsystems
lede: One end-to-end walkthrough tying selection, drag, DB snapshots, tasks, and sync dirtying together.
order: 27
---

<p class="trail"><strong>Trail</strong> <a href="../architecture/messages.md">Messages</a> <span class="sep">·</span> <a href="database.md">Database</a> <span class="sep">·</span> <a href="../crates/ui.md">UI crate</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/library/tasks.md">API · tasks</a></p>

Concrete scenario: **select 40 PDFs, drag them onto a folder.**

## 1. Selection (synchronous)

Card/row clicks update selection in [`LibraryRuntime`](../architecture/state.md#library-libraryruntime) via helpers in [`components/library/selection.rs`](../api/pdf-folio-ui/components/library/selection.md):

- Click — select only / open depending on target
- Ctrl/Cmd-click — toggle
- Shift-click — range via `range_selection_ids`

Handled in [`library/update.rs`](../api/pdf-folio-ui/library/update.md) with **no async** — selection is pure UI state. Message clusters: [Message surface](../architecture/messages.md#clusters-mental-index).

## 2. Drag geometry (pure)

[`components/library/drag.rs`](../api/pdf-folio-ui/components/library/drag.md) owns hit-testing, auto-scroll velocity, drop targets, and reorder index math. It does not open SQLite. Dropping onto a folder resolves a folder target id; the update handler decides the DB operation. Cursor position lives on [`ChromeRuntime`](../architecture/state.md).

## 3. Snapshot + write (async task)

Before membership changes:

1. Capture [`LibraryOrganizationSnapshot`](../api/pdf-folio-core/db/types.md) (before).
2. Run `entry_folders` updates on the blocking pool ([`library/tasks.rs`](../api/pdf-folio-ui/library/tasks.md) bulk/move helpers).
3. Capture after snapshot for undo stack ([organization](../api/pdf-folio-core/db/organization.md)).
4. Emit completion [message](../architecture/messages.md); UI refreshes lists and clears drag state.

Undo later restores the before snapshot — see [Database · undo](database.md#undo-as-snapshots).

## 4. Sync dirtying

Writes bump the metadata that CRDT preparation watches. The next auto-sync preflight ([`prepare_local_crdt_operations`](../api/pdf-folio-cloud/sync/crdt.md)) turns membership changes into ops without a special “bulk” code path on the wire. Full pass: [A sync pass](sync.md#a-sync-pass).

## 5. Concurrency

Filesystem [watch events](search.md#filesystem-watching) or remote [sync pulls](sync.md) arrive as ordinary messages. The single-threaded reducer serializes them with the bulk completion message — there is no parallel mutation of [`PDFolioApp`](../architecture/state.md). See [architecture overview](../architecture/overview.md).

## Pattern to copy

Any new multi-entry library edit should:

1. Update selection/UI state sync in [`library/update`](../api/pdf-folio-ui/library/update.md).
2. Perform IO in [`library/tasks`](../api/pdf-folio-ui/library/tasks.md) with `spawn_blocking` / `Task::perform`.
3. Use organization snapshots if the edit should be undoable ([database](database.md#undo-as-snapshots)).
4. Return a completion [`Message`](../architecture/messages.md) that refreshes derived UI (folders, entries, status).
5. Rely on existing [sync seeding](../api/pdf-folio-core/db/sync.md) rather than inventing a second dirty-tracking system.
