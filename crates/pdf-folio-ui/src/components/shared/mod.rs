//! # Shared application chrome
//!
//! Cross-mode widgets under `components::shared`: app menus, command palette,
//! context menus, error banners, loading overlays, icons, sync status, and
//! the root surface compositor. Used by the shell and by library/viewer
//! domains that need chrome outside a single mode.
//!
//! ## Notable modules
//!
//! - [`root_surface`] — top-level mode switch and overlay stack
//! - [`command_palette`] / [`context_menu`] / [`menus`] — command and menu UI
//! - [`loading`] / [`error_banner`] / [`sync_status`] — status feedback
//! - [`icons`] / [`sidebar`] — shared glyphs and sidebar chrome helpers

/// Keyboard-driven command overlay with capture layer and searchable list.
pub(crate) mod command_palette;
/// Right-click menus for library entries, folders, tags, and the viewer canvas.
pub(crate) mod context_menu;
/// Full-width dismissible error strip for shell/domain failures.
pub(crate) mod error_banner;
/// Shared SVG icon byte constants for chevrons, undo/redo, layout, and trash.
pub(crate) mod icons;
/// Blocking full-surface spinners for restore, document open, and startup.
pub(crate) mod loading;
/// Multi-library switcher grid and overflow menu glyphs.
pub(crate) mod menus;
/// Top-level app mode switch and global overlay stack compositor.
pub(crate) mod root_surface;
/// Shared sidebar scroll, color, chevron, and action-button helpers.
pub(crate) mod sidebar;
/// Cloud sync toolbar indicator (spinner, check, queued ellipsis).
pub(crate) mod sync_status;
