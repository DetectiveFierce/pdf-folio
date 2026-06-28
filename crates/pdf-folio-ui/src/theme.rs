//! Application theme tokens.

use iced::Color;

/// Supported visual themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTheme {
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
}

impl AppTheme {
    /// Returns the opposite theme.
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    /// Returns the color tokens for this theme.
    pub fn tokens(self) -> ThemeTokens {
        match self {
            Self::Light => ThemeTokens {
                background: Color::from_rgb8(238, 240, 244),
                surface: Color::WHITE,
                text_primary: Color::from_rgb8(25, 31, 42),
                text_secondary: Color::from_rgb8(92, 101, 116),
                accent: Color::from_rgb8(43, 112, 197),
                border: Color::from_rgb8(207, 213, 224),
                canvas: Color::from_rgb8(224, 228, 235),
                placeholder: Color::from_rgb8(204, 211, 221),
            },
            Self::Dark => ThemeTokens {
                background: Color::from_rgb8(22, 25, 31),
                surface: Color::from_rgb8(32, 37, 47),
                text_primary: Color::from_rgb8(238, 241, 246),
                text_secondary: Color::from_rgb8(157, 166, 181),
                accent: Color::from_rgb8(104, 166, 255),
                border: Color::from_rgb8(63, 71, 86),
                canvas: Color::from_rgb8(34, 38, 46),
                placeholder: Color::from_rgb8(89, 98, 114),
            },
        }
    }
}

/// Theme color tokens used by PDF-Folio views.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    /// Window background.
    pub background: Color,
    /// Toolbar and sidebar surface.
    pub surface: Color,
    /// Primary text color.
    pub text_primary: Color,
    /// Secondary text color.
    pub text_secondary: Color,
    /// Accent color for active controls.
    pub accent: Color,
    /// Border color.
    pub border: Color,
    /// Viewer canvas background.
    pub canvas: Color,
    /// Placeholder page fill.
    pub placeholder: Color,
}
