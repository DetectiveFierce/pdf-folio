//! # UI components
//!
//! Presentation building blocks used by domain view modules. Prefer keeping
//! these helpers free of `Db` access and multi-step app mutation.
//!
//! - [`library`] — library cards, drag math, filters, dialogs, selection
//! - [`shared`] — shell chrome (menus, command palette, banners, icons)
//! - [`viewer`] — PDF viewer toolbar, canvas, outline, zoom, find
//!
//! Domain modules under `crate::library` and the shell compose these widgets
//! into full screens and own the `Message` routing.

pub(crate) mod library;
pub(crate) mod shared;
pub(crate) mod viewer;
