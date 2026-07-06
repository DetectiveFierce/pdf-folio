//! PDF-Folio binary entrypoint.
//!
//! This crate produces the `pdf-folio` executable. It initializes the
//! tracing subscriber, parses command-line arguments via [`clap`], and
//! delegates to [`pdf_folio_ui::run`] to launch the application.
//!
//! Usage:
//!
//! ```text
//! pdf-folio               # open the library manager
//! pdf-folio document.pdf  # open a PDF directly
//! ```
//!
//! [`clap`]: https://docs.rs/clap

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use pdf_folio_db::Db;
use pdf_folio_sync::{
    cached_session, sign_in_with_google, BlobCache, GoogleAuthConfig, R2Client, SyncClient,
    SyncLibraryRow,
};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

/// Command-line arguments for PDF-Folio.
#[derive(Debug, Parser)]
#[command(
    name = "pdf-folio",
    version,
    about = "Native PDF viewer and library manager"
)]
struct Args {
    /// Maintenance and sync commands.
    #[command(subcommand)]
    command: Option<Command>,
    /// PDF file to open at startup.
    file: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage PDF-Folio sync.
    Sync(SyncArgs),
}

#[derive(Debug, Parser)]
struct SyncArgs {
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

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    match args.command {
        Some(Command::Sync(sync)) => {
            tokio::runtime::Runtime::new()?.block_on(run_sync_command(sync))
        }
        None => pdf_folio_ui::run(args.file),
    }
}

async fn run_sync_command(args: SyncArgs) -> Result<()> {
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
            let profiles = sync_profiles(args.db, args.library_id.as_deref(), false).await?;
            let device_id = args.device_id.unwrap_or_else(default_device_id);
            for profile in profiles {
                let db = Db::open(&profile.db_path)?;
                let checkpoint = db.sync_checkpoint(&profile.id, &device_id)?.unwrap_or(0);
                let entries_to_push = db
                    .sync_entries_updated_since(&profile.id, checkpoint)?
                    .len();
                let folders_to_push = db
                    .sync_folders_updated_since(&profile.id, checkpoint)?
                    .len();
                let memberships_to_push = db
                    .sync_entry_folders_updated_since(&profile.id, checkpoint)?
                    .len();
                println!(
                    "Push plan for `{}` ({}) on `{}`: {} entries, {} folders, {} memberships.",
                    profile.name,
                    profile.id,
                    device_id,
                    entries_to_push,
                    folders_to_push,
                    memberships_to_push
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

fn sync_client() -> Result<SyncClient> {
    let session = cached_session().context("No cached sync session. Run `pdf-folio sync auth`.")?;
    Ok(SyncClient::new(session))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredLibraryRegistry {
    active_library_id: String,
    libraries: Vec<SyncLibraryProfile>,
    #[serde(default)]
    deleted_library_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SyncLibraryProfile {
    id: String,
    name: String,
    db_path: PathBuf,
}

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

fn library_rows(profiles: &[SyncLibraryProfile]) -> Vec<SyncLibraryRow> {
    let updated_at = Utc::now().timestamp();
    profiles
        .iter()
        .map(|profile| SyncLibraryRow {
            id: profile.id.clone(),
            name: profile.name.clone(),
            updated_at,
            deleted_at: None,
        })
        .collect()
}

fn app_data_dir() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().to_path_buf())
}

fn registry_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("libraries.json"))
}

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

fn is_blob_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn default_device_id() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("local-device"))
}

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
