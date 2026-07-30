//! # PDF viewer chrome
//!
//! Presentation widgets for document viewing: canvas, toolbar, page controls,
//! find bar, outline sidebar, and zoom menus. Document loading and page cache
//! live outside this module in the viewer domain/shell.

pub(crate) mod canvas;
pub(crate) mod find_bar;
pub(crate) mod outline;
pub(crate) mod page_controls;
pub(crate) mod sidebar;
pub(crate) mod toolbar;
pub(crate) mod zoom;
