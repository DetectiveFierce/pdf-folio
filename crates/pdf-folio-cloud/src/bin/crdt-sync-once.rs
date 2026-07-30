//! Maintenance binary that runs a one-shot CRDT sync + hydration pass.
//!
//! Intended for ops and debugging when you want a full metadata exchange without
//! the interactive `pdf-folio sync` CLI. Requires a cached sync session
//! (`pdf-folio sync auth` or equivalent) under the XDG data directory.
//!
//! # Usage
//!
//! ```text
//! crdt-sync-once --db PATH --library-id ID --device-id DEVICE
//! ```
//!
//! # Data flow
//!
//! 1. Open the local library SQLite DB.
//! 2. Load [`pdf_folio_cloud::sync::cached_session`].
//! 3. `ensure_remote_schema`, then `sync_crdt_metadata`, then `hydrate_remote_library`
//!    with the default [`pdf_folio_cloud::sync::BlobCache`].
//! 4. Print CRDT and hydration counters.
//!
//! # Related
//!
//! - Library APIs: `pdf_folio_cloud::sync::{SyncClient, BlobCache}`
//! - Higher-level orchestration: `SyncClient::sync_library_if_needed`
//! - Schema bootstrap: `ensure-turso-schema` binary

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use pdf_folio_cloud::sync::{cached_session, BlobCache, SyncClient};
use pdf_folio_core::Db;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse()?;
    let db = Db::open(&args.db)?;
    let session = cached_session().context("No cached sync session is available.")?;
    let client = SyncClient::new(session);
    client.ensure_remote_schema().await?;
    let crdt = client
        .sync_crdt_metadata(&db, &args.library_id, &args.device_id)
        .await?;
    let cache = BlobCache::open_default()?;
    let hydration = client
        .hydrate_remote_library(&db, &args.library_id, &cache)
        .await?;
    println!(
        "crdt generated={} pushed={} pulled={} materialized_entries={} materialized_folders={} materialized_memberships={}",
        crdt.generated_operations,
        crdt.pushed_operations,
        crdt.pulled_operations,
        crdt.materialized_entries,
        crdt.materialized_folders,
        crdt.materialized_memberships
    );
    println!(
        "hydrated entries={} relinked={} folders={} memberships={} downloaded={} cached={} skipped={} cache={}",
        hydration.hydrated_entries,
        hydration.relinked_entries,
        hydration.hydrated_folders,
        hydration.hydrated_memberships,
        hydration.downloaded_blobs,
        hydration.cached_blobs,
        hydration.skipped_entries,
        cache.root().display()
    );
    Ok(())
}

#[derive(Debug)]
struct Args {
    db: PathBuf,
    library_id: String,
    device_id: String,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut db = None;
        let mut library_id = None;
        let mut device_id = None;
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--db" => db = args.next().map(PathBuf::from),
                "--library-id" => library_id = args.next(),
                "--device-id" => device_id = args.next(),
                "--help" | "-h" => {
                    println!("Usage: crdt_sync_once --db PATH --library-id ID --device-id DEVICE");
                    std::process::exit(0);
                }
                other => anyhow::bail!("Unexpected argument: {other}"),
            }
        }
        Ok(Self {
            db: db.context("Missing --db PATH.")?,
            library_id: library_id.context("Missing --library-id ID.")?,
            device_id: device_id.context("Missing --device-id DEVICE.")?,
        })
    }
}
