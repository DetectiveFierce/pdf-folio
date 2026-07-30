---
title: Application Shell
eyebrow: Architecture
lede: How messages, commands, shortcuts, session restore, and subscriptions are wired in pdf-folio-ui.
order: 3
---

<p class="trail"><strong>Trail</strong> <a href="overview.md">Overview</a> <span class="sep">·</span> <a href="messages.md">Messages</a> <span class="sep">·</span> <a href="state.md">State</a> <span class="sep">·</span> <a href="../crates/ui.md">UI crate</a> <span class="sep">·</span> <a href="../api/pdf-folio-ui/shell.md">API · shell</a></p>

The shell is the orchestration layer in [`crates/pdf-folio-ui/src/shell/`](../api/pdf-folio-ui/shell.md). It owns the iced application loop glue, the giant `Message` enum, cross-mode chrome (menus, command palette, confirmations), session persistence, and sync-auth gating.

Domain features (library grid, PDF canvas) live under `library/` and `viewer/`; the shell decides **when** those surfaces are active and handles everything that spans modes.

## Module map

| File | Responsibility |
| --- | --- |
| `app.rs` | `PDFolioApp`, `AppMode`, `Settings`, `LibraryRuntime`, `ChromeRuntime`, `AppearanceRuntime` |
| `messages.rs` | [`Message`](messages.md) enum and related menu/context types |
| `update.rs` | Top-level reducer ([`update`](../api/pdf-folio-ui/shell/update.md)); delegates library/viewer first |
| `commands.rs` | Higher-level command helpers used by menus and palette |
| `shortcuts.rs` | Keyboard binding → message mapping |
| `subscriptions.rs` | iced subscription tree |
| `session.rs` | App session + `SyncAuthRuntime` load/save |
| `tasks.rs` | Shell-owned async tasks (sync registry, auto-sync, …) |
| `platform.rs` | Linux file-manager reveal helpers |
| `constants.rs` | Shared timing/size constants |

API pages: [shell](../api/pdf-folio-ui/shell.md) · [app](../api/pdf-folio-ui/shell/app.md) · [messages](../api/pdf-folio-ui/shell/messages.md)

## Launch sequence

`pdf_folio_ui::run` (`lib.rs`):

1. Optionally load `session.json` (skipped when a CLI file path is provided so the file wins).
2. Load multi-library registry (`libraries.json`) and open the active `Db`.
3. Build `PDFolioApp` via `with_initial_file_and_session` (or equivalent constructor path).
4. Start iced with initial window size from session or style layout defaults.
5. On boot, if signed in: open startup file or restore last document; schedule startup probe when `PDF_FOLIO_STARTUP_PROBE` is set.
6. After first frame, `StartupBackgroundReady` enables heavier subscriptions (thumbnails, registry sync).

Startup is deliberately staged so the first paint is not blocked by network or full library thumbnail fan-out.

```text
main
  → tracing init
  → clap parse
  → pdf_folio_ui::run(file?)
       → load session / registry / db
       → iced::application(update, view, subscription)
       → first frames (light)
       → StartupBackgroundReady → heavy work enabled
```

## Message surface

`Message` is the single event vocabulary for the entire UI. Deep dive: [Message surface](messages.md).

It covers:

- Document open/render/find/zoom/outline
- Library selection, drag, folders, tags, bulk ops, export
- Dialogs and confirmations
- Command palette and context menus
- Sync sign-in, auto-sync ticks, remote-available signals
- Style reload and theme changes
- Multi-library registry create/rename/delete/switch

When adding a feature, prefer **extending an existing cluster** of variants (e.g. library bulk ops) over inventing a parallel channel. Domain updaters already pattern-match large subsets of the enum.

## Commands vs messages

- **Messages** are pure events: “user clicked X”, “task finished with Y”.
- **Commands** (`shell/commands.rs`) are helpers that interpret intent into tasks or sequences of state changes (e.g. menu action → confirmation → bulk task).

Menus and the command palette typically emit messages or resolve `CommandId`s; some resolve through command helpers that produce `Task`s. Keep business rules out of the menu builder itself.

## Shortcuts

`shortcuts.rs` maps key chords to messages (or command ids). When adding a shortcut:

1. Decide the message the key should produce (same as the toolbar/menu path when possible).
2. Register the binding in the shortcut table.
3. Ensure the updater handles that message in the relevant mode (and ignore it harmlessly otherwise).

Platform modifiers (Ctrl vs Super) follow iced’s keyboard types; test on Wayland.

## Subscriptions

Background event sources become messages through iced subscriptions (`shell/subscriptions.rs`):

| Source | Typical message |
| --- | --- |
| Keyboard | Shortcut → mapped message |
| Style file watch | Style reload request |
| Library folder `notify` | `LibraryWatchEvent` wrapper |
| Auto-sync timer | Tick → maybe start sync task |
| Cursor / window | Position for drag and menus |

Subscriptions are pure producers. Enable expensive ones only after startup readiness when possible.

## Session and auth

`SyncAuthRuntime` (in `session.rs`) gates access when sync is configured:

| State | UI effect |
| --- | --- |
| `SignedOut` | Sign-in surface |
| `SigningIn` | In-progress |
| `SignedIn { email, expires_at }` | Full app |
| `WrongAccount` | Rejected identity vs allow-list expectation |

Session cache for the desktop app and cloud sync session files live under the XDG data directory — see [Data directories](../operations/data-dirs.md).

`AppSession` restores window size, mode, and last document path so relaunch feels continuous. Prefer small, version-tolerant JSON; migrations should tolerate missing fields.

## Chrome

`ChromeRuntime` holds UI that spans library and viewer:

- Pending confirmation modals (`ConfirmationAction`)
- Open context menu target + position
- Command palette query/selection
- Live cursor position (for context menus and drag)

Context menu targets include library entries, folders, tags, background, and the viewer canvas (`ContextMenuTarget` in `messages.rs`).

## Appearance

`AppearanceRuntime` holds the loaded `StyleBook` and theme selection. Style reload:

1. User edits KDL or triggers Reload Styles.
2. Shell loads a new book (bundled + user overrides).
3. On success, replace `appearance`; on failure, keep previous book and surface an error string.

See [Style system](../subsystems/style-system.md).

## Platform helpers

`platform.rs` isolates Linux-specific behaviors (e.g. revealing a file in the file manager). Keep OS branching here so library/viewer modules stay portable in spirit even though the product targets Linux first.

## Related

- [Runtime state](state.md)
- [Message surface](messages.md)
- [UI crate map](../crates/ui.md)
- [Sync subsystem](../subsystems/sync.md)
