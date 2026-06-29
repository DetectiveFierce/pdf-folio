//! Application theme selection.

use crate::style::{fallback_dark_tokens, fallback_light_tokens, StyleBook, ThemeTokens};

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

    /// Stable style-book theme id.
    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "espresso",
        }
    }

    /// Returns resolved tokens from the active style book.
    pub fn tokens(self, style_book: &StyleBook) -> ThemeTokens {
        style_book.tokens(self.id())
    }

    /// Returns built-in fallback tokens without reading style files.
    pub fn fallback_tokens(self) -> ThemeTokens {
        match self {
            Self::Light => fallback_light_tokens(),
            Self::Dark => fallback_dark_tokens(),
        }
    }
}
