//! R2 blob client and local content-addressed [`BlobCache`] for managed PDFs.
//!
//! PDF bytes never pass through the control plane. [`R2Client`] requests a
//! short-lived presigned URL from the server, then PUT/GET directly against
//! Cloudflare R2. Object keys are content-addressed: `blobs/<blake3>.pdf`,
//! where the hash is also the library entry id for managed PDFs.
//!
//! [`BlobCache`] is the local side of that contract: files live under
//! `…/sync/blobs/<2-hex-prefix>/<hash>.pdf` so uploads and hydration always
//! read a stable path rather than an arbitrary user file location.
//!
//! # Related
//!
//! - Server presign: control-plane `/token/r2/*` and storage helpers
//! - Upload orchestration: [`SyncClient::upload_local_blobs`](super::crdt)
//! - Hydration downloads: [`SyncClient::hydrate_remote_library`](super::crdt)

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use reqwest::header::AUTHORIZATION;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::session::Session;

/// Control-plane JSON from `POST /token/r2/upload` (presign or short-circuit).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct R2UploadResponse {
    /// `true` when R2 already has `blobs/<hash>.pdf` (client may skip PUT).
    pub exists: bool,
    /// Presigned PUT URL when an upload is still required (`None` if fully present).
    pub upload_url: Option<String>,
    /// When the presigned URL (if any) stops being valid.
    pub expires_at: DateTime<Utc>,
}

/// Control-plane JSON from `GET /token/r2/download` for a content hash.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct R2DownloadResponse {
    /// Presigned GET URL for the object at `blobs/<hash>.pdf`.
    pub download_url: String,
    /// When the presigned URL stops being valid.
    pub expires_at: DateTime<Utc>,
}

/// Client for direct PDF blob transfers through R2 presigned URLs.
///
/// Uses the session JWT only to mint URLs; the PDF body is sent/received on
/// the presigned URL without the control plane in the path.
#[derive(Debug, Clone)]
pub struct R2Client {
    /// Shared HTTP client for control-plane and direct R2 requests.
    http: reqwest::Client,
    /// Session JWT + server base URL used to mint presigned URLs.
    session: Session,
}

impl R2Client {
    /// Builds a client that mints R2 URLs with the given sync session JWT.
    pub fn new(session: Session) -> Self {
        Self {
            http: reqwest::Client::new(),
            session,
        }
    }

    /// Presigns upload for `hash` and PUTs local `path` bytes when R2 is missing the object.
    ///
    /// `hash` is the BLAKE3 content id (entry id for managed PDFs). No-ops the body
    /// transfer when the control plane reports the blob already exists.
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

    /// Downloads `blobs/<hash>.pdf` via a presigned GET into `destination`.
    ///
    /// Creates parent directories as needed. Used by hydration to fill [`BlobCache`].
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

    /// Calls the control plane for a presigned upload URL (or `exists: true`).
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

    /// Calls the control plane for a presigned download URL for `hash`.
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

/// Body for `POST /token/r2/upload` on the control plane.
#[derive(Debug, Serialize)]
struct R2UploadRequest<'a> {
    /// BLAKE3 hex content hash of the PDF to upload.
    hash: &'a str,
}

/// Content-addressed local PDF blob cache (`…/sync/blobs`).
///
/// Layout: `{root}/{hash[0..2]}/{hash}.pdf`. Entry ids that are 64-char hex
/// digests map 1:1 onto these paths after managed import/sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobCache {
    /// Root directory of the content-addressed blob tree.
    root: PathBuf,
}

impl BlobCache {
    /// Opens `…/pdf-folio/sync/blobs` under the platform data directory (XDG on Linux).
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

    /// Creates a cache with an explicit root (tests and custom data dirs).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Content-addressed path `{root}/{hash[0..2]}/{hash}.pdf` (prefix `xx` if hash is short).
    pub fn path_for_hash(&self, hash: &str) -> PathBuf {
        let prefix = hash.get(0..2).unwrap_or("xx");
        self.root.join(prefix).join(format!("{hash}.pdf"))
    }

    /// `true` when a regular file already exists at [`Self::path_for_hash`].
    pub fn contains(&self, hash: &str) -> bool {
        self.path_for_hash(hash).is_file()
    }

    /// Absolute root directory that holds the two-hex-prefix layout.
    pub fn root(&self) -> &Path {
        &self.root
    }
}
