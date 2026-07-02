//! Shared style system for PDF-Folio UI surfaces.

pub mod book;
pub mod classes;
pub mod components;
pub mod layout;
pub mod side_border;
pub mod tokens;

pub use book::{fallback_dark_tokens, fallback_light_tokens, StyleBook};
pub use classes::{
    button_style, container_style, menu_style, menu_style_for_class, mix_color, pick_list_style,
    progress_bar_style, scrollable_style, side_border_for_class, side_border_for_style,
    slider_style, text_input_style, viewer_primitives, Class, ComponentState, Shadow,
    ViewerPrimitiveStyle, VisualOverride,
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
