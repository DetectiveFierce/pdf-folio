//! Application theme tokens.

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
}
