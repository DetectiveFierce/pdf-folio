//! Viewer view composition modules.
//!
//! Pure UI assembly for viewer mode: the root layout wires toolbar, optional
//! sidebar, error banner, jump dialog, and document canvas. Presentational
//! building blocks come from `components::viewer`; this module only composes
//! them against [`crate::PDFolioApp`] state.
//!
//! - [`root`] — full viewer surface (`view_viewer`).
//! - [`document`] — scrollable canvas + find bar overlay.
//!
//! Message emission stays in the widgets; this layer does not implement
//! update logic.

pub(crate) mod document;
pub(crate) mod root;

pub(crate) use root::view_viewer;
