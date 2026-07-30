//! # Shared application chrome
//!
//! Cross-mode widgets: app menus, command palette, context menus, error
//! banners, loading overlays, icons, and sync status. Used by the shell root
//! surface and by library/viewer domains.

pub(crate) mod command_palette;
pub(crate) mod context_menu;
pub(crate) mod error_banner;
pub(crate) mod icons;
pub(crate) mod loading;
pub(crate) mod menus;
pub(crate) mod root_surface;
pub(crate) mod sidebar;
pub(crate) mod sync_status;
