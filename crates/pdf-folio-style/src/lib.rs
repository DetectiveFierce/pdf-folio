//! Design system for PDF-Folio: KDL-backed themes, semantic classes, and
//! reusable iced chrome.
//!
//! Views in `pdf-folio-ui` should describe structure and messages only. Colors,
//! radii, borders, shadows, spacing, layout metrics, and repeated widget chrome
//! flow through this crate so light/dark themes share the same token names and
//! designers can tweak palettes without recompiling Rust.
//!
//! # Modules
//!
//! | Module | Role |
//! | --- | --- |
//! | [`book`] | Load/parse/validate the style book from KDL sources |
//! | [`tokens`] | `ThemeTokens`, `Spacing`, `FontSize`, layout/label structs |
//! | [`classes`] | `Class` → iced stylesheet closures (`button_style`, …) |
//! | [`components`] | Widget builders (`toolbar_button`, `library_card`, …) |
//! | [`borders`] | Per-side border drawing beyond iced's uniform borders |
//! | [`theme`] | [`AppTheme`] bridge (`Light` / `Dark` → theme id) |
//!
//! # KDL book pipeline
//!
//! ```text
//! bundled KDL  (+ on-disk styles/ in a source checkout)
//!         │
//!         ▼
//!   StyleBook::load()
//!         ├── themes  → ThemeTokens (espresso, light)
//!         ├── components → ClassStyle per ComponentState
//!         └── application layout / labels
//!         │
//!         ▼
//!   user overrides: $XDG_CONFIG_HOME/pdf-folio/styles/**/*.kdl
//!         │
//!         ▼
//!   Arc<StyleBook> held by the UI appearance runtime
//! ```
//!
//! - [`StyleBook::load`] merges bundled sources with any user KDL under the XDG
//!   config `styles/` tree. Failed reloads keep the previous book active.
//! - [`StyleBook::bundled`] loads only embedded sources (tests, fallbacks).
//! - [`StyleBook::style_dirs`] lists directories the shell watches for hot reload
//!   (View → Reload Styles / Ctrl+Shift+R and filesystem watch).
//!
//! # File map under `styles/`
//!
//! ```text
//! styles/
//!   application.kdl                      # window size, virtualization, menus
//!   themes/espresso.kdl                  # dark palette (AppTheme::Dark)
//!   themes/light.kdl                     # light palette (AppTheme::Light)
//!   components/core.kdl                  # shell chrome classes
//!   components/library/library.kdl       # cards, rows, control bar
//!   components/library/sidebar.kdl       # library sidebar tree chrome
//!   components/viewer/viewer.kdl         # viewer toolbar, canvas, find bar
//! ```
//!
//! Embedded copies of these files are compiled in via `include_str!`. In a
//! development checkout, on-disk files under `crates/pdf-folio-style/styles/`
//! take precedence so palette edits hot-reload without a rebuild.
//!
//! # How the UI uses this crate
//!
//! 1. Resolve tokens: `app_theme.tokens(&style_book)` → [`ThemeTokens`].
//! 2. Paint with class stylesheets: `button_style(tokens, Class::ToolbarButton, status)`.
//! 3. Prefer component helpers when chrome is repeated: `toolbar_button("Open", tokens)`.
//! 4. Read layout metrics from `style_book.layout()` (sidebar widths, card sizes, …).
//!
//! Helpers must **not** read `PDFolioApp`, the database, or document state — only
//! labels, tokens, and message callbacks.
//!
//! # Fonts
//!
//! IBM Plex Sans (UI) and Vollkorn (display) are embedded as bytes and re-exported
//! so the UI crate can register them with iced at startup. Prefer
//! [`ui_font`] / [`display_font`] over ad-hoc font picks.

pub mod book;
pub mod borders;
pub mod classes;
pub mod components;
pub mod theme;
pub mod tokens;

/// Embedded TrueType bytes for IBM Plex Sans Regular (primary UI face).
pub const IBM_PLEX_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf");
/// Embedded TrueType bytes for IBM Plex Sans Medium.
pub const IBM_PLEX_SANS_MEDIUM: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf");
/// Embedded TrueType bytes for IBM Plex Sans SemiBold.
pub const IBM_PLEX_SANS_SEMIBOLD: &[u8] =
    include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf");
/// Embedded TrueType bytes for IBM Plex Sans Bold.
pub const IBM_PLEX_SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSans-Bold.ttf");
/// Embedded TrueType bytes for Vollkorn Regular (display / brand face).
pub const VOLLKORN_REGULAR: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Regular.ttf");
/// Embedded TrueType bytes for Vollkorn Medium.
pub const VOLLKORN_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Medium.ttf");
/// Embedded TrueType bytes for Vollkorn SemiBold.
pub const VOLLKORN_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Vollkorn-SemiBold.ttf");
/// Embedded TrueType bytes for Vollkorn Bold.
pub const VOLLKORN_BOLD: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Bold.ttf");
/// Embedded TrueType bytes for Vollkorn Italic.
pub const VOLLKORN_ITALIC: &[u8] = include_bytes!("../assets/fonts/Vollkorn-Italic.ttf");
/// Embedded TrueType bytes for Vollkorn Medium Italic.
pub const VOLLKORN_MEDIUM_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/Vollkorn-MediumItalic.ttf");
/// Embedded TrueType bytes for Vollkorn SemiBold Italic.
pub const VOLLKORN_SEMIBOLD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/Vollkorn-SemiBoldItalic.ttf");
/// Embedded TrueType bytes for Vollkorn Bold Italic.
pub const VOLLKORN_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/Vollkorn-BoldItalic.ttf");

/// Every application font embedded into the executable for iced registration.
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
pub use borders::side_border;
pub use classes::{
    button_style, container_style, menu_style_for_class, mix_color, pick_list_style,
    progress_bar_style, scrollable_style, side_border_for_class, side_border_for_style,
    sidebar_scrollable_style, slider_style, text_input_style, viewer_primitives, Class,
    ComponentState, Shadow, ViewerPrimitiveStyle, VisualOverride,
};
pub use components::{
    aligned_text, empty_state, icon_button, master_checkbox, progress_bar, search_input_with_class,
    section_heading, selection_checkbox, tag_pill, toc_entry, toolbar_button, MasterCheckboxState,
};
pub use tokens::{
    display_font, ui_font, AppLabelTokens, AppLayoutTokens, BorderWidth, BoxShadow,
    ContentAlignment, FontSize, FontWeight, LabelSection, Radius, Spacing, TextAlignment,
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
