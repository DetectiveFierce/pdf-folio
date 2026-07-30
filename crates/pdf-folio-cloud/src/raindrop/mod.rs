//! Raindrop.io **import** support for PDF-Folio (product three of three in this crate).
//!
//! This module talks to the Raindrop REST API: OAuth (or env/test bearer tokens),
//! collection listing, PDF discovery, individual downloads, and bulk ZIP export
//! matching. It keeps Raindrop-specific HTTP and download guards out of the UI
//! and database crates. Local provenance tables (`raindrop_collections`,
//! `raindrop_entries`) live in `pdf-folio-core::db::raindrop`; this crate only
//! performs remote I/O and drives import into a [`pdf_folio_core::Db`].
//!
//! # Key types
//!
//! Public DTOs (re-exported from [`types`]):
//!
//! - [`RaindropImportPreview`] / [`RaindropPdfCandidate`] — list remote PDFs before import
//! - [`RaindropImportDestination`] — preserve Raindrop folders, library root, or a single folder
//! - [`RaindropImportProgress`] / [`RaindropImportPhase`] — UI progress callbacks
//! - [`RaindropImportSummary`] — aggregate result after import
//! - [`RaindropOAuthConfig`] — app client id/secret for browser sign-in
//!
//! # Data flow
//!
//! 1. Resolve a bearer token: `PDF_FOLIO_RAINDROP_TOKEN` → cached OAuth token → browser OAuth.
//! 2. List user + collections + PDF raindrops via the REST client.
//! 3. Optionally mirror collections into local folders (when destination preserves structure).
//! 4. For large sets of uploaded files (≥ [`ZIP_IMPORT_THRESHOLD`]), download Raindrop’s
//!    ZIP export and match archive entries to raindrops by name/size; otherwise download
//!    each PDF individually with SSRF-safe URL validation.
//! 5. Import PDF bytes into the library and record Raindrop id mappings in core DB tables.
//!
//! # Related modules
//!
//! | Submodule | Responsibility |
//! | --- | --- |
//! | `auth` | OAuth loopback, token cache under XDG data, env/bundled credentials |
//! | `client` | REST client, PDF/ZIP download with size and private-IP guards |
//! | `types` | Import-facing DTOs |
//! | `import` | Orchestration: preview, selected/all import, progress, folder mirroring |
//! | `matching` | ZIP strategy selection and archive entry ↔ raindrop matching |
//!
//! Sync client and control plane are unrelated products: [`crate::sync`], [`crate::server`].

/// Raindrop REST API root (`/rest/v1`).
pub(crate) const API_BASE: &str = "https://api.raindrop.io/rest/v1";
/// Maximum raindrops requested per list page.
pub(crate) const MAX_PER_PAGE: u16 = 50;
/// When this many selected raindrops have uploaded files, prefer the ZIP export path.
pub(crate) const ZIP_IMPORT_THRESHOLD: usize = 12;
/// Progress basis points (1/100 of a percent) after ZIP export request is prepared.
pub(crate) const ZIP_PREPARING_PROGRESS_BASIS_POINTS: u16 = 1_250;
/// Progress basis points when the ZIP bytes have finished downloading.
pub(crate) const ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS: u16 = 3_750;
/// Progress basis points when selected PDFs have been extracted from the ZIP.
pub(crate) const ZIP_EXTRACTED_PROGRESS_BASIS_POINTS: u16 = 5_000;

/// OAuth loopback, token cache, and env/bundled app credentials.
mod auth;
/// REST client, remote models, and SSRF-safe PDF/ZIP downloads.
mod client;
/// Preview/import orchestration and local PDF metadata import.
mod import;
/// ZIP vs individual strategy selection and archive entry matching.
mod matching;
/// Import-facing DTOs (preview, progress, destination, OAuth config).
mod types;
pub use auth::OAUTH_CALLBACK_URL;
use auth::{bundled_or_env_oauth_config, cached_access_token};
pub use import::{
    import_all_pdfs, import_all_pdfs_with_auth, import_preview, import_preview_pdfs_with_progress,
    import_preview_with_auth, import_selected_pdfs, import_selected_pdfs_with_auth,
    import_selected_pdfs_with_progress,
};
pub use types::*;

/// Returns true when the importer can run without prompting for OAuth app credentials.
///
/// True if any of: non-empty `PDF_FOLIO_RAINDROP_TOKEN`, a cached access token on disk,
/// or bundled/env `PDF_FOLIO_RAINDROP_CLIENT_ID` + `PDF_FOLIO_RAINDROP_CLIENT_SECRET`.
pub fn can_import_without_prompt() -> bool {
    std::env::var("PDF_FOLIO_RAINDROP_TOKEN")
        .ok()
        .is_some_and(|token| !token.trim().is_empty())
        || cached_access_token().is_ok_and(|token| !token.trim().is_empty())
        || bundled_or_env_oauth_config().is_some()
}

#[cfg(test)]
mod tests;
