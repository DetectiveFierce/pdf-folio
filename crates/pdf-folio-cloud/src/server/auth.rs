use anyhow::{anyhow, bail, Context, Result};
use axum::http::HeaderMap;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};

use super::config::Config;
use super::handlers::AppState;

const GOOGLE_USERINFO_URI: &str = "https://openidconnect.googleapis.com/v1/userinfo";

pub(crate) const SESSION_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;

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
    Ok(state
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
        .context("Google token response was not JSON.")?)
}

pub(crate) async fn google_userinfo(
    state: &AppState,
    access_token: &str,
) -> Result<GoogleUserInfo> {
    Ok(state
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
        .context("Google userinfo response was not JSON.")?)
}

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct SessionClaims {
    pub(crate) sub: String,
    pub(crate) email: Option<String>,
    pub(crate) iat: i64,
    pub(crate) exp: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoogleTokenResponse {
    pub(crate) access_token: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoogleUserInfo {
    pub(crate) sub: String,
    pub(crate) email: Option<String>,
}
