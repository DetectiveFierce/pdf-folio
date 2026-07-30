//! # Library domain (UI orchestration)
//!
//! This module owns the **library mode** half of the iced application: how folder
//! trees, selection, filters, drag/drop, thumbnails, and multi-library switching
//! are updated and rendered. It is the domain layer that sits between the shell
//! message loop and the pure presentation helpers under
//! [`crate::components::library`].
//!
//! ## Ownership split
//!
//! - **Domain (this module tree):** mutates `PDFolioApp` / `app.library`, builds
//!   iced `Task`s for SQLite and filesystem work, and composes view trees that
//!   know about app state. Files such as [`update`], [`actions`], and [`tasks`]
//!   are the main entry points.
//! - **Pure helpers (re-exported from components):** drag geometry, selection
//!   set math, filter predicates, and metadata formatting live in
//!   `components::library::{drag, filters, metadata, selection}` and are
//!   re-exported here so domain code can call them without reaching into the
//!   component crate path on every use.
//!
//! ## How the pieces fit together
//!
//! 1. **[`update`]** — library-domain `Message` handler. Returns
//!    `Option<Task<Message>>` when a message is claimed; `None` leaves it for
//!    other domains (viewer, shell).
//! 2. **[`actions`]** — high-level intents on `PDFolioApp` (selection, clipboard,
//!    drag lifecycle, folder navigation, history). Called from `update` and
//!    command routing.
//! 3. **[`tasks`]** — async/blocking constructors that talk to `pdf_folio_core::Db`
//!    and the filesystem; results come back as `Message` variants.
//! 4. **[`view`]** — library-mode composition (root pane, sidebar, entry grid,
//!    folder cards). Uses widgets from `components::library` for reusable chrome.
//! 5. **[`registry`]** — multi-vault profiles (`libraries.json`), switch/create/
//!    rename/delete, and switcher previews.
//!
//! Supporting modules:
//! - [`state`] — viewport windowing, masonry layout, hover/drop flash animations
//! - [`data`] — derived lists (tags, visible entries refresh, thumbnail requests)
//! - [`layout`] — zoom limits, filtered/sorted visible entry lists, scroll geometry
//! - [`thumbnails`] — cache keys, disk/async render of cover previews
//!
//! Prefer keeping pure geometry and string formatting in components; keep anything
//! that needs `Db`, session persistence, or multi-field app mutation here.

pub(crate) use crate::components::library::{drag, filters, metadata, selection};
/// Imperative methods on `PDFolioApp` for selection, drag, clipboard, and history.
pub(crate) mod actions;
/// Derived library lists (tags, visible-entry refresh, thumbnail request windows).
pub(crate) mod data;
/// Zoom limits, filtered/sorted visible entries, and scroll geometry.
pub(crate) mod layout;
/// Multi-vault profiles (`libraries.json`), switch/create/rename/delete, and previews.
pub mod registry;
/// Viewport windowing, masonry layout, and hover/drop flash animations.
pub(crate) mod state;
/// Async/blocking DB and filesystem tasks that emit library `Message`s.
pub(crate) mod tasks;
/// Cover thumbnail cache keys and disk/async render helpers.
pub(crate) mod thumbnails;
/// Library-domain `Message` handler (`Option<Task>` when claimed).
pub(crate) mod update;
/// Library-mode view composition (root pane, sidebar, entry grid, folder cards).
pub(crate) mod view;
