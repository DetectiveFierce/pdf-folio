//! # UI components
//!
//! Presentation building blocks under `pdf_folio_ui::components`, used by
//! domain view modules. Prefer keeping these helpers free of `Db` access and
//! multi-step app mutation; emit `Message`s or take generic message parameters
//! instead.
//!
//! ## Subtrees
//!
//! - [`library`] — library cards, drag math, filters, dialogs, selection,
//!   inspector, and toolbar widgets (`components::library::*`)
//! - [`shared`] — shell chrome: menus, command palette, context menus,
//!   banners, loading overlays, icons, sync indicator, root surface
//! - [`viewer`] — PDF viewer toolbar, canvas, outline, zoom, find, page
//!   controls, and sidebar
//!
//! Domain modules under `crate::library` / `crate::viewer` and the shell
//! compose these widgets into full screens and own `Message` routing.

/// Library cards, dialogs, drag math, filters, inspector, and toolbar widgets.
pub(crate) mod library;
/// Shell chrome: menus, command palette, context menus, banners, loading, icons.
pub(crate) mod shared;
/// PDF viewer toolbar, canvas, outline, zoom, find bar, and sidebar.
pub(crate) mod viewer;
