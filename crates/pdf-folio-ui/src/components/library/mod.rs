//! # Library presentation helpers
//!
//! Pure and lightly app-aware widgets for library UI. Heavy message handling
//! and `Db` tasks stay in `crate::library::{update, actions, tasks}`.
//!
//! ## Notable modules
//!
//! - [`drag`] / [`selection`] — pure drag and multi-select math (re-exported
//!   into the library domain)
//! - [`filters`] / [`metadata`] — matching and display formatting
//! - [`dialogs`] — modal import/export/Raindrop/confirmation UIs
//! - [`view`] / [`cards`] — reusable toolbar and card chrome
//! - [`folder_tree`] / [`inspector`] / [`import_status`] — structured panels

pub(crate) mod cards;
pub(crate) mod dialogs;
pub mod drag;
pub mod filters;
pub(crate) mod folder_tree;
pub(crate) mod import_status;
pub(crate) mod inspector;
pub mod metadata;
pub mod selection;
pub mod state;
pub mod view;
