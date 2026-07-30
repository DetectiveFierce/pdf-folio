//! Application shell: root state, messages, update, session, and platform glue.
//!
//! The shell is the orchestration layer for `pdf-folio-ui`. It owns types that
//! span library and viewer modes, the single [`messages::Message`] vocabulary,
//! and the top-level iced reducers that domain modules do not claim.
//!
//! # Module map
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`app`] | [`PDFolioApp`], [`AppMode`], runtimes, library chrome structs |
//! | [`messages`] | `Message` enum and context-menu / confirmation / shortcut types |
//! | [`update`] | Top-level reducer; delegates library/viewer first |
//! | [`commands`] | Command palette / menu registry (`CommandId` → enabled/message) |
//! | [`shortcuts`] | Keyboard binding → `Message` / `Shortcut` handling |
//! | [`subscriptions`] | iced subscription tree (watchers, sync ticks, input) |
//! | [`session`] | `AppSession` persistence and `SyncAuthRuntime` |
//! | [`tasks`] | Shell-owned async work (registry sync fan-out) |
//! | [`platform`] | OS file-manager reveal helpers |
//! | [`constants`] | Widget ids, animation timings, shared option lists |
//!
//! # Message ownership
//!
//! Domain updaters (`library::update`, `viewer::update`) return `Some(task)`
//! when they handle a message. Shell update only matches leftovers: sync
//! auth, multi-library switcher chrome, file dialogs, theme/style reload,
//! context menus, command palette, and cross-mode navigation helpers.
//!
//! When adding a feature, extend an existing message cluster in [`messages`]
//! and handle it in the owning domain updater when possible.

use crate::*;

/// Root application state: [`PDFolioApp`], modes, and chrome runtimes.
pub mod app;
/// Command palette / menu registry and enablement helpers.
pub(crate) mod commands;
/// Widget ids, animation timings, and shared option lists.
pub(crate) mod constants;
/// Crate-wide [`Message`] vocabulary and related enums.
pub mod messages;
/// OS file-manager reveal and path URI helpers.
pub(crate) mod platform;
/// Session persistence and Google sync auth runtime.
pub(crate) mod session;
/// Keyboard binding → [`Message`] / shortcut handling.
pub(crate) mod shortcuts;
/// iced subscription tree (watchers, sync ticks, input).
pub(crate) mod subscriptions;
/// Shell-owned async work (registry sync fan-out, auto-sync).
pub(crate) mod tasks;
/// Top-level iced reducer; delegates library/viewer first.
pub(crate) mod update;
