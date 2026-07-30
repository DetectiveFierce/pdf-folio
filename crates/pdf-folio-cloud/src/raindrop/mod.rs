//! Raindrop.io import support for PDF-Folio.
//!
//! This crate keeps Raindrop-specific HTTP, OAuth, download, and metadata
//! mirroring logic out of the UI and database crates.

pub(crate) const API_BASE: &str = "https://api.raindrop.io/rest/v1";
pub(crate) const MAX_PER_PAGE: u16 = 50;
pub(crate) const ZIP_IMPORT_THRESHOLD: usize = 12;
pub(crate) const ZIP_PREPARING_PROGRESS_BASIS_POINTS: u16 = 1_250;
pub(crate) const ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS: u16 = 3_750;
pub(crate) const ZIP_EXTRACTED_PROGRESS_BASIS_POINTS: u16 = 5_000;

mod auth;
mod client;
mod import;
mod matching;
mod types;
pub use auth::OAUTH_CALLBACK_URL;
use auth::{bundled_or_env_oauth_config, cached_access_token};
pub use import::{
    import_all_pdfs, import_all_pdfs_with_auth, import_preview, import_preview_pdfs_with_progress,
    import_preview_with_auth, import_selected_pdfs, import_selected_pdfs_with_auth,
    import_selected_pdfs_with_progress,
};
pub use types::*;

/// Returns true when the importer can run without asking the user for OAuth app credentials.
pub fn can_import_without_prompt() -> bool {
    std::env::var("PDF_FOLIO_RAINDROP_TOKEN")
        .ok()
        .is_some_and(|token| !token.trim().is_empty())
        || cached_access_token().is_ok_and(|token| !token.trim().is_empty())
        || bundled_or_env_oauth_config().is_some()
}

#[cfg(test)]
mod tests;
