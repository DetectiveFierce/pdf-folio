//! `pdf-folio sync` subcommands: auth, plan, push/pull, blobs, sync-once.
//!
//! CLI surface for the sync **client** product. Wired from `pdf-folio-main` as
//! the `sync` subcommand group. Subcommands map onto [`super::auth`],
//! [`super::client::SyncClient`] methods in [`super::crdt`], and local
//! library-registry files under the XDG data dir (`libraries.json`).
//!
//! # Subcommands (overview)
//!
//! | Command | Role |
//! | --- | --- |
//! | `health` | `GET /health` on the control plane |
//! | `auth` | Google PKCE; cache session |
//! | `status` | Print cached session validity |
//! | `ensure-schema` | Apply embedded Turso schema via session credentials |
//! | `seed` / `plan` | Local sync metadata prep / push plan |
//! | `push` / `pull` | Relational metadata tables (checkpoint-based) |
//! | `upload-blobs` / `download-blobs` | R2 transfer via session |
//! | `sync-once` | Upload blobs + CRDT metadata + hydration |
//!
//! Global flags: `--server` (`PDF_FOLIO_SYNC_SERVER`), optional `--library-id`,
//! `--device-id`, `--db`.
//!
//! # Related
//!
//! - Automatic UI path: [`super::run::SyncClient::sync_library_if_needed`]
//! - Control plane: [`crate::server`]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use pdf_folio_core::Db;
use serde::{Deserialize, Serialize};

use crate::sync::{
    cached_session, sign_in_with_google, BlobCache, GoogleAuthConfig, R2Client, SyncClient,
    SyncLibraryRow,
};

/// CLI arguments for `pdf-folio sync`, including server URL and library targeting.
#[derive(Debug, Parser)]
pub struct SyncArgs {
    /// Sync command to run.
    #[command(subcommand)]
    command: SyncCommand,
    /// Sync server base URL.
    #[arg(
        long,
        env = "PDF_FOLIO_SYNC_SERVER",
        default_value = "http://mind-palace:53148"
    )]
    server: String,
    /// Local library id to sync. Omit to sync all libraries in the app registry.
    #[arg(long)]
    library_id: Option<String>,
    /// Stable local device id for checkpoints.
    #[arg(long)]
    device_id: Option<String>,
    /// Explicit local library database path.
    #[arg(long)]
    db: Option<PathBuf>,
}

/// Subcommands available under `pdf-folio sync`.
#[derive(Debug, Subcommand)]
enum SyncCommand {
    /// Check the sync server health endpoint.
    Health,
    /// Start Google sign-in and cache a sync session.
    Auth {
        /// Google OAuth desktop client id.
        #[arg(long, env = "PDF_FOLIO_GOOGLE_CLIENT_ID")]
        client_id: Option<String>,
    },
    /// Print cached sync session status.
    Status,
    /// Apply the remote Turso sync schema using a cached sync session.
    EnsureSchema,
    /// Seed local sync metadata from local library records.
    Seed,
    /// Show how many local metadata rows would be pushed.
    Plan,
    /// Push local metadata rows to Turso.
    Push,
    /// Pull remote metadata rows from Turso into local sync metadata tables.
    Pull,
    /// Upload all current local PDF blobs to R2 if missing.
    UploadBlobs,
    /// Download pulled PDF blobs from R2 into the local sync blob cache.
    DownloadBlobs,
    /// Run the current manual sync sequence: seed, upload blobs, push, pull, download blobs.
    SyncOnce,
}
/// Dispatches a `pdf-folio sync <subcommand>` invocation.
///
/// # Errors
///
/// Returns an error when auth, network, local DB, or remote sync operations fail.
/// Missing sessions typically suggest running `pdf-folio sync auth` first.
pub async fn run_sync_command(args: SyncArgs) -> Result<()> {
    match args.command {
        SyncCommand::Health => {
            let url = format!("{}/health", args.server.trim_end_matches('/'));
            let health = reqwest::get(&url)
                .await
                .with_context(|| format!("Could not reach {url}."))?
                .error_for_status()
                .with_context(|| format!("Sync server health check failed at {url}."))?
                .text()
                .await?;
            println!("{health}");
        }
        SyncCommand::Auth { client_id } => {
            let client_id = client_id
                .or_else(load_google_client_id_from_secrets)
                .context("Provide --client-id or PDF_FOLIO_GOOGLE_CLIENT_ID.")?;
            let session = sign_in_with_google(&GoogleAuthConfig {
                client_id,
                sync_server_base_url: args.server,
            })
            .await?;
            println!(
                "Signed in as {}. Session expires at {}.",
                session
                    .email
                    .as_deref()
                    .unwrap_or(session.google_sub.as_str()),
                session.expires_at
            );
        }
        SyncCommand::Status => {
            let session =
                cached_session().context("No cached sync session. Run `pdf-folio sync auth`.")?;
            println!("server: {}", session.server_base_url);
            println!(
                "account: {}",
                session
                    .email
                    .as_deref()
                    .unwrap_or(session.google_sub.as_str())
            );
            println!("expires_at: {}", session.expires_at);
            println!("valid: {}", session.is_valid());
        }
        SyncCommand::EnsureSchema => {
            let client = sync_client()?;
            client.ensure_remote_schema().await?;
            println!("Remote Turso schema is ready.");
        }
        SyncCommand::Seed => {
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), false).await?;
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let summary = db.seed_sync_metadata(&profile.id)?;
                println!(
                    "Seeded `{}`: {} entries, {} folders, {} memberships.",
                    profile.name, summary.entries, summary.folders, summary.entry_folders
                );
            }
        }
        SyncCommand::Plan => {
            // Local-only: no authenticated session required.
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), false).await?;
            let device_id = args.device_id.unwrap_or_else(default_device_id);
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let plan = SyncClient::plan_push(&db, &profile.id, &device_id)?;
                println!(
                    "Push plan for `{}` ({}) on `{}`: {} entries, {} folders, {} memberships.",
                    profile.name,
                    profile.id,
                    device_id,
                    plan.entries_to_push,
                    plan.folders_to_push,
                    plan.memberships_to_push
                );
            }
        }
        SyncCommand::Push => {
            let client = sync_client()?;
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), false).await?;
            if args.library_id.is_none() {
                let pushed = client.push_libraries(&library_rows(&profiles)).await?;
                println!("Pushed {pushed} library records.");
            }
            let device_id = args.device_id.unwrap_or_else(default_device_id);
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let plan = client
                    .push_local_metadata(&db, &profile.id, &device_id)
                    .await?;
                println!(
                    "Pushed `{}`: {} entries, {} folders, {} memberships.",
                    profile.name,
                    plan.entries_to_push,
                    plan.folders_to_push,
                    plan.memberships_to_push
                );
            }
        }
        SyncCommand::Pull => {
            let client = sync_client()?;
            let profiles =
                sync_profiles_from_remote_if_needed(args.db, args.library_id.as_deref(), &client)
                    .await?;
            let device_id = args.device_id.unwrap_or_else(default_device_id);
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let plan = client
                    .pull_remote_metadata(&db, &profile.id, &device_id)
                    .await?;
                println!(
                    "Pulled `{}`: {} entries, {} folders, {} memberships.",
                    profile.name,
                    plan.entries_to_push,
                    plan.folders_to_push,
                    plan.memberships_to_push
                );
            }
        }
        SyncCommand::UploadBlobs => {
            let client = sync_client()?;
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), false).await?;
            let cache = BlobCache::open_default()?;
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let upload = client.upload_local_blobs(&db, &cache).await?;
                println!(
                    "Blob upload for `{}` complete: {} uploaded, {} already remote, {} skipped, {} failed. Cache: {}",
                    profile.name,
                    upload.uploaded_blobs,
                    upload.already_remote_blobs,
                    upload.skipped_blobs,
                    upload.failed_blobs,
                    cache.root().display()
                );
            }
        }
        SyncCommand::DownloadBlobs => {
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), true).await?;
            let session =
                cached_session().context("No cached sync session. Run `pdf-folio sync auth`.")?;
            let r2 = R2Client::new(session);
            let cache = BlobCache::open_default()?;
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let (downloaded, cached, skipped) =
                    download_blobs(&db, &r2, &cache, &profile.id).await?;
                println!(
                    "Blob download for `{}` complete: {downloaded} downloaded, {cached} cached, {skipped} skipped. Cache: {}",
                    profile.name,
                    cache.root().display()
                );
            }
        }
        SyncCommand::SyncOnce => {
            let client = sync_client()?;
            let profiles =
                sync_profiles_for_sync_once(args.db, args.library_id.as_deref(), &client).await?;
            let device_id = args.device_id.unwrap_or_else(default_device_id);
            if args.library_id.is_none() {
                let pushed = client.push_libraries(&library_rows(&profiles)).await?;
                println!("Synced {pushed} library records.");
            }
            let cache = BlobCache::open_default()?;
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let upload = client.upload_local_blobs(&db, &cache).await?;
                let report = client
                    .sync_crdt_metadata(&db, &profile.id, &device_id)
                    .await?;
                let hydration = client
                    .hydrate_remote_library(&db, &profile.id, &cache)
                    .await?;
                println!(
                    "`{}`: uploaded {} blobs, {} already remote, {} skipped, {} failed.",
                    profile.name,
                    upload.uploaded_blobs,
                    upload.already_remote_blobs,
                    upload.skipped_blobs,
                    upload.failed_blobs
                );
                println!(
                    "`{}`: CRDT metadata sync generated {}, pushed {}, pulled {} operations.",
                    profile.name,
                    report.generated_operations,
                    report.pushed_operations,
                    report.pulled_operations
                );
                println!(
                    "`{}`: materialized {} entries, {} folders, {} memberships.",
                    profile.name,
                    report.materialized_entries,
                    report.materialized_folders,
                    report.materialized_memberships
                );
                println!(
                    "`{}`: hydrated {} entries, healed {} PDFs, {} folders, {} memberships; downloaded {} blobs, {} already cached, {} skipped. Cache: {}",
                    profile.name,
                    hydration.hydrated_entries,
                    hydration.relinked_entries,
                    hydration.hydrated_folders,
                    hydration.hydrated_memberships,
                    hydration.downloaded_blobs,
                    hydration.cached_blobs,
                    hydration.skipped_entries,
                    cache.root().display()
                );
            }
        }
    }
    Ok(())
}

/// Downloads non-deleted sync entry PDFs from R2 into the local blob cache.
///
/// Returns `(downloaded, already_cached, skipped)` counts. Skips tombstoned rows and non-hash ids.
///
/// # Errors
///
/// Returns an error when local metadata cannot be read or an R2 download fails.
async fn download_blobs(
    db: &Db,
    r2: &R2Client,
    cache: &BlobCache,
    library_id: &str,
) -> Result<(usize, usize, usize)> {
    let entries = db.sync_entries_updated_since(library_id, 0)?;
    let mut downloaded = 0_usize;
    let mut cached = 0_usize;
    let mut skipped = 0_usize;
    for entry in entries {
        if entry.deleted_at.is_some() {
            skipped += 1;
            continue;
        }
        let hash = entry.id.as_str();
        if !is_blob_hash(hash) {
            skipped += 1;
            continue;
        }
        if cache.contains(hash) {
            cached += 1;
            continue;
        }
        r2.download_pdf(hash, &cache.path_for_hash(hash)).await?;
        downloaded += 1;
    }
    Ok((downloaded, cached, skipped))
}

/// Builds a [`SyncClient`] from the cached session JWT (requires prior `sync auth`).
///
/// # Errors
///
/// Returns an error when no valid session is cached on disk.
fn sync_client() -> Result<SyncClient> {
    let session = cached_session().context("No cached sync session. Run `pdf-folio sync auth`.")?;
    Ok(SyncClient::new(session))
}

/// On-disk multi-library registry (`libraries.json` under the XDG data dir).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredLibraryRegistry {
    /// Currently selected library id in the desktop app.
    active_library_id: String,
    /// Known local library profiles (id, name, db path).
    libraries: Vec<SyncLibraryProfile>,
    /// Library ids tombstoned remotely/locally so merge does not re-create them.
    #[serde(default)]
    deleted_library_ids: Vec<String>,
}

/// One local library entry in the app registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SyncLibraryProfile {
    /// Stable library id (stream key for CRDT and Turso).
    id: String,
    /// User-visible library name.
    name: String,
    /// Absolute path to the library SQLite database.
    db_path: PathBuf,
}

/// Resolves libraries for `sync-once`, seeding the registry from remote when none exist locally.
///
/// # Errors
///
/// Returns an error when local registry I/O or remote library pull fails.
async fn sync_profiles_for_sync_once(
    explicit_db: Option<PathBuf>,
    library_id: Option<&str>,
    client: &SyncClient,
) -> Result<Vec<SyncLibraryProfile>> {
    if explicit_db.is_some() {
        return sync_profiles(explicit_db, library_id, false).await;
    }
    let mut profiles = sync_profiles(None, library_id, false).await?;
    if profiles.is_empty() {
        profiles = sync_profiles_from_remote_if_needed(None, library_id, client).await?;
    }
    Ok(profiles)
}

/// Pulls remote library rows into `libraries.json` when the local registry is empty/absent.
///
/// # Errors
///
/// Returns an error when remote pull or registry merge/write fails.
async fn sync_profiles_from_remote_if_needed(
    explicit_db: Option<PathBuf>,
    library_id: Option<&str>,
    client: &SyncClient,
) -> Result<Vec<SyncLibraryProfile>> {
    if explicit_db.is_some() {
        return sync_profiles(explicit_db, library_id, true).await;
    }
    let remote_libraries = client.pull_libraries().await?;
    if !remote_libraries.is_empty() {
        merge_remote_libraries_into_registry(&remote_libraries)?;
    }
    sync_profiles(None, library_id, true).await
}

/// Loads library profiles from `--db` / registry, optionally filtered by `library_id`.
///
/// When `create_missing` is true, ensures each profile’s SQLite file exists (opens/creates).
///
/// # Errors
///
/// Returns an error when the data dir, registry, or library database cannot be accessed.
async fn sync_profiles(
    explicit_db: Option<PathBuf>,
    library_id: Option<&str>,
    create_missing: bool,
) -> Result<Vec<SyncLibraryProfile>> {
    if let Some(db_path) = explicit_db {
        let id = library_id.unwrap_or("default").to_owned();
        return Ok(vec![SyncLibraryProfile {
            name: id.clone(),
            id,
            db_path,
        }]);
    }

    let data_dir = app_data_dir()?;
    let mut registry = load_stored_library_registry(&data_dir)?;
    if create_missing {
        for profile in &registry.libraries {
            if let Some(parent) = profile.db_path.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("Could not create library directory {}.", parent.display())
                })?;
            }
            Db::open(&profile.db_path)?;
        }
    }
    registry.libraries.sort_by(|left, right| {
        (left.id != "default")
            .cmp(&(right.id != "default"))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(registry
        .libraries
        .into_iter()
        .filter(|profile| library_id.is_none_or(|id| profile.id == id))
        .collect())
}

/// Merges remote library rows into the local registry (add/update, apply deletions).
///
/// # Errors
///
/// Returns an error when registry load/save or local storage removal fails.
fn merge_remote_libraries_into_registry(remote_libraries: &[SyncLibraryRow]) -> Result<()> {
    let data_dir = app_data_dir()?;
    let had_registry_file = registry_path()?.exists();
    let mut registry = load_stored_library_registry(&data_dir)?;
    let preferred_remote_active = remote_libraries
        .iter()
        .find(|row| row.deleted_at.is_none() && row.id != "default")
        .or_else(|| remote_libraries.iter().find(|row| row.deleted_at.is_none()))
        .map(|row| row.id.clone());
    for row in remote_libraries {
        if row.deleted_at.is_some() {
            if !registry.deleted_library_ids.iter().any(|id| id == &row.id) {
                registry.deleted_library_ids.push(row.id.clone());
            }
            if let Some(index) = registry
                .libraries
                .iter()
                .position(|profile| profile.id == row.id)
            {
                let removed = registry.libraries.remove(index);
                let _ = remove_library_storage(&removed.db_path);
            }
            continue;
        }

        if registry.deleted_library_ids.iter().any(|id| id == &row.id) {
            continue;
        }

        let db_path = local_library_db_path(&data_dir, &row.id);
        if let Some(profile) = registry
            .libraries
            .iter_mut()
            .find(|profile| profile.id == row.id)
        {
            profile.db_path = db_path;
        } else {
            registry.libraries.push(SyncLibraryProfile {
                id: row.id.clone(),
                name: row.name.clone(),
                db_path,
            });
        }
    }
    if !had_registry_file {
        if let Some(active_library_id) = preferred_remote_active {
            registry.active_library_id = active_library_id;
        }
    } else if registry
        .libraries
        .iter()
        .all(|profile| profile.id != registry.active_library_id)
    {
        registry.active_library_id = registry
            .libraries
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| String::from("default"));
    }
    save_stored_library_registry(&registry)
}

/// Loads `libraries.json`, or synthesizes a default profile from a legacy `library.db`.
///
/// # Errors
///
/// Returns an error when the registry file cannot be read or parsed.
fn load_stored_library_registry(data_dir: &Path) -> Result<StoredLibraryRegistry> {
    let path = data_dir.join("libraries.json");
    if path.exists() {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("Could not read {}.", path.display()))?;
        let mut registry = serde_json::from_str::<StoredLibraryRegistry>(&json)
            .with_context(|| format!("Could not parse {}.", path.display()))?;
        for profile in &mut registry.libraries {
            if profile.db_path.is_relative() || !profile.db_path.starts_with(data_dir) {
                profile.db_path = local_library_db_path(data_dir, &profile.id);
            }
        }
        return Ok(registry);
    }

    let default_path = data_dir.join("library.db");
    let libraries = if default_path.exists() {
        vec![SyncLibraryProfile {
            id: String::from("default"),
            name: String::from("Default Library"),
            db_path: default_path,
        }]
    } else {
        Vec::new()
    };
    Ok(StoredLibraryRegistry {
        active_library_id: libraries
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| String::from("default")),
        libraries,
        deleted_library_ids: Vec::new(),
    })
}

/// Writes the registry JSON pretty-printed to the XDG data path.
///
/// # Errors
///
/// Returns an error when parent dirs cannot be created or the file cannot be written.
fn save_stored_library_registry(registry: &StoredLibraryRegistry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(registry)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

/// Maps local profiles to relational [`SyncLibraryRow`] values for Turso library push.
fn library_rows(profiles: &[SyncLibraryProfile]) -> Vec<SyncLibraryRow> {
    let registry_updated_at = registry_path()
        .ok()
        .and_then(|path| file_modified_unix_timestamp(&path))
        .unwrap_or_else(current_unix_timestamp);
    profiles
        .iter()
        .map(|profile| SyncLibraryRow {
            id: profile.id.clone(),
            name: profile.name.clone(),
            updated_at: file_modified_unix_timestamp(&profile.db_path)
                .unwrap_or(registry_updated_at),
            deleted_at: None,
        })
        .collect()
}

/// Current wall-clock Unix seconds (0 if the system clock is before the epoch).
fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// File mtime as Unix seconds, when available.
fn file_modified_unix_timestamp(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
}

/// XDG data directory for PDF-Folio (`…/PDF-Folio`).
///
/// # Errors
///
/// Returns an error when the platform project dirs cannot be resolved.
fn app_data_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().to_path_buf())
}

/// Path to the multi-library registry JSON under the app data dir.
///
/// # Errors
///
/// Returns an error when the app data dir cannot be resolved.
fn registry_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("libraries.json"))
}

/// Canonical SQLite path for a library id (`library.db` for `default`, else `libraries/<id>/…`).
fn local_library_db_path(data_dir: &Path, library_id: &str) -> PathBuf {
    if library_id == "default" {
        data_dir.join("library.db")
    } else {
        data_dir
            .join("libraries")
            .join(library_id)
            .join("library.db")
    }
}

/// Removes a library database file and its empty parent directory after remote tombstone merge.
///
/// # Errors
///
/// Returns an error when deletion fails for a reason other than “not found” / non-empty dir.
fn remove_library_storage(db_path: &Path) -> Result<()> {
    match std::fs::remove_file(db_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("Could not remove {}.", db_path.display()));
        }
    }
    if let Some(parent) = db_path.parent() {
        match std::fs::remove_dir(parent) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Could not remove {}.", parent.display()));
            }
        }
    }
    Ok(())
}

/// True when `value` is a 64-character hex string (BLAKE3 entry/blob id).
fn is_blob_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Default sync device id from `/etc/hostname`, else `local-device`.
fn default_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

/// Reads Google OAuth desktop `client_id` from a local `secrets/client_secret_*.json` if present.
fn load_google_client_id_from_secrets() -> Option<String> {
    let secrets_dir = Path::new("secrets");
    let path = std::fs::read_dir(secrets_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("client_secret_") && name.ends_with(".json"))
        })?;
    let json = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&json).ok()?;
    value
        .get("installed")
        .and_then(|installed| installed.get("client_id"))
        .and_then(|client_id| client_id.as_str())
        .map(str::to_owned)
}
