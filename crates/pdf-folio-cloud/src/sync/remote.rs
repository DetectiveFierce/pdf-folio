//! Turso/libSQL HTTP (Hrana) client for remote sync metadata.
//!
//! After the control plane returns credentials ([`TursoClient::token`]), the
//! desktop talks **directly** to Turso using SQL-over-HTTP (`/v2/pipeline`).
//! This module is the low-level transport; CRDT upsert/select SQL lives in
//! [`super::crdt`].
//!
//! # Key types
//!
//! - [`TursoClient`] — authenticated requests to `GET /token/turso` on the control plane
//! - [`TursoToken`] — database URL + auth token returned by the server
//! - [`TursoRemote`] — execute/query/batch against Turso with those credentials
//! - [`TursoValue`] — Hrana-tagged SQL cell values
//!
//! # Related
//!
//! - Control-plane route: server handlers `/token/turso`
//! - Schema application: `TursoRemote::execute_batch` / `ensure-turso-schema` binary
//! - Op log traffic: [`super::crdt`]

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::session::Session;

/// Turso access details returned by the sync control plane.
///
/// For the single-user deploy the server currently reuses its long-lived Turso
/// token and advertises a session-aligned `expires_at`. Clients should still
/// treat the token as secret and re-fetch after session refresh.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TursoToken {
    /// libSQL database URL (`libsql://…` or `https://…`).
    pub database_url: String,
    /// Auth token for the database.
    pub auth_token: String,
    /// Server-side expiration timestamp.
    pub expires_at: DateTime<Utc>,
}

/// One Hrana SQL-over-HTTP cell value (`type` + payload) for Turso `/v2/pipeline`.
///
/// Wire shape matches libSQL’s tagged JSON encoding. Prefer the constructors
/// ([`TursoValue::text`], [`TursoValue::integer`], …) when binding args so
/// integers stay string-encoded as Hrana requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TursoValue {
    /// SQL `NULL` (`{"type":"null"}`).
    Null,
    /// Integer cell; Hrana encodes the digits as a **string** (`{"type":"integer","value":"42"}`).
    Integer {
        /// Decimal integer digits (may include a leading `-`).
        value: String,
    },
    /// Floating-point cell encoded as a string payload for symmetric bind/read.
    Float {
        /// Decimal or scientific float digits.
        value: String,
    },
    /// UTF-8 text cell (`{"type":"text","value":"…"}`).
    Text {
        /// Column text contents.
        value: String,
    },
    /// Binary cell as base64 (`{"type":"blob","base64":"…"}`).
    Blob {
        /// Standard base64 of the blob bytes.
        base64: String,
    },
}

impl TursoValue {
    /// Binds a UTF-8 text argument (`type: text`).
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text {
            value: value.into(),
        }
    }

    /// Binds an `i64` as a Hrana integer (digits stringified for the wire).
    pub fn integer(value: i64) -> Self {
        Self::Integer {
            value: value.to_string(),
        }
    }

    /// Binds `Some(text)` or SQL `NULL` when the optional is empty.
    pub fn nullable_text(value: Option<&str>) -> Self {
        value.map(Self::text).unwrap_or(Self::Null)
    }

    /// Binds `Some(i64)` as integer or SQL `NULL` when the optional is empty.
    pub fn nullable_integer(value: Option<i64>) -> Self {
        value.map(Self::integer).unwrap_or(Self::Null)
    }

    /// Reads text/integer/float cells as a string; errors on `NULL` or blob.
    ///
    /// Integers and floats return their wire digit strings so callers can parse
    /// with [`Self::as_i64`] when a numeric type is required.
    pub fn as_string(&self) -> Result<String> {
        match self {
            Self::Text { value } | Self::Integer { value } | Self::Float { value } => {
                Ok(value.clone())
            }
            Self::Null => bail!("Expected value but found NULL."),
            Self::Blob { .. } => bail!("Expected text/integer but found blob."),
        }
    }

    /// Like [`Self::as_string`] but maps SQL `NULL` to `Ok(None)`.
    pub fn as_optional_string(&self) -> Result<Option<String>> {
        match self {
            Self::Null => Ok(None),
            _ => self.as_string().map(Some),
        }
    }

    /// Parses an integer/float/text cell as `i64` (errors on `NULL`/blob/non-numeric).
    pub fn as_i64(&self) -> Result<i64> {
        self.as_string()?
            .parse()
            .context("Turso integer value was not a valid i64.")
    }

    /// Like [`Self::as_i64`] but maps SQL `NULL` to `Ok(None)`.
    pub fn as_optional_i64(&self) -> Result<Option<i64>> {
        match self {
            Self::Null => Ok(None),
            _ => self.as_i64().map(Some),
        }
    }
}

/// A remote Turso database connection over SQL-over-HTTP (Hrana pipeline).
///
/// Prefer constructing via [`TursoClient::remote`] so credentials come from the
/// control plane. [`TursoRemote::from_token`] is used by maintenance binaries
/// that inject env credentials directly.
#[derive(Debug, Clone)]
pub struct TursoRemote {
    /// Shared HTTP client for Hrana pipeline requests.
    http: reqwest::Client,
    /// Database URL (`libsql://…` or `https://…`); converted to HTTP for the pipeline.
    database_url: String,
    /// Bearer auth token for the database.
    auth_token: String,
}

impl TursoRemote {
    /// Creates a remote from already-resolved credentials (no control-plane call).
    pub fn from_token(token: TursoToken) -> Self {
        Self {
            http: reqwest::Client::new(),
            database_url: token.database_url,
            auth_token: token.auth_token,
        }
    }

    /// Executes one statement and returns result rows.
    ///
    /// # Errors
    ///
    /// Returns an error when Turso rejects the request or returns an error result.
    pub async fn query(&self, sql: &str, args: Vec<TursoValue>) -> Result<Vec<Vec<TursoValue>>> {
        let result = self
            .pipeline(vec![
                StreamRequest::Execute {
                    stmt: Statement {
                        sql: sql.to_owned(),
                        args,
                    },
                },
                StreamRequest::Close,
            ])
            .await?;
        let first = result
            .results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Turso response did not include an execute result."))?;
        let response = first.into_execute_response()?;
        Ok(response.result.rows)
    }

    /// Executes one statement and discards result rows.
    pub async fn execute(&self, sql: &str, args: Vec<TursoValue>) -> Result<()> {
        self.query(sql, args).await.map(|_| ())
    }

    /// Executes a semicolon-separated SQL sequence.
    ///
    /// # Errors
    ///
    /// Returns an error when Turso rejects the sequence.
    pub async fn execute_batch(&self, sql: &str) -> Result<()> {
        let result = self
            .pipeline(vec![
                StreamRequest::Sequence {
                    sql: sql.to_owned(),
                },
                StreamRequest::Close,
            ])
            .await?;
        result
            .results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Turso response did not include a sequence result."))?
            .into_ok_response()?;
        Ok(())
    }

    async fn pipeline(&self, requests: Vec<StreamRequest>) -> Result<PipelineResponse> {
        let url = format!("{}/v2/pipeline", http_database_url(&self.database_url)?);
        self.http
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.auth_token))
            .json(&PipelineRequest {
                baton: None,
                requests,
            })
            .send()
            .await
            .context("Could not send Turso SQL-over-HTTP request.")?
            .error_for_status()
            .context("Turso rejected SQL-over-HTTP request.")?
            .json::<PipelineResponse>()
            .await
            .context("Turso SQL-over-HTTP response was not valid JSON.")
    }
}

/// Client for asking the sync control plane for Turso credentials.
///
/// Clients in the process share a short-lived credential cache so concurrent
/// startup sync phases do not each call the control plane for the same token.
#[derive(Debug, Clone)]
pub struct TursoClient {
    /// Shared HTTP client for control-plane credential requests.
    http: reqwest::Client,
    /// Session JWT + server base URL used to mint Turso credentials.
    session: Session,
}

impl TursoClient {
    /// Creates a Turso credential client from a session.
    pub fn new(session: Session) -> Self {
        Self {
            http: reqwest::Client::new(),
            session,
        }
    }

    /// Fetches current Turso credentials from the sync server.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is expired or the sync server rejects the request.
    pub async fn token(&self) -> Result<TursoToken> {
        static TOKEN_CACHE: std::sync::OnceLock<
            tokio::sync::Mutex<std::collections::HashMap<String, TursoToken>>,
        > = std::sync::OnceLock::new();
        let key = format!(
            "{}\n{}\n{}",
            self.session.server_base_url.trim_end_matches('/'),
            self.session.google_sub,
            self.session.expires_at.timestamp()
        );
        let mut cached_tokens = TOKEN_CACHE
            .get_or_init(|| tokio::sync::Mutex::new(std::collections::HashMap::new()))
            .lock()
            .await;
        if let Some(token) = cached_tokens
            .get(&key)
            .filter(|token| token.expires_at > Utc::now() + chrono::Duration::seconds(30))
        {
            return Ok(token.clone());
        }
        let url = format!(
            "{}/token/turso",
            self.session.server_base_url.trim_end_matches('/')
        );
        let token = self
            .http
            .get(url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.session.session_token),
            )
            .send()
            .await
            .context("Could not request Turso token from sync server.")?
            .error_for_status()
            .context("Sync server rejected Turso token request.")?
            .json::<TursoToken>()
            .await
            .context("Sync server returned an invalid Turso token response.")?;
        cached_tokens.insert(key, token.clone());
        Ok(token)
    }

    /// Creates a SQL-over-HTTP remote using credentials minted by the sync server.
    ///
    /// # Errors
    ///
    /// Returns an error when credentials cannot be fetched.
    pub async fn remote(&self) -> Result<TursoRemote> {
        let token = self.token().await?;
        Ok(TursoRemote::from_token(token))
    }
}

/// Converts a Turso `libsql://` URL to `https://` for Hrana HTTP; leaves `http(s)://` as-is.
///
/// # Errors
///
/// Returns an error for unsupported URL schemes.
fn http_database_url(database_url: &str) -> Result<String> {
    let trimmed = database_url.trim_end_matches('/');
    if let Some(rest) = trimmed.strip_prefix("libsql://") {
        Ok(format!("https://{rest}"))
    } else if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed.to_owned())
    } else {
        bail!("Unsupported Turso database URL: {database_url}");
    }
}

/// Hrana `POST /v2/pipeline` body: optional stream baton + ordered requests.
#[derive(Debug, Serialize)]
struct PipelineRequest {
    /// Stream resume token when continuing a multi-step pipeline (`None` = new stream).
    baton: Option<String>,
    /// Ordered Hrana requests executed on the same stream.
    requests: Vec<StreamRequest>,
}

/// One Hrana pipeline request (`type` tag) sent to Turso SQL-over-HTTP.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamRequest {
    /// Run a single prepared statement with bound args (`type: execute`).
    Execute {
        /// Statement SQL + positional args.
        stmt: Statement,
    },
    /// Run a multi-statement SQL script without returning row sets (`type: sequence`).
    Sequence {
        /// Semicolon-separated SQL (schema migrations, batch DDL/DML).
        sql: String,
    },
    /// Close the Hrana stream after prior requests (`type: close`).
    Close,
}

/// Bound SQL statement payload inside an [`StreamRequest::Execute`].
#[derive(Debug, Serialize)]
struct Statement {
    /// SQL text with `?` placeholders matching `args` length.
    sql: String,
    /// Positional Hrana values; omitted from JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<TursoValue>,
}

/// Top-level Hrana pipeline response: one result entry per request.
#[derive(Debug, Deserialize)]
struct PipelineResponse {
    /// Parallel array of ok/error results for each request in the pipeline.
    results: Vec<StreamResult>,
}

/// Per-request Hrana result envelope (`type: ok` | `type: error`).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamResult {
    /// Successful request; body depends on the matching request type.
    Ok {
        /// Typed response payload (execute rows, sequence ack, …).
        response: StreamResponse,
    },
    /// Failed request with a server-side SQL/protocol error.
    Error {
        /// Error code/message from Turso/libSQL.
        error: TursoError,
    },
}

impl StreamResult {
    fn into_ok_response(self) -> Result<StreamResponse> {
        match self {
            Self::Ok { response } => Ok(response),
            Self::Error { error } => bail!("Turso SQL error: {}", error.message()),
        }
    }

    fn into_execute_response(self) -> Result<ExecuteResponse> {
        match self.into_ok_response()? {
            StreamResponse::Execute { result } => Ok(ExecuteResponse { result }),
            other => bail!("Expected Turso execute response, got {other:?}."),
        }
    }
}

/// Successful Hrana response body variants nested under [`StreamResult::Ok`].
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamResponse {
    /// Rows from an `execute` request.
    Execute {
        /// Column metadata + row matrix.
        result: StatementResult,
    },
    /// Ack for a `sequence` request (no row payload).
    Sequence,
    /// Ack for a `close` request.
    Close,
    /// Forward-compatible catch-all for unknown response types.
    #[serde(other)]
    Other,
}

/// Thin wrapper used when extracting the first execute result from a pipeline.
#[derive(Debug, Deserialize)]
struct ExecuteResponse {
    /// Statement result rows/cols.
    result: StatementResult,
}

/// Row set returned by a successful Hrana `execute`.
#[derive(Debug, Deserialize)]
struct StatementResult {
    /// Column descriptors (name/decltype); currently unused by callers.
    #[serde(rename = "cols")]
    _cols: Vec<Value>,
    /// Result rows as `TursoValue` cells in column order.
    rows: Vec<Vec<TursoValue>>,
}

/// Hrana error object carried by [`StreamResult::Error`].
#[derive(Debug, Deserialize)]
struct TursoError {
    /// Human-readable SQL/protocol message when provided.
    message: Option<String>,
    /// Machine-readable error code when provided.
    code: Option<String>,
}

impl TursoError {
    fn message(&self) -> String {
        match (&self.code, &self.message) {
            (Some(code), Some(message)) => format!("{code}: {message}"),
            (Some(code), None) => code.clone(),
            (None, Some(message)) => message.clone(),
            (None, None) => String::from("unknown SQL error"),
        }
    }
}
