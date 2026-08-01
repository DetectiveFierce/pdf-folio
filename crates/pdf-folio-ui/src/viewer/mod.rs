//! PDF viewer subsystem: open document runtime, render pipeline, and UI composition.
//!
//! Owns everything needed while a document is open: page tiles, zoom/scroll
//! modes, text selection and find-in-document, outline expand state, debounced
//! reading progress, and the async tasks that load PDFs and rasterize pages.
//! The shell mounts [`document::ViewerRuntime`] on [`crate::PDFolioApp`] and
//! delegates viewer messages here first via [`update`].
//!
//! UX refinement plan: `scratch/viewer-ux-plan.md`. Pipeline notes:
//! `docs/content/subsystems/rendering.md`.
//!
//! # Module map
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`document`] | [`ViewerRuntime`] fields for the open document |
//! | [`state`] | Scroll/spread modes, text selection, find state, app helpers |
//! | [`layout`] | Spread groups, page rects helpers, render-key selection |
//! | [`annotation_layout`] | Anchored card placement + mark geometry (domain) |
//! | [`navigation`] | Jump/scroll/zoom methods on `PDFolioApp` |
//! | [`rendering`] | Zoom presets, percent math, render policy |
//! | [`tasks`] | Open document / render page / zoom debounce tasks |
//! | [`update`] | Viewer-domain message reducer |
//! | [`view`] | Root toolbar+sidebar+canvas composition |
//!
//! # Message ownership
//!
//! [`update`] handles zoom, viewport, page navigation, outline, find, text
//! selection, and `PageRendered` / text-layer results. Document open from the
//! library and shell file dialog still flows through library/shell update,
//! then viewer tasks/helpers finish initialization.
//!
//! Presentational widgets (canvas, toolbar, find bar) live under
//! `components::viewer` and are composed by [`view`].

/// Pure annotation card placement and mark geometry (domain, not widgets).
pub(crate) mod annotation_layout;
/// Open-document runtime fields mounted on `PDFolioApp` (`ViewerRuntime`).
pub(crate) mod document;
/// Spread groups, page rect helpers, and render-key selection.
pub(crate) mod layout;
/// Jump/scroll/zoom methods on `PDFolioApp` for the open document.
pub(crate) mod navigation;
/// Zoom presets, percent math, and render policy after width changes.
pub(crate) mod rendering;
/// Scroll/spread modes, text selection, find state, and related helpers.
pub(crate) mod state;
/// Open document / render page / zoom debounce iced tasks.
pub(crate) mod tasks;
/// Viewer-domain message reducer.
pub(crate) mod update;
/// Root toolbar + sidebar + canvas composition for viewer mode.
pub(crate) mod view;
