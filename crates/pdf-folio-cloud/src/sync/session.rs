//! Cached sync session load/save under the XDG data directory.
//!
//! After a successful Google PKCE flow ([`super::auth::sign_in_with_google`]),
//! the control plane’s session JWT and metadata are serialized to
//! `$XDG_DATA_HOME/…/PDF-Folio/sync/session.json` (via `directories::ProjectDirs`).
//! All subsequent control-plane calls ([`super::remote::TursoClient`],
//! [`super::blobs::R2Client`]) read this cache.
//!
//! # Security notes
//!
//! The file contains a long-lived bearer JWT. Treat the data directory as
//! private. There is no refresh flow beyond re-running sign-in when
//! [`Session::is_valid`] is false or the server rejects the token.
//!
//! # Related
//!
//! - Written by: [`super::auth`]
//! - Consumed by: [`super::client::SyncClient`], CLI status/auth commands

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Cached PDF-Folio sync server session (JWT + identity metadata).
///
/// Constructed by the OAuth callback path and reused until `expires_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Tailscale-reachable sync server base URL.
    pub server_base_url: String,
    /// Bearer JWT issued by `pdf-folio-sync-server`.
    pub session_token: String,
    /// Session expiration timestamp (from the control plane).
    pub expires_at: DateTime<Utc>,
    /// Authorized Google account subject.
    pub google_sub: String,
    /// Authorized Google email, when available.
    pub email: Option<String>,
}

impl Session {
    /// Returns true when `expires_at` is still in the future.
    ///
    /// Does not contact the server; an allow-list or secret rotation can still
    /// invalidate a non-expired token.
    pub fn is_valid(&self) -> bool {
        self.expires_at > Utc::now()
    }
}

/// Loads the cached sync session from the PDF-Folio data directory.
///
/// # Errors
///
/// Returns an error when the cache file is missing or invalid JSON.
pub fn cached_session() -> Result<Session> {
    let path = session_cache_path()?;
    let json =
        fs::read_to_string(&path).with_context(|| format!("Could not read {}.", path.display()))?;
    serde_json::from_str::<Session>(&json)
        .with_context(|| format!("Could not parse {}.", path.display()))
}

/// Saves a sync session under `…/sync/session.json` in PDF-Folio's data directory.
///
/// Creates parent directories as needed. Overwrites any previous session.
///
/// # Errors
///
/// Returns an error when the cache directory or file cannot be written.
pub fn save_session(session: &Session) -> Result<()> {
    let path = session_cache_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    fs::write(&path, serde_json::to_vec_pretty(session)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

/// Path to the session JWT cache file (`…/sync/session.json` under the app data dir).
///
/// # Errors
///
/// Returns an error when the platform data directory cannot be resolved.
fn session_cache_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().join("sync").join("session.json"))
}
