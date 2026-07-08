//! Single-user PDF-Folio sync control plane.
//!
//! This binary is intended to run on `mind-palace` behind Tailscale. It handles
//! Google identity verification and returns short-lived credentials or
//! presigned URLs for direct Turso/R2 access.

use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URI: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const DEFAULT_R2_BUCKET: &str = "pdf-folio";
const SESSION_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;
const R2_URL_TTL_SECONDS: i64 = 60 * 15;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let bind_addr = config.bind_addr;
    let app = Router::new()
        .route("/health", get(health))
        .route("/auth/google/callback", post(google_callback))
        .route("/token/turso", get(turso_token))
        .route("/token/r2/upload", post(r2_upload_token))
        .route("/token/r2/download", get(r2_download_token))
        .with_state(Arc::new(AppState {
            http: reqwest::Client::new(),
            config,
        }));

    tracing::info!("Starting folio-sync-server on {bind_addr}");
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("Could not bind {bind_addr}."))?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Debug)]
struct AppState {
    http: reqwest::Client,
    config: Config,
}

#[derive(Debug)]
struct Config {
    bind_addr: SocketAddr,
    google_client_id: String,
    google_client_secret: Option<String>,
    google_token_uri: String,
    allowed_google_sub: Option<String>,
    allowed_google_email: Option<String>,
    session_secret: Vec<u8>,
    turso_database_url: String,
    turso_auth_token: String,
    _r2_account_id: String,
    r2_bucket: String,
    r2_access_key_id: String,
    r2_secret_access_key: String,
    r2_endpoint: Url,
}

impl Config {
    fn load() -> Result<Self> {
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

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { ok: true })
}

async fn google_callback(
    State(state): State<Arc<AppState>>,
    Json(request): Json<GoogleCallbackRequest>,
) -> ApiResult<Json<SessionResponse>> {
    let redirect_uri = request
        .redirect_uri
        .unwrap_or_else(|| String::from("http://127.0.0.1:53149/callback"));
    let token =
        exchange_google_code(&state, request.code, request.code_verifier, &redirect_uri).await?;
    let user = google_userinfo(&state, &token.access_token).await?;
    verify_google_identity(&state.config, &user)?;

    let now = Utc::now();
    let expires_at = now + Duration::seconds(SESSION_TTL_SECONDS);
    let claims = SessionClaims {
        sub: user.sub.clone(),
        email: user.email.clone(),
        iat: now.timestamp(),
        exp: expires_at.timestamp(),
    };
    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(&state.config.session_secret),
    )
    .context("Could not sign PDF-Folio session token.")?;
    Ok(Json(SessionResponse {
        session_token: jwt,
        expires_at,
        google_sub: user.sub,
        email: user.email,
    }))
}

async fn turso_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> ApiResult<Json<TursoTokenResponse>> {
    require_session(&state.config, &headers)?;
    Ok(Json(TursoTokenResponse {
        database_url: state.config.turso_database_url.clone(),
        auth_token: state.config.turso_auth_token.clone(),
        expires_at: Utc::now() + Duration::seconds(SESSION_TTL_SECONDS),
    }))
}

async fn r2_upload_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<R2UploadRequest>,
) -> ApiResult<Json<R2UploadResponse>> {
    require_session(&state.config, &headers)?;
    validate_hash(&request.hash)?;
    let key = r2_blob_key(&request.hash);
    Ok(Json(R2UploadResponse {
        exists: false,
        upload_url: Some(presigned_r2_url(
            &state.config,
            "PUT",
            &key,
            R2_URL_TTL_SECONDS,
        )?),
        expires_at: Utc::now() + Duration::seconds(R2_URL_TTL_SECONDS),
    }))
}

async fn r2_download_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<R2DownloadQuery>,
) -> ApiResult<Json<R2DownloadResponse>> {
    require_session(&state.config, &headers)?;
    validate_hash(&query.hash)?;
    let key = r2_blob_key(&query.hash);
    Ok(Json(R2DownloadResponse {
        download_url: presigned_r2_url(&state.config, "GET", &key, R2_URL_TTL_SECONDS)?,
        expires_at: Utc::now() + Duration::seconds(R2_URL_TTL_SECONDS),
    }))
}

async fn exchange_google_code(
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

async fn google_userinfo(state: &AppState, access_token: &str) -> Result<GoogleUserInfo> {
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

fn verify_google_identity(config: &Config, user: &GoogleUserInfo) -> Result<()> {
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

fn require_session(config: &Config, headers: &HeaderMap) -> Result<SessionClaims> {
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

fn presigned_r2_url(
    config: &Config,
    method: &str,
    key: &str,
    expires_seconds: i64,
) -> Result<String> {
    let now = Utc::now();
    let date = now.format("%Y%m%d").to_string();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let region = "auto";
    let service = "s3";
    let credential_scope = format!("{date}/{region}/{service}/aws4_request");
    let credential = format!("{}/{}", config.r2_access_key_id, credential_scope);
    let host = config
        .r2_endpoint
        .host_str()
        .ok_or_else(|| anyhow!("R2 endpoint URL has no host."))?;
    let canonical_uri = format!("/{}/{}", config.r2_bucket, percent_encode_path(key));

    let mut query = vec![
        ("X-Amz-Algorithm".to_owned(), "AWS4-HMAC-SHA256".to_owned()),
        ("X-Amz-Credential".to_owned(), credential),
        ("X-Amz-Date".to_owned(), timestamp),
        ("X-Amz-Expires".to_owned(), expires_seconds.to_string()),
        ("X-Amz-SignedHeaders".to_owned(), "host".to_owned()),
    ];
    query.sort_by(|left, right| left.0.cmp(&right.0));
    let canonical_query = query
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\nhost:{host}\n\nhost\nUNSIGNED-PAYLOAD"
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        now.format("%Y%m%dT%H%M%SZ"),
        credential_scope,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = sigv4_signing_key(&config.r2_secret_access_key, &date, region, service)?;
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes())?);

    let mut url = config.r2_endpoint.clone();
    url.set_path(&format!("{}/{}", config.r2_bucket, key));
    url.set_query(Some(&format!(
        "{canonical_query}&X-Amz-Signature={signature}"
    )));
    Ok(url.to_string())
}

fn sigv4_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Result<Vec<u8>> {
    let date_key = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let region_key = hmac_sha256(&date_key, region.as_bytes())?;
    let service_key = hmac_sha256(&region_key, service.as_bytes())?;
    hmac_sha256(&service_key, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key).context("Could not create HMAC signer.")?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn percent_encode_path(path: &str) -> String {
    path.split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/")
}

fn percent_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
        .replace("%7E", "~")
}

fn r2_blob_key(hash: &str) -> String {
    format!("blobs/{hash}.pdf")
}

fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(())
    } else {
        bail!("PDF blob hash must be a 64-character hex BLAKE3 digest.")
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

type ApiResult<T> = std::result::Result<T, ApiError>;

struct ApiError(anyhow::Error);

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        tracing::warn!("{:#}", self.0);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct GoogleCallbackRequest {
    code: String,
    code_verifier: String,
    redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct R2UploadRequest {
    hash: String,
}

#[derive(Debug, Deserialize)]
struct R2DownloadQuery {
    hash: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct SessionResponse {
    session_token: String,
    expires_at: DateTime<Utc>,
    google_sub: String,
    email: Option<String>,
}

#[derive(Debug, Serialize)]
struct TursoTokenResponse {
    database_url: String,
    auth_token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct R2UploadResponse {
    exists: bool,
    upload_url: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct R2DownloadResponse {
    download_url: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct SessionClaims {
    sub: String,
    email: Option<String>,
    iat: i64,
    exp: i64,
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: Option<String>,
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
