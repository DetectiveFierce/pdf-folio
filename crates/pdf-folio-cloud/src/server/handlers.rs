use std::sync::Arc;

use anyhow::Context;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use super::auth::{
    exchange_google_code, google_userinfo, require_session, verify_google_identity, SessionClaims,
    SESSION_TTL_SECONDS,
};
use super::config::Config;
use super::storage::{presigned_r2_url, r2_blob_key, validate_hash, R2_URL_TTL_SECONDS};

#[derive(Debug)]
pub(crate) struct AppState {
    pub(crate) http: reqwest::Client,
    pub(crate) config: Config,
}

pub(crate) fn router(config: Config) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health))
        .route(
            "/auth/google/callback",
            axum::routing::post(google_callback),
        )
        .route("/token/turso", axum::routing::get(turso_token))
        .route("/token/r2/upload", axum::routing::post(r2_upload_token))
        .route("/token/r2/download", axum::routing::get(r2_download_token))
        .with_state(Arc::new(AppState {
            http: reqwest::Client::new(),
            config,
        }))
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
