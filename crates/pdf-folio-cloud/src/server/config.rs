use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

const DEFAULT_GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_R2_BUCKET: &str = "pdf-folio";

#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) bind_addr: SocketAddr,
    pub(crate) google_client_id: String,
    pub(crate) google_client_secret: Option<String>,
    pub(crate) google_token_uri: String,
    pub(crate) allowed_google_sub: Option<String>,
    pub(crate) allowed_google_email: Option<String>,
    pub(crate) session_secret: Vec<u8>,
    pub(crate) turso_database_url: String,
    pub(crate) turso_auth_token: String,
    pub(crate) _r2_account_id: String,
    pub(crate) r2_bucket: String,
    pub(crate) r2_access_key_id: String,
    pub(crate) r2_secret_access_key: String,
    pub(crate) r2_endpoint: Url,
}

impl Config {
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

fn env_nonempty(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

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

fn parse_labeled_url(text: &str) -> Result<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Missing R2 endpoint URL in Cloudflare credentials."))
}

#[derive(Debug)]
struct GoogleCredentials {
    client_id: String,
    client_secret: Option<String>,
    token_uri: String,
}

#[derive(Debug, Deserialize)]
struct GoogleCredentialFile {
    installed: GoogleInstalledCredentials,
}

#[derive(Debug, Deserialize)]
struct GoogleInstalledCredentials {
    client_id: String,
    client_secret: Option<String>,
    token_uri: Option<String>,
}

#[derive(Debug)]
struct TursoCredentials {
    database_url: String,
    auth_token: String,
}

#[derive(Debug)]
struct R2Credentials {
    account_id: String,
    access_key_id: String,
    secret_access_key: String,
    endpoint: String,
}
