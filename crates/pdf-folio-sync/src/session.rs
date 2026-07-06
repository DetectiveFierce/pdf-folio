use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Cached PDF-Folio sync server session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Tailscale-reachable sync server base URL.
    pub server_base_url: String,
    /// Bearer JWT issued by `folio-sync-server`.
    pub session_token: String,
    /// Session expiration timestamp.
    pub expires_at: DateTime<Utc>,
    /// Authorized Google account subject.
    pub google_sub: String,
    /// Authorized Google email, when available.
    pub email: Option<String>,
}

impl Session {
    /// Returns true when the cached session is still usable.
    pub fn is_valid(&self) -> bool {
        self.expires_at > Utc::now()
    }
}

/// Loads the cached sync session.
///
/// # Errors
///
/// Returns an error when the cache file is missing or invalid.
pub fn cached_session() -> Result<Session> {
    let path = session_cache_path()?;
    let json =
        fs::read_to_string(&path).with_context(|| format!("Could not read {}.", path.display()))?;
    serde_json::from_str::<Session>(&json)
        .with_context(|| format!("Could not parse {}.", path.display()))
}

/// Saves a sync session in PDF-Folio's data directory.
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

fn session_cache_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().join("sync").join("session.json"))
}
