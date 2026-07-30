//! # Library presentation helpers
//!
//! Pure and lightly app-aware widgets under `components::library`. Heavy
//! message handling and `Db` tasks stay in
//! `crate::library::{update, actions, tasks}`.
//!
//! ## Notable modules
//!
//! - [`drag`] / [`selection`] — pure drag and multi-select math (re-exported
//!   into the library domain)
//! - [`filters`] / [`metadata`] / [`state`] — matching, display formatting,
//!   and density/filter enums
//! - [`dialogs`] — modal import/export/Raindrop/confirmation UIs
//! - [`view`] / [`cards`] — reusable toolbar and card chrome
//! - [`folder_tree`] / [`inspector`] / [`import_status`] — structured panels

/// Reusable card chrome: previews, drop markers, and tag pill rows.
pub(crate) mod cards;
/// Modal import/export/Raindrop/confirmation dialogs for library mode.
pub(crate) mod dialogs;
/// Pure drag/drop geometry, dwell timing, and reorder helpers.
pub mod drag;
/// Free-text search matching, reading-state buckets, and folder-scope visibility.
pub mod filters;
/// Indented folder tree rows and fold chevrons for the library sidebar.
pub(crate) mod folder_tree;
/// Bulk-operation and Raindrop import progress banners/modals.
pub(crate) mod import_status;
/// Right-hand inspector pane for selected entries, folders, tags, or summary.
pub(crate) mod inspector;
/// Pure display formatting for entry titles, sizes, progress, and density labels.
pub mod metadata;
/// Multi-select ranges, checkbox tri-state, and drag-reorder list splicing.
pub mod selection;
/// Metadata-density and reading-filter enums shared with domain state.
pub mod state;
/// Message-generic scrollable shell, layout toggle, zoom, and pick lists.
pub mod view;
