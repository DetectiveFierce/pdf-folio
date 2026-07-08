//! Single-user PDF-Folio sync control plane.
//!
//! This binary is intended to run on `mind-palace` behind Tailscale. It handles
//! Google identity verification and returns short-lived credentials or
//! presigned URLs for direct Turso/R2 access.

mod auth;
mod config;
mod handlers;
mod storage;

use anyhow::{Context, Result};

use self::config::Config;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let bind_addr = config.bind_addr;
    let app = handlers::router(config);

    tracing::info!("Starting folio-sync-server on {bind_addr}");
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("Could not bind {bind_addr}."))?;
    axum::serve(listener, app).await?;
    Ok(())
}
