//! PDF viewer subsystem: open document runtime, render pipeline, and UI composition.
//!
//! Owns everything needed while a document is open: page tiles, zoom/scroll
//! modes, text selection and find-in-document, outline expand state, and the
//! async tasks that load PDFs and rasterize pages. The shell mounts
//! [`document::ViewerRuntime`] on [`crate::PDFolioApp`] and delegates viewer
//! messages here first via [`update`].
//!
//! # Module map
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`document`] | [`ViewerRuntime`] fields for the open document |
//! | [`state`] | Scroll/spread modes, text selection, find state, app helpers |
//! | [`layout`] | Spread groups, page rects helpers, render-key selection |
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

pub(crate) mod document;
pub(crate) mod layout;
pub(crate) mod navigation;
pub(crate) mod rendering;
pub(crate) mod state;
pub(crate) mod tasks;
pub(crate) mod update;
pub(crate) mod view;
