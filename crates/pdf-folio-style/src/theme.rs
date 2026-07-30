//! Application theme selection bridging UI state to style-book theme ids.
//!
//! The shell stores an [`AppTheme`] (user-facing light/dark preference). That
//! maps to a stable style-book id via [`AppTheme::id`]:
//!
//! | [`AppTheme`] | Style-book id | KDL file |
//! | --- | --- | --- |
//! | [`AppTheme::Light`] | `"light"` | `styles/themes/light.kdl` |
//! | [`AppTheme::Dark`] | `"espresso"` | `styles/themes/espresso.kdl` |
//!
//! Resolve colors with [`AppTheme::tokens`] against the loaded [`StyleBook`].
//! Use [`AppTheme::fallback_tokens`] only when a book is not available yet
//! (startup before load, or hard failure paths).

use crate::{fallback_dark_tokens, fallback_light_tokens, StyleBook, ThemeTokens};

/// User-facing visual theme preference (light or dark).
///
/// Maps to named palettes inside the style book; the dark preference uses the
/// `espresso` palette rather than a generic `"dark"` id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTheme {
    /// Light palette (`styles/themes/light.kdl`).
    Light,
    /// Dark espresso palette (`styles/themes/espresso.kdl`).
    Dark,
}

impl AppTheme {
    /// Returns the opposite theme (for a light/dark toggle).
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    /// Stable style-book theme id (`"light"` or `"espresso"`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "espresso",
        }
    }

    /// Resolves [`ThemeTokens`] for this preference from the active style book.
    pub fn tokens(self, style_book: &StyleBook) -> ThemeTokens {
        style_book.tokens(self.id())
    }

    /// Built-in fallback tokens without reading style files.
    ///
    /// Prefer [`Self::tokens`] once a [`StyleBook`] is loaded so KDL overrides apply.
    pub fn fallback_tokens(self) -> ThemeTokens {
        match self {
            Self::Light => fallback_light_tokens(),
            Self::Dark => fallback_dark_tokens(),
        }
    }
}
