//! Maintenance binary that applies the remote Turso sync schema.
//!
//! Reads Turso credentials from the environment (not via the control plane) and
//! executes `turso_schema.sql` over SQL-over-HTTP. Use this for first-time
//! remote setup or when schema drifts from the client’s embedded copy.
//!
//! # Required environment
//!
//! - `PDF_FOLIO_TURSO_DATABASE_URL`
//! - `PDF_FOLIO_TURSO_AUTH_TOKEN`
//!
//! # Related
//!
//! - Schema source: `crates/pdf-folio-cloud/turso_schema.sql`
//! - Runtime path for authenticated clients: `SyncClient::ensure_remote_schema`
//! - Control plane does not apply schema; it only hands out credentials

use anyhow::{Context, Result};
use pdf_folio_cloud::sync::remote::{TursoRemote, TursoToken};

/// Process entry: apply embedded `turso_schema.sql` with direct Turso env credentials.
///
/// # Errors
///
/// Returns an error when env vars are missing or the remote schema batch fails.
#[tokio::main]
async fn main() -> Result<()> {
    let database_url = std::env::var("PDF_FOLIO_TURSO_DATABASE_URL")
        .context("PDF_FOLIO_TURSO_DATABASE_URL is required.")?;
    let auth_token = std::env::var("PDF_FOLIO_TURSO_AUTH_TOKEN")
        .context("PDF_FOLIO_TURSO_AUTH_TOKEN is required.")?;
    let remote = TursoRemote::from_token(TursoToken {
        database_url,
        auth_token,
        expires_at: chrono::Utc::now(),
    });
    remote
        .execute_batch(include_str!("../../turso_schema.sql"))
        .await
        .context("Could not apply PDF-Folio Turso schema.")?;
    println!("Applied PDF-Folio Turso sync schema.");
    Ok(())
}
