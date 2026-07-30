//! Google identity verification, session JWT verification, and allow-list checks.
//!
//! Part of the control-plane product ([`crate::server`]). Handlers call these
//! helpers when completing OAuth and when protecting credential-mint routes.
//!
//! # Security notes
//!
//! - Authorization codes are exchanged at Google’s token endpoint (PKCE
//!   `code_verifier` required; client secret optional for public desktop apps).
//! - [`verify_google_identity`] enforces the single-user allow-list: match on
//!   Google `sub` and/or email (case-insensitive). At least one configured
//!   allow-list entry must match or the request is rejected.
//! - Session tokens are HS256 JWTs signed with the server’s session secret.
//!   [`require_session`] reads `Authorization: Bearer <jwt>` and validates
//!   signature/expiry; audience is not checked (single-tenant server).
//!
//! # Related
//!
//! - [`super::handlers`] — mints session JWTs after successful verification
//! - [`super::config`] — allow-list env vars and session secret
//! - Client PKCE flow: [`crate::sync::auth`]

use anyhow::{anyhow, bail, Context, Result};
use axum::http::HeaderMap;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};

use super::config::Config;
use super::handlers::AppState;

const GOOGLE_USERINFO_URI: &str = "https://openidconnect.googleapis.com/v1/userinfo";

/// Session JWT lifetime (~30 days). Also used as the advertised Turso token window.
pub(crate) const SESSION_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;

/// Exchanges a Google authorization code for an access token (PKCE).
///
/// # Errors
///
/// Returns an error when the HTTP request fails or Google rejects the code.
pub(crate) async fn exchange_google_code(
    state: &AppState,
    code: String,
    code_verifier: String,
    redirect_uri: &str,
) -> Result<GoogleTokenResponse> {
    let mut form = vec![
        ("grant_type", "authorization_code".to_owned()),
        ("code", code),
        ("client_id", state.config.google_client_id.clone()),
        ("code_verifier", code_verifier),
        ("redirect_uri", redirect_uri.to_owned()),
    ];
    if let Some(secret) = &state.config.google_client_secret {
        form.push(("client_secret", secret.clone()));
    }
    state
        .http
        .post(&state.config.google_token_uri)
        .form(&form)
        .send()
        .await
        .context("Google token exchange request failed.")?
        .error_for_status()
        .context("Google rejected the authorization code.")?
        .json::<GoogleTokenResponse>()
        .await
        .context("Google token response was not JSON.")
}

/// Fetches OpenID userinfo (`sub`, optional email) for a Google access token.
///
/// # Errors
///
/// Returns an error when userinfo is unreachable or the token is rejected.
pub(crate) async fn google_userinfo(
    state: &AppState,
    access_token: &str,
) -> Result<GoogleUserInfo> {
    state
        .http
        .get(GOOGLE_USERINFO_URI)
        .header(AUTHORIZATION, format!("Bearer {access_token}"))
        .send()
        .await
        .context("Google userinfo request failed.")?
        .error_for_status()
        .context("Google userinfo rejected the access token.")?
        .json::<GoogleUserInfo>()
        .await
        .context("Google userinfo response was not JSON.")
}

/// Ensures the Google account is on the server allow-list (sub and/or email).
///
/// # Errors
///
/// Returns an error when neither allow-list field matches. This is the primary
/// single-user gate for the control plane.
pub(crate) fn verify_google_identity(config: &Config, user: &GoogleUserInfo) -> Result<()> {
    let sub_matches = config
        .allowed_google_sub
        .as_deref()
        .is_some_and(|sub| sub == user.sub);
    let email_matches = config
        .allowed_google_email
        .as_deref()
        .zip(user.email.as_deref())
        .is_some_and(|(allowed, email)| allowed.eq_ignore_ascii_case(email));
    if sub_matches || email_matches {
        Ok(())
    } else {
        bail!("Google account is not authorized for this PDF-Folio sync server.")
    }
}

/// Decodes and validates a bearer session JWT from request headers.
///
/// Expects `Authorization: Bearer <token>`. Does not re-check the Google
/// allow-list; that is enforced only at mint time.
///
/// # Errors
///
/// Returns an error when the header is missing/malformed or the JWT is invalid
/// or expired.
pub(crate) fn require_session(config: &Config, headers: &HeaderMap) -> Result<SessionClaims> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| anyhow!("Missing bearer session token."))?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_aud = false;
    Ok(decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(&config.session_secret),
        &validation,
    )
    .context("Invalid PDF-Folio session token.")?
    .claims)
}

/// Claims embedded in PDF-Folio session JWTs (`sub`, optional email, iat/exp).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct SessionClaims {
    /// Google subject of the authorized account.
    pub(crate) sub: String,
    /// Google email when present on the userinfo response.
    pub(crate) email: Option<String>,
    /// Issued-at (Unix seconds).
    pub(crate) iat: i64,
    /// Expiration (Unix seconds).
    pub(crate) exp: i64,
}

/// Subset of Google’s token-endpoint JSON used after code exchange.
#[derive(Debug, Deserialize)]
pub(crate) struct GoogleTokenResponse {
    pub(crate) access_token: String,
}

/// Subset of Google OpenID userinfo used for allow-list checks.
#[derive(Debug, Deserialize)]
pub(crate) struct GoogleUserInfo {
    /// Stable Google account subject.
    pub(crate) sub: String,
    /// Account email when the `email` scope was granted.
    pub(crate) email: Option<String>,
}
