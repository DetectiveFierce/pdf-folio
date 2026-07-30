//! Environment-backed configuration for the sync control-plane process.
//!
//! [`Config::load`] merges process environment variables with optional files
//! under `PDF_FOLIO_SECRETS_DIR` (default `secrets/`): Google
//! `client_secret_*.json`, `turso credentials`, and `cloudflare credentials`.
//! Env vars always win when set.
//!
//! # Required security settings
//!
//! At least one of `PDF_FOLIO_ALLOWED_GOOGLE_SUB` or
//! `PDF_FOLIO_ALLOWED_GOOGLE_EMAIL` must be set; otherwise the server refuses
//! to start. Without an allow-list any Google account that completes OAuth
//! could mint credentials for Turso/R2.
//!
//! # Session secret
//!
//! `PDF_FOLIO_SESSION_SECRET` is preferred. If unset, a stable secret is derived
//! by hashing Turso + R2 credentials — convenient for single-host deploys, but
//! rotating those credentials will invalidate existing session JWTs.
//!
//! # Related
//!
//! Used by [`super::run`] and held inside handler [`super::handlers`] app state.

use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

/// Default Google OAuth token endpoint when not set in env/credentials file.
const DEFAULT_GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
/// Default R2 bucket name when `PDF_FOLIO_R2_BUCKET` is unset.
const DEFAULT_R2_BUCKET: &str = "pdf-folio";

/// Fully resolved runtime configuration for the control-plane process.
#[derive(Debug)]
pub(crate) struct Config {
    /// Address the HTTP server binds (default `127.0.0.1:53148`).
    pub(crate) bind_addr: SocketAddr,
    /// Google OAuth desktop/web client id.
    pub(crate) google_client_id: String,
    /// Google client secret when the OAuth client is confidential.
    pub(crate) google_client_secret: Option<String>,
    /// Google token endpoint URI.
    pub(crate) google_token_uri: String,
    /// Allowed Google subject (`sub`); either this or email must be set.
    pub(crate) allowed_google_sub: Option<String>,
    /// Allowed Google email (case-insensitive match).
    pub(crate) allowed_google_email: Option<String>,
    /// HMAC key material for session JWTs.
    pub(crate) session_secret: Vec<u8>,
    /// Turso / libSQL database URL handed to authenticated clients.
    pub(crate) turso_database_url: String,
    /// Turso auth token handed to authenticated clients.
    pub(crate) turso_auth_token: String,
    /// Cloudflare account id (loaded for completeness; unused by handlers today).
    pub(crate) _r2_account_id: String,
    /// R2 bucket name (default `pdf-folio`).
    pub(crate) r2_bucket: String,
    /// R2 access key id for SigV4 signing.
    pub(crate) r2_access_key_id: String,
    /// R2 secret access key for SigV4 signing (never sent to clients).
    pub(crate) r2_secret_access_key: String,
    /// R2 S3-compatible endpoint URL.
    pub(crate) r2_endpoint: Url,
}

impl Config {
    /// Loads configuration from the environment and optional secrets directory.
    ///
    /// # Errors
    ///
    /// Returns an error when secrets cannot be read, required allow-list vars
    /// are missing, or URLs/addresses fail to parse.
    pub(crate) fn load() -> Result<Self> {
        let secrets_dir = env::var("PDF_FOLIO_SECRETS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("secrets"));
        let google = load_google_credentials(&secrets_dir)?;
        let turso = load_turso_credentials(&secrets_dir)?;
        let r2 = load_r2_credentials(&secrets_dir)?;
        let bind_addr = env::var("PDF_FOLIO_SYNC_BIND_ADDR")
            .unwrap_or_else(|_| String::from("127.0.0.1:53148"))
            .parse()
            .context("PDF_FOLIO_SYNC_BIND_ADDR must be a socket address.")?;
        let allowed_google_sub = env_nonempty("PDF_FOLIO_ALLOWED_GOOGLE_SUB");
        let allowed_google_email = env_nonempty("PDF_FOLIO_ALLOWED_GOOGLE_EMAIL");
        if allowed_google_sub.is_none() && allowed_google_email.is_none() {
            bail!(
                "Set PDF_FOLIO_ALLOWED_GOOGLE_SUB or PDF_FOLIO_ALLOWED_GOOGLE_EMAIL before running the sync server."
            );
        }
        let session_secret = env_nonempty("PDF_FOLIO_SESSION_SECRET")
            .unwrap_or_else(|| {
                let mut hasher = Sha256::new();
                hasher.update(turso.auth_token.as_bytes());
                hasher.update(r2.secret_access_key.as_bytes());
                hex::encode(hasher.finalize())
            })
            .into_bytes();

        Ok(Self {
            bind_addr,
            google_client_id: env_nonempty("PDF_FOLIO_GOOGLE_CLIENT_ID")
                .unwrap_or(google.client_id),
            google_client_secret: env_nonempty("PDF_FOLIO_GOOGLE_CLIENT_SECRET")
                .or(google.client_secret),
            google_token_uri: env_nonempty("PDF_FOLIO_GOOGLE_TOKEN_URI")
                .unwrap_or(google.token_uri),
            allowed_google_sub,
            allowed_google_email,
            session_secret,
            turso_database_url: env_nonempty("PDF_FOLIO_TURSO_DATABASE_URL")
                .unwrap_or(turso.database_url),
            turso_auth_token: env_nonempty("PDF_FOLIO_TURSO_AUTH_TOKEN")
                .unwrap_or(turso.auth_token),
            _r2_account_id: env_nonempty("PDF_FOLIO_R2_ACCOUNT_ID").unwrap_or(r2.account_id),
            r2_bucket: env_nonempty("PDF_FOLIO_R2_BUCKET")
                .unwrap_or_else(|| String::from(DEFAULT_R2_BUCKET)),
            r2_access_key_id: env_nonempty("PDF_FOLIO_R2_ACCESS_KEY_ID")
                .unwrap_or(r2.access_key_id),
            r2_secret_access_key: env_nonempty("PDF_FOLIO_R2_SECRET_ACCESS_KEY")
                .unwrap_or(r2.secret_access_key),
            r2_endpoint: env_nonempty("PDF_FOLIO_R2_ENDPOINT")
                .unwrap_or(r2.endpoint)
                .parse()
                .context("R2 endpoint must be a URL.")?,
        })
    }
}

/// Reads a non-empty trimmed env var, or `None` when missing/blank.
fn env_nonempty(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Loads Google OAuth app credentials from env or `client_secret_*.json` under secrets.
///
/// # Errors
///
/// Returns an error when neither env nor a parseable client secret file is available.
fn load_google_credentials(secrets_dir: &Path) -> Result<GoogleCredentials> {
    if let Some(client_id) = env_nonempty("PDF_FOLIO_GOOGLE_CLIENT_ID") {
        return Ok(GoogleCredentials {
            client_id,
            client_secret: env_nonempty("PDF_FOLIO_GOOGLE_CLIENT_SECRET"),
            token_uri: env_nonempty("PDF_FOLIO_GOOGLE_TOKEN_URI")
                .unwrap_or_else(|| String::from(DEFAULT_GOOGLE_TOKEN_URI)),
        });
    }
    let path = std::fs::read_dir(secrets_dir)
        .with_context(|| {
            format!(
                "Could not read secrets directory {}.",
                secrets_dir.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("client_secret_") && name.ends_with(".json"))
        })
        .ok_or_else(|| {
            anyhow!("Could not find Google client_secret_*.json in secrets directory.")
        })?;
    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("Could not read {}.", path.display()))?;
    let file = serde_json::from_str::<GoogleCredentialFile>(&json)
        .with_context(|| format!("Could not parse {}.", path.display()))?;
    Ok(GoogleCredentials {
        client_id: file.installed.client_id,
        client_secret: file.installed.client_secret,
        token_uri: file
            .installed
            .token_uri
            .unwrap_or_else(|| String::from(DEFAULT_GOOGLE_TOKEN_URI)),
    })
}

/// Loads Turso URL/token from env or the labeled `turso credentials` secrets file.
///
/// # Errors
///
/// Returns an error when credentials cannot be read or required labels are missing.
fn load_turso_credentials(secrets_dir: &Path) -> Result<TursoCredentials> {
    if let (Some(database_url), Some(auth_token)) = (
        env_nonempty("PDF_FOLIO_TURSO_DATABASE_URL"),
        env_nonempty("PDF_FOLIO_TURSO_AUTH_TOKEN"),
    ) {
        return Ok(TursoCredentials {
            database_url,
            auth_token,
        });
    }
    let text = std::fs::read_to_string(secrets_dir.join("turso credentials"))
        .context("Could not read Turso credentials.")?;
    Ok(TursoCredentials {
        database_url: parse_labeled_secret(&text, "Database URL")?,
        auth_token: parse_labeled_secret(&text, "Token")?,
    })
}

/// Loads R2 credentials from env or the labeled `cloudflare credentials` secrets file.
///
/// # Errors
///
/// Returns an error when credentials cannot be read or required labels/URL are missing.
fn load_r2_credentials(secrets_dir: &Path) -> Result<R2Credentials> {
    if let (Some(account_id), Some(access_key_id), Some(secret_access_key), Some(endpoint)) = (
        env_nonempty("PDF_FOLIO_R2_ACCOUNT_ID"),
        env_nonempty("PDF_FOLIO_R2_ACCESS_KEY_ID"),
        env_nonempty("PDF_FOLIO_R2_SECRET_ACCESS_KEY"),
        env_nonempty("PDF_FOLIO_R2_ENDPOINT"),
    ) {
        return Ok(R2Credentials {
            account_id,
            access_key_id,
            secret_access_key,
            endpoint,
        });
    }
    let text = std::fs::read_to_string(secrets_dir.join("cloudflare credentials"))
        .context("Could not read Cloudflare credentials.")?;
    Ok(R2Credentials {
        account_id: parse_labeled_secret(&text, "Account ID")?,
        access_key_id: parse_labeled_secret(&text, "Access Key ID")?,
        secret_access_key: parse_labeled_secret(&text, "Secret Access Key")?,
        endpoint: parse_labeled_url(&text)?,
    })
}

/// Parses `Label: value` lines from a secrets text file.
///
/// # Errors
///
/// Returns an error when the label is missing or the value is empty.
fn parse_labeled_secret(text: &str, label: &str) -> Result<String> {
    text.lines()
        .find_map(|line| {
            line.strip_prefix(label)
                .and_then(|rest| rest.strip_prefix(':'))
                .map(str::trim)
                .map(str::to_owned)
        })
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Missing {label} in secrets file."))
}

/// Finds the first `http(s)://…` token in a secrets text blob (R2 endpoint).
///
/// # Errors
///
/// Returns an error when no URL token is present.
fn parse_labeled_url(text: &str) -> Result<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Missing R2 endpoint URL in Cloudflare credentials."))
}

/// Intermediate Google OAuth app credentials before merge into [`Config`].
#[derive(Debug)]
struct GoogleCredentials {
    /// OAuth client id.
    client_id: String,
    /// Optional client secret for confidential clients.
    client_secret: Option<String>,
    /// Token endpoint URI.
    token_uri: String,
}

/// Root of Google’s desktop `client_secret_*.json` file.
#[derive(Debug, Deserialize)]
struct GoogleCredentialFile {
    /// Installed-application credential block.
    installed: GoogleInstalledCredentials,
}

/// `installed` object inside a Google desktop client secret JSON file.
#[derive(Debug, Deserialize)]
struct GoogleInstalledCredentials {
    /// OAuth client id.
    client_id: String,
    /// Optional client secret.
    client_secret: Option<String>,
    /// Optional token URI (defaults to Google’s public endpoint).
    token_uri: Option<String>,
}

/// Intermediate Turso credentials before merge into [`Config`].
#[derive(Debug)]
struct TursoCredentials {
    /// libSQL / Turso database URL.
    database_url: String,
    /// Database auth token.
    auth_token: String,
}

/// Intermediate R2 credentials before merge into [`Config`].
#[derive(Debug)]
struct R2Credentials {
    /// Cloudflare account id.
    account_id: String,
    /// R2 access key id.
    access_key_id: String,
    /// R2 secret access key.
    secret_access_key: String,
    /// S3-compatible endpoint URL string.
    endpoint: String,
}
