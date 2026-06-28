//! Application theme selection.

use iced::Color;

use crate::style::ThemeTokens;

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
                surface_raised: Color::from_rgb8(247, 249, 252),
                text_primary: Color::from_rgb8(25, 31, 42),
                text_secondary: Color::from_rgb8(92, 101, 116),
                accent: Color::from_rgb8(43, 112, 197),
                border: Color::from_rgb8(207, 213, 224),
                error: Color::from_rgb8(176, 48, 64),
                canvas: Color::from_rgb8(224, 228, 235),
                placeholder: Color::from_rgb8(204, 211, 221),
                focus: Color::from_rgb8(43, 112, 197),
                shadow: Color::from_rgba8(0, 0, 0, 0.20),
            },
            Self::Dark => ThemeTokens {
                background: Color::from_rgb8(24, 24, 24),
                surface: Color::from_rgb8(32, 32, 32),
                surface_raised: Color::from_rgb8(40, 40, 40),
                text_primary: Color::from_rgb8(228, 228, 228),
                text_secondary: Color::from_rgb8(153, 153, 153),
                accent: Color::from_rgb8(102, 102, 102),
                border: Color::from_rgb8(46, 46, 46),
                error: Color::from_rgb8(217, 64, 64),
                canvas: Color::from_rgb8(24, 24, 24),
                placeholder: Color::from_rgb8(40, 40, 40),
                focus: Color::from_rgb8(68, 68, 68),
                shadow: Color::from_rgba8(0, 0, 0, 0.50),
            },
        }
    }
}
