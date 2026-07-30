//! Binary entry for the single-user sync control plane (`pdf-folio-sync-server`).
//!
//! This is product two of the `pdf-folio-cloud` crate: a small HTTP service that
//! verifies Google identity, enforces the allow-list, and mints short-lived
//! Turso/R2 credentials. It does not store or proxy library data.
//!
//! # Configuration
//!
//! See `pdf_folio_cloud::server` / `Config::load`. Typical env vars:
//!
//! - `PDF_FOLIO_SYNC_BIND_ADDR` (default `127.0.0.1:53148`)
//! - `PDF_FOLIO_ALLOWED_GOOGLE_SUB` and/or `PDF_FOLIO_ALLOWED_GOOGLE_EMAIL` (**required**)
//! - Google / Turso / R2 credentials via env or `PDF_FOLIO_SECRETS_DIR`
//! - `RUST_LOG` / tracing `EnvFilter` (default `info`)
//!
//! # Related
//!
//! - Library: [`pdf_folio_cloud::server::run`]
//! - Client: `pdf_folio_cloud::sync`
//! - Packaging: `packaging/folio-sync-server.*`

use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// Process entry: init tracing from `RUST_LOG`, then block on [`pdf_folio_cloud::server::run`].
///
/// # Errors
///
/// Returns an error when configuration load or the HTTP server fails.
#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    pdf_folio_cloud::server::run().await
}
