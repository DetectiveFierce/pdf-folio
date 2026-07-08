use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use chrono::{DateTime, Utc};
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::session::Session;

/// Upload URL returned by the sync server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct R2UploadResponse {
    /// Whether the blob already exists in R2.
    pub exists: bool,
    /// Presigned PUT URL when an upload is needed.
    pub upload_url: Option<String>,
    /// URL expiration timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Download URL returned by the sync server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct R2DownloadResponse {
    /// Presigned GET URL.
    pub download_url: String,
    /// URL expiration timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Client for direct PDF blob transfers through R2 presigned URLs.
#[derive(Debug, Clone)]
pub struct R2Client {
    http: reqwest::Client,
    session: Session,
}

impl R2Client {
    /// Creates an R2 client from a cached session.
    pub fn new(session: Session) -> Self {
        Self {
            http: reqwest::Client::new(),
            session,
        }
    }

    /// Requests an upload URL and uploads `path` if R2 does not already have the blob.
    ///
    /// # Errors
    ///
    /// Returns an error when token minting or the direct R2 upload fails.
    pub async fn upload_pdf_if_missing(&self, hash: &str, path: &Path) -> Result<R2UploadResponse> {
        let response = self.upload_token(hash).await?;
        if let Some(url) = &response.upload_url {
            let mut file = tokio::fs::File::open(path)
                .await
                .with_context(|| format!("Could not open {}.", path.display()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).await?;
            let upload_response = self
                .http
                .put(url)
                .header("content-type", "application/pdf")
                .body(bytes)
                .send()
                .await
                .context("Could not upload PDF blob to R2.")?;
            if !upload_response.status().is_success() {
                let status = upload_response.status();
                let body = upload_response.text().await.unwrap_or_default();
                bail!("R2 rejected PDF blob upload with HTTP {status}: {body}");
            }
        }
        Ok(response)
    }

    /// Downloads a PDF blob into `destination`.
    ///
    /// # Errors
    ///
    /// Returns an error when token minting, the direct R2 download, or the local write fails.
    pub async fn download_pdf(&self, hash: &str, destination: &Path) -> Result<()> {
        let token = self.download_token(hash).await?;
        let download_response = self
            .http
            .get(token.download_url)
            .send()
            .await
            .context("Could not download PDF blob from R2.")?;
        if !download_response.status().is_success() {
            let status = download_response.status();
            let body = download_response.text().await.unwrap_or_default();
            bail!("R2 rejected PDF blob download with HTTP {status}: {body}");
        }
        let bytes = download_response.bytes().await?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(destination)
            .await
            .with_context(|| format!("Could not create {}.", destination.display()))?;
        file.write_all(&bytes).await?;
        Ok(())
    }

    /// Requests a presigned upload URL.
    pub async fn upload_token(&self, hash: &str) -> Result<R2UploadResponse> {
        let url = format!(
            "{}/token/r2/upload",
            self.session.server_base_url.trim_end_matches('/')
        );
        self.http
            .post(url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.session.session_token),
            )
            .json(&R2UploadRequest { hash })
            .send()
            .await
            .context("Could not request R2 upload token from sync server.")?
            .error_for_status()
            .context("Sync server rejected R2 upload token request.")?
            .json::<R2UploadResponse>()
            .await
            .context("Sync server returned an invalid R2 upload response.")
    }

    /// Requests a presigned download URL.
    pub async fn download_token(&self, hash: &str) -> Result<R2DownloadResponse> {
        let url = format!(
            "{}/token/r2/download?hash={}",
            self.session.server_base_url.trim_end_matches('/'),
            url::form_urlencoded::byte_serialize(hash.as_bytes()).collect::<String>()
        );
        self.http
            .get(url)
            .header(
                AUTHORIZATION,
                format!("Bearer {}", self.session.session_token),
            )
            .send()
            .await
            .context("Could not request R2 download token from sync server.")?
            .error_for_status()
            .context("Sync server rejected R2 download token request.")?
            .json::<R2DownloadResponse>()
            .await
            .context("Sync server returned an invalid R2 download response.")
    }
}

#[derive(Debug, Serialize)]
struct R2UploadRequest<'a> {
    hash: &'a str,
}

/// Content-addressed local PDF blob cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobCache {
    root: PathBuf,
}

impl BlobCache {
    /// Opens the default blob cache under PDF-Folio's data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform data directory cannot be resolved.
    pub fn open_default() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
            .context("Could not find a data directory for PDF-Folio.")?;
        Ok(Self {
            root: project_dirs.data_dir().join("sync").join("blobs"),
        })
    }

    /// Creates a blob cache rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the cache path for a BLAKE3 hash.
    pub fn path_for_hash(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(0..2).unwrap_or("xx");
        self.root.join(prefix).join(format!("{hash}.pdf"))
    }

    /// Returns true when the cache already has this blob.
    pub fn contains(&self, hash: &str) -> bool {
        self.path_for_hash(hash).is_file()
    }

    /// Root directory for the cache.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
