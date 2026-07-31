//! # PDF viewer chrome
//!
//! Presentation widgets under `components::viewer` for document viewing:
//! canvas, toolbar, page controls, find bar, outline/thumbnail sidebar, and
//! zoom menus. Document loading, page raster cache, and scroll/zoom math live
//! outside this tree in the viewer domain and shell.
//!
//! ## Notable modules
//!
//! - [`canvas`] — page paint, selection overlay, spinner, wheel handling
//! - [`toolbar`] / [`zoom`] / [`page_controls`] — top chrome and menus
//! - [`sidebar`] / [`outline`] / [`annotations`] — contents, thumbnails, document-anchored notes
//! - [`find_bar`] — in-document text search strip

/// Continuous page paint, selection overlay, spinner, and wheel handling.
pub(crate) mod canvas;
/// Document-anchored annotation cards (scroll with content; collision layout).
pub(crate) mod annotations;
/// Floating find-in-document strip (query, matches, toggles).
pub(crate) mod find_bar;
/// Hierarchical table-of-contents list for the Contents sidebar tab.
pub(crate) mod outline;
/// Prev/next page chevrons, page number edit, and jump-to-page dialog.
pub(crate) mod page_controls;
/// Viewer Contents/Thumbnails sidebar shell and tab bodies.
pub(crate) mod sidebar;
/// Top viewer toolbar, zoom dropdown host, and floating sidebar toggle.
pub(crate) mod toolbar;
/// Zoom percent readout, preset menu, and chevron disclosure control.
pub(crate) mod zoom;
