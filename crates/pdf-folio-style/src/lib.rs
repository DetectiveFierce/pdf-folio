//! Visual styling, theming, and layout tokens for PDF-Folio.
//!
//! This crate provides the design-system foundation for the application:
//!
//! - [`StyleBook`] loads theme definitions from KDL files (or falls back to
//!   bundled defaults) and exposes resolved [`ThemeTokens`] for light and dark
//!   modes.
//! - [`Class`] and the `*_style` functions produce iced stylesheet closures
//!   for buttons, containers, scrollables, text inputs, and more.
//! - [`components`] supplies reusable styled widget builders (library cards,
//!   toolbars, tags, icons) that close over theme tokens.
//! - [`tokens`] defines strongly-typed spacing, font-size, radius, and
//!   weight constants so the rest of the app avoids magic numbers.
//! - [`layout`] holds shared layout constants such as window size, card grid
//!   dimensions, and scroll increment.
//!
//! Bundled IBM Plex Sans font bytes are re-exported so the UI crate can
//! register them with iced at startup.

pub mod book;
pub mod classes;
pub mod components;
pub mod layout;
pub mod side_border;
pub mod theme;
pub mod tokens;

pub const IBM_PLEX_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
pub const IBM_PLEX_SANS_MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
pub const IBM_PLEX_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
pub const IBM_PLEX_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");

pub use book::{fallback_dark_tokens, fallback_light_tokens, StyleBook};
pub use classes::{
    button_style, container_style, menu_style, menu_style_for_class, mix_color, pick_list_style,
    progress_bar_style, scrollable_style, side_border_for_class, side_border_for_style,
    sidebar_scrollable_style, slider_style, text_input_style, viewer_primitives, Class,
    ComponentState, Shadow, ViewerPrimitiveStyle, VisualOverride,
};
pub use components::{
    align_content_x, align_content_y, aligned_text, annotation_popover, annotation_toolbar,
    empty_state, error_banner, icon_button, library_card, library_row, master_checkbox,
    progress_bar, search_input, search_input_with_class, section_heading, selection_checkbox,
    sidebar_button, tag_pill, toc_entry, toolbar_button, MasterCheckboxState,
};
pub use layout::{
    CARD_GRID_COLUMNS, LIBRARY_GRID_CARD_WIDTH, LIBRARY_OVERSCAN_ROWS, LINE_SCROLL_PIXELS,
    WINDOW_SIZE,
};
pub use side_border::side_border;
pub use tokens::{
    display_font, ui_font, AppLabelTokens, AppLayoutTokens, BorderWidth, ContentAlignment,
    FontSize, FontWeight, IconSize, LabelSection, Radius, Spacing, TextAlignment, ThemeTokens,
    DISPLAY_FONT_FAMILY, UI_FONT_FAMILY,
};
