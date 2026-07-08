use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use directories::ProjectDirs;
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

use crate::RaindropOAuthConfig;

const OAUTH_AUTHORIZE_URL: &str = "https://raindrop.io/oauth/authorize";
const OAUTH_TOKEN_URL: &str = "https://raindrop.io/oauth/access_token";
const OAUTH_REDIRECT_PORT: u16 = 53147;

/// OAuth redirect URI that must be configured in the user's Raindrop app.
pub const OAUTH_CALLBACK_URL: &str = "http://localhost:53147/raindrop/callback";

pub(crate) async fn resolve_access_token(
    oauth_config: Option<RaindropOAuthConfig>,
) -> Result<String> {
    if let Ok(token) = std::env::var("PDF_FOLIO_RAINDROP_TOKEN") {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    if let Ok(token) = cached_access_token() {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    if let Some(config) = oauth_config {
        return oauth_access_token(&config.client_id, &config.client_secret).await;
    }

    let config = bundled_or_env_oauth_config().context(
        "Connect Raindrop.io first, or configure PDF_FOLIO_RAINDROP_TOKEN / OAuth credentials.",
    )?;
    oauth_access_token(&config.client_id, &config.client_secret).await
}

pub(crate) fn bundled_or_env_oauth_config() -> Option<RaindropOAuthConfig> {
    let client_id = std::env::var("PDF_FOLIO_RAINDROP_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("PDF_FOLIO_RAINDROP_CLIENT_ID").map(str::to_owned))?;
    let client_secret = std::env::var("PDF_FOLIO_RAINDROP_CLIENT_SECRET")
        .ok()
        .or_else(|| option_env!("PDF_FOLIO_RAINDROP_CLIENT_SECRET").map(str::to_owned))?;
    let client_id = client_id.trim().to_owned();
    let client_secret = client_secret.trim().to_owned();
    (!client_id.is_empty() && !client_secret.is_empty()).then_some(RaindropOAuthConfig {
        client_id,
        client_secret,
    })
}

pub(crate) fn cached_access_token() -> Result<String> {
    let path = token_cache_path()?;
    let json =
        fs::read_to_string(&path).with_context(|| format!("Could not read {}.", path.display()))?;
    let cache = serde_json::from_str::<TokenCache>(&json)
        .with_context(|| format!("Could not parse {}.", path.display()))?;
    Ok(cache.access_token)
}

async fn oauth_access_token(client_id: &str, client_secret: &str) -> Result<String> {
    let redirect_uri = String::from(OAUTH_CALLBACK_URL);
    let state = format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let mut authorize_url = Url::parse(OAUTH_AUTHORIZE_URL)?;
    authorize_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("state", &state);

    let listener = TcpListener::bind(("127.0.0.1", OAUTH_REDIRECT_PORT))
        .await
        .with_context(|| format!("Could not listen on {redirect_uri} for Raindrop OAuth."))?;
    webbrowser::open(authorize_url.as_str())
        .context("Could not open browser for Raindrop sign-in.")?;
    let code = wait_for_oauth_code(listener, &state).await?;

    let http = reqwest::Client::new();
    let token = http
        .post(OAUTH_TOKEN_URL)
        .header(CONTENT_TYPE, "application/json")
        .json(&TokenExchangeRequest {
            grant_type: "authorization_code",
            code,
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
            redirect_uri,
        })
        .send()
        .await?
        .error_for_status()?
        .json::<TokenExchangeResponse>()
        .await?;
    save_cached_access_token(&token.access_token)?;
    Ok(token.access_token)
}

fn save_cached_access_token(access_token: &str) -> Result<()> {
    let path = token_cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    let cache = TokenCache {
        access_token: access_token.to_owned(),
    };
    fs::write(&path, serde_json::to_vec_pretty(&cache)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

fn token_cache_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().join("raindrop").join("token.json"))
}

async fn wait_for_oauth_code(listener: TcpListener, expected_state: &str) -> Result<String> {
    let (mut stream, _) = listener.accept().await?;
    let mut buffer = vec![0_u8; 8192];
    let read = stream.read(&mut buffer).await?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request
        .lines()
        .next()
        .context("Raindrop OAuth callback was empty.")?;
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("Raindrop OAuth callback did not include a path.")?;
    let callback = Url::parse(&format!("http://localhost{path}"))?;
    let params = callback.query_pairs().collect::<HashMap<_, _>>();

    let body = "Raindrop sign-in complete. You can return to PDF-Folio.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;

    if let Some(error) = params.get("error") {
        bail!("Raindrop sign-in failed: {error}");
    }
    if params.get("state").map(|value| value.as_ref()) != Some(expected_state) {
        bail!("Raindrop sign-in returned an invalid state.");
    }
    params
        .get("code")
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow!("Raindrop sign-in did not return an authorization code."))
}

#[derive(Debug, Serialize)]
struct TokenExchangeRequest {
    grant_type: &'static str,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
}
