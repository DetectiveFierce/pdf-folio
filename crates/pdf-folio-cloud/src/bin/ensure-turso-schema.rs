use anyhow::{Context, Result};
use pdf_folio_cloud::sync::turso_client::{TursoRemote, TursoToken};

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
