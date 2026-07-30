use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::session::Session;

/// Short-lived Turso access details returned by the sync server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TursoToken {
    /// libSQL database URL.
    pub database_url: String,
    /// Auth token for the database.
    pub auth_token: String,
    /// Server-side expiration timestamp.
    pub expires_at: DateTime<Utc>,
}

/// One SQL-over-HTTP cell value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TursoValue {
    /// SQL NULL.
    Null,
    /// Integer values are encoded as strings by Hrana.
    Integer { value: String },
    /// Floating-point values are encoded as numbers by Hrana.
    Float { value: String },
    /// Text value.
    Text { value: String },
    /// Base64 blob value.
    Blob { base64: String },
}

impl TursoValue {
    /// Creates a text value.
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text {
            value: value.into(),
        }
    }

    /// Creates an integer value.
    pub fn integer(value: i64) -> Self {
        Self::Integer {
            value: value.to_string(),
        }
    }

    /// Creates a nullable text value.
    pub fn nullable_text(value: Option<&str>) -> Self {
        value.map(Self::text).unwrap_or(Self::Null)
    }

    /// Creates a nullable integer value.
    pub fn nullable_integer(value: Option<i64>) -> Self {
        value.map(Self::integer).unwrap_or(Self::Null)
    }

    /// Reads the value as a string.
    pub fn as_string(&self) -> Result<String> {
        match self {
            Self::Text { value } | Self::Integer { value } | Self::Float { value } => {
                Ok(value.clone())
            }
            Self::Null => bail!("Expected value but found NULL."),
            Self::Blob { .. } => bail!("Expected text/integer but found blob."),
        }
    }

    /// Reads the value as an optional string.
    pub fn as_optional_string(&self) -> Result<Option<String>> {
        match self {
            Self::Null => Ok(None),
            _ => self.as_string().map(Some),
        }
    }

    /// Reads the value as an integer.
    pub fn as_i64(&self) -> Result<i64> {
        self.as_string()?
            .parse()
            .context("Turso integer value was not a valid i64.")
    }

    /// Reads the value as an optional integer.
    pub fn as_optional_i64(&self) -> Result<Option<i64>> {
        match self {
            Self::Null => Ok(None),
            _ => self.as_i64().map(Some),
        }
    }
}

/// A remote Turso database connection over SQL-over-HTTP.
#[derive(Debug, Clone)]
pub struct TursoRemote {
    http: reqwest::Client,
    database_url: String,
    auth_token: String,
}

impl TursoRemote {
    /// Creates a remote from already-resolved credentials.
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

/// Client for asking the sync server for Turso credentials.
#[derive(Debug, Clone)]
pub struct TursoClient {
    http: reqwest::Client,
    session: Session,
}

impl TursoClient {
    /// Creates a Turso credential client from a cached session.
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
        let url = format!(
            "{}/token/turso",
            self.session.server_base_url.trim_end_matches('/')
        );
        self.http
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
            .context("Sync server returned an invalid Turso token response.")
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

#[derive(Debug, Serialize)]
struct PipelineRequest {
    baton: Option<String>,
    requests: Vec<StreamRequest>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamRequest {
    Execute { stmt: Statement },
    Sequence { sql: String },
    Close,
}

#[derive(Debug, Serialize)]
struct Statement {
    sql: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<TursoValue>,
}

#[derive(Debug, Deserialize)]
struct PipelineResponse {
    results: Vec<StreamResult>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamResult {
    Ok { response: StreamResponse },
    Error { error: TursoError },
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamResponse {
    Execute {
        result: StatementResult,
    },
    Sequence,
    Close,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ExecuteResponse {
    result: StatementResult,
}

#[derive(Debug, Deserialize)]
struct StatementResult {
    #[serde(rename = "cols")]
    _cols: Vec<Value>,
    rows: Vec<Vec<TursoValue>>,
}

#[derive(Debug, Deserialize)]
struct TursoError {
    message: Option<String>,
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
