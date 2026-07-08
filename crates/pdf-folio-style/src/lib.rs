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
//! - [`StyleBook::layout`] exposes KDL-backed layout values such as window size,
//!   card grid dimensions, and scroll increment.
//!
//! Bundled IBM Plex Sans and Vollkorn font bytes are re-exported so the UI
//! crate can register them with iced at startup.

pub mod book;
pub mod borders;
pub mod classes;
pub mod components;
pub mod theme;
pub mod tokens;

pub const IBM_PLEX_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
pub const IBM_PLEX_SANS_MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
pub const IBM_PLEX_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
pub const IBM_PLEX_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");
pub const VOLLKORN_REGULAR: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Regular.ttf");
pub const VOLLKORN_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Medium.ttf");
pub const VOLLKORN_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Vollkorn-SemiBold.ttf");
pub const VOLLKORN_BOLD: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Bold.ttf");
pub const VOLLKORN_ITALIC: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Italic.ttf");
pub const VOLLKORN_MEDIUM_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/Vollkorn-MediumItalic.ttf");
pub const VOLLKORN_SEMIBOLD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/Vollkorn-SemiBoldItalic.ttf");
pub const VOLLKORN_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/Vollkorn-BoldItalic.ttf");

/// All application fonts embedded into the executable and registered with iced.
pub const BUNDLED_FONT_BYTES: &[&[u8]] = &[
    IBM_PLEX_SANS_REGULAR,
    IBM_PLEX_SANS_MEDIUM,
    IBM_PLEX_SANS_SEMIBOLD,
    IBM_PLEX_SANS_BOLD,
    VOLLKORN_REGULAR,
    VOLLKORN_MEDIUM,
    VOLLKORN_SEMIBOLD,
    VOLLKORN_BOLD,
    VOLLKORN_ITALIC,
    VOLLKORN_MEDIUM_ITALIC,
    VOLLKORN_SEMIBOLD_ITALIC,
    VOLLKORN_BOLD_ITALIC,
];

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
pub use borders::side_border;
pub use tokens::{
    display_font, ui_font, AppLabelTokens, AppLayoutTokens, BorderWidth, BoxShadow,
    ContentAlignment, FontSize, FontWeight, IconSize, LabelSection, Radius, Spacing, TextAlignment,
    ThemeTokens, DISPLAY_FONT_FAMILY, UI_FONT_FAMILY,
};

#[cfg(test)]
mod tests {
    use super::*;
    use fontdb::{Database, Family, Query, Stretch, Style, Weight};
    use std::sync::Arc;

    #[test]
    fn bundled_fonts_include_display_family_weights() {
        let mut db = Database::new();

        for font in BUNDLED_FONT_BYTES {
            db.load_font_source(fontdb::Source::Binary(Arc::new(font.to_vec())));
        }

        for style in [Style::Normal, Style::Italic] {
            for weight in [
                Weight::NORMAL,
                Weight::MEDIUM,
                Weight::SEMIBOLD,
                Weight::BOLD,
            ] {
                let query = Query {
                    families: &[Family::Name(DISPLAY_FONT_FAMILY)],
                    weight,
                    stretch: Stretch::Normal,
                    style,
                };

                let id = db.query(&query).unwrap_or_else(|| {
                    panic!("missing embedded display font style {style:?} weight {weight:?}")
                });
                let face = db.face(id).expect("fontdb face for queried font");

                assert!(
                    face.families
                        .iter()
                        .any(|(family, _)| family == DISPLAY_FONT_FAMILY),
                    "display font resolved to {:?}",
                    face.families
                );
            }
        }
    }
}
