//! Single-user PDF-Folio sync **control plane** (product two of three in this crate).
//!
//! The server is a small axum process intended to run on a trusted host
//! (historically `mind-palace`) reachable over Tailscale. It is an identity and
//! credential mint — it does **not** proxy PDF bytes or SQL queries. After a
//! successful Google sign-in, clients talk directly to Turso and Cloudflare R2.
//!
//! # Security model
//!
//! - Google OAuth code exchange + OpenID userinfo to establish identity.
//! - Hard allow-list: at least one of `PDF_FOLIO_ALLOWED_GOOGLE_SUB` or
//!   `PDF_FOLIO_ALLOWED_GOOGLE_EMAIL` must match (see [`config`]).
//! - Session JWTs (HS256, ~30 day TTL) signed with `PDF_FOLIO_SESSION_SECRET`
//!   (or a derived secret from Turso/R2 credentials).
//! - Turso auth token and R2 secrets stay on the server; clients receive either
//!   the Turso token for the session window or short-lived (~15 min) R2 SigV4
//!   presigned URLs for a single content-addressed blob key.
//!
//! # Key types and entry points
//!
//! - [`run`] — load env/secrets config and serve the HTTP router.
//! - Internal modules: [`config::Config`], auth helpers (JWT / allow-list),
//!   handlers (routes), storage (Turso token + R2 presign).
//!
//! # Data flow
//!
//! ```text
//! Desktop ──POST /auth/google/callback──► server ──► Google token + userinfo
//!        ◄── session JWT ────────────────┘
//! Desktop ──GET  /token/turso (Bearer)──► server ──► Turso URL + auth token
//! Desktop ──POST /token/r2/upload ──────► server ──► presigned PUT
//! Desktop ──GET  /token/r2/download ────► server ──► presigned GET
//! ```
//!
//! # Related modules
//!
//! | Submodule | Responsibility |
//! | --- | --- |
//! | `config` | Env + secrets-dir configuration |
//! | `auth` | Google exchange, userinfo, allow-list, session JWT verify |
//! | `handlers` | Axum routes and response DTOs |
//! | `storage` | R2 SigV4 presign + BLAKE3 hash validation |
//!
//! Binary entry: `src/bin/pdf-folio-sync-server.rs`. Client counterpart: [`crate::sync`].

/// Google exchange, allow-list checks, and session JWT verification.
mod auth;
/// Env + secrets-dir configuration for the control-plane process.
mod config;
/// Axum routes and request/response DTOs for health, OAuth, and credential minting.
mod handlers;
/// R2 SigV4 presign helpers and BLAKE3 hash validation.
mod storage;

use anyhow::{Context, Result};

use self::config::Config;

/// Starts the sync control-plane HTTP server using environment configuration.
///
/// Loads [`Config`] (bind address, Google OAuth app, allow-list, Turso, R2),
/// builds the axum router, and blocks serving until the process exits.
///
/// # Errors
///
/// Returns an error when required env/secrets are missing, the allow-list is
/// empty, the bind address is invalid, or the TCP listener cannot be opened.
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
