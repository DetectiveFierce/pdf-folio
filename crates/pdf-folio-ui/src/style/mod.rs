//! Shared style system for PDF-Folio UI surfaces.

pub mod classes;
pub mod components;
pub mod layout;
pub mod tokens;

pub use classes::{
    button_style, container_style, menu_style, mix_color, pick_list_style, progress_bar_style,
    scrollable_style, text_input_style, viewer_primitives, Class, ComponentState, Shadow,
    ViewerPrimitiveStyle,
};
pub use components::{
    align_content_x, align_content_y, aligned_text, annotation_popover, annotation_toolbar,
    empty_state, error_banner, icon_button, library_card, library_row, progress_bar, search_input,
    section_heading, sidebar_button, tag_pill, toc_entry, toolbar_button,
};
pub use layout::{CARD_GRID_COLUMNS, LIBRARY_OVERSCAN_ROWS, LINE_SCROLL_PIXELS, WINDOW_SIZE};
pub use tokens::{
    BorderWidth, ContentAlignment, FontSize, FontWeight, IconSize, Radius, Spacing, TextAlignment,
    ThemeTokens,
};
