//! Desktop Google OAuth (PKCE) against the sync control plane.
//!
//! Part of the sync **client** product ([`crate::sync`]). The desktop app never
//! holds Google client secrets for confidential clients when using PKCE; it
//! opens a browser, catches the redirect on a loopback port, and posts
//! `code` + `code_verifier` to the control plane’s
//! `POST /auth/google/callback`. The server performs Google token exchange,
//! allow-list checks, and returns a session JWT.
//!
//! # Security notes
//!
//! - Uses S256 PKCE; state nonce is checked on the loopback callback.
//! - Loopback URL is fixed: `http://127.0.0.1:53149/callback` (must match the
//!   Google OAuth client’s authorized redirect URIs).
//! - Successful sessions are written via [`super::session::save_session`]; the
//!   JWT is a bearer credential for Turso/R2 token routes — treat the cache file
//!   as secret.
//!
//! # Related
//!
//! - Server callback: control-plane handlers under [`crate::server`]
//! - Session persistence: [`super::session`]
//! - CLI: `pdf-folio sync auth` in [`super::cli`]

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use rand::RngCore;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use super::session::{save_session, Session};

/// Google OAuth 2.0 authorize endpoint for the browser PKCE flow.
const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Loopback TCP port for the Google OAuth redirect listener.
const OAUTH_REDIRECT_PORT: u16 = 53149;
/// Fixed loopback redirect URI registered on the Google OAuth desktop client.
const OAUTH_CALLBACK_URL: &str = "http://127.0.0.1:53149/callback";

/// Client-side configuration for signing into the PDF-Folio sync server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleAuthConfig {
    /// Google OAuth desktop app client ID.
    pub client_id: String,
    /// Tailscale-reachable sync server base URL, e.g. `http://mind-palace:53148`.
    pub sync_server_base_url: String,
}

/// Starts the browser-based Google PKCE flow and caches the returned sync session.
///
/// Opens the system browser, waits on the loopback redirect, exchanges the code
/// with the control plane, and persists a [`Session`] under the XDG data dir.
///
/// # Errors
///
/// Returns an error when browser sign-in, the loopback callback, or the sync
/// server token exchange fails (including allow-list rejection on the server).
pub async fn sign_in_with_google(config: &GoogleAuthConfig) -> Result<Session> {
    let state = nonce();
    let code_verifier = pkce_verifier();
    let code_challenge = pkce_challenge(&code_verifier);
    let mut authorize_url = Url::parse(GOOGLE_AUTHORIZE_URL)?;
    authorize_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", OAUTH_CALLBACK_URL)
        .append_pair("scope", "openid email profile")
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");

    let listener = TcpListener::bind(("127.0.0.1", OAUTH_REDIRECT_PORT))
        .await
        .with_context(|| format!("Could not listen on {OAUTH_CALLBACK_URL} for Google OAuth."))?;
    webbrowser::open(authorize_url.as_str())
        .context("Could not open browser for Google sign-in.")?;
    let code = wait_for_oauth_code(listener, &state).await?;

    let server_url = format!(
        "{}/auth/google/callback",
        config.sync_server_base_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .post(server_url)
        .header(CONTENT_TYPE, "application/json")
        .json(&ServerCallbackRequest {
            code,
            code_verifier,
            redirect_uri: OAUTH_CALLBACK_URL.to_owned(),
        })
        .send()
        .await
        .context("Could not exchange Google code with PDF-Folio sync server.")?
        .error_for_status()
        .context("PDF-Folio sync server rejected the Google sign-in.")?
        .json::<ServerSessionResponse>()
        .await
        .context("PDF-Folio sync server returned an invalid session response.")?;
    let session = Session {
        server_base_url: config.sync_server_base_url.clone(),
        session_token: response.session_token,
        expires_at: response.expires_at,
        google_sub: response.google_sub,
        email: response.email,
    };
    save_session(&session)?;
    Ok(session)
}

/// Accepts one loopback HTTP request and returns the Google OAuth `code` after validating `state`.
///
/// # Errors
///
/// Returns an error when the request is malformed, `state` mismatches, or Google returned `error`.
async fn wait_for_oauth_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener.accept().await?;
    let mut buffer = vec![0_u8; 8192];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request
        .lines()
        .next()
        .context("Google OAuth callback was empty.")?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("Google OAuth callback did not include a path.")?;
    let callback = Url::parse(&format!("http://127.0.0.1{path}"))?;
    let params = callback.query_pairs().collect::<HashMap<_, _>>();

    let body = "Google sign-in complete. You can return to PDF-Folio.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;

    if let Some(error) = params.get("error") {
        bail!("Google sign-in failed: {error}");
    }
    if params.get("state").map(|value| value.as_ref()) != Some(expected_state) {
        bail!("Google sign-in returned an invalid state.");
    }
    params
        .get("code")
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("Google sign-in did not return an authorization code."))
}

/// Random URL-safe PKCE code verifier (32 random bytes, base64url, no padding).
fn pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// S256 PKCE challenge: base64url(SHA-256(verifier)) without padding.
fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Opaque OAuth `state` nonce (random + wall-clock nanos) for CSRF protection.
fn nonce() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{clock}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    )
}

/// JSON body posted to the control plane after Google returns an authorization code.
#[derive(Debug, Serialize)]
struct ServerCallbackRequest {
    /// Google authorization code from the loopback callback.
    code: String,
    /// PKCE verifier that pairs with the authorize-step challenge.
    code_verifier: String,
    /// Redirect URI used at authorize time (must match the Google client config).
    redirect_uri: String,
}

/// Session payload returned by `POST /auth/google/callback` on the control plane.
#[derive(Debug, Deserialize)]
struct ServerSessionResponse {
    /// HS256 session JWT for subsequent control-plane calls.
    session_token: String,
    /// When the session JWT expires.
    expires_at: chrono::DateTime<chrono::Utc>,
    /// Google subject of the authorized account.
    google_sub: String,
    /// Google email when present on userinfo.
    email: Option<String>,
}
