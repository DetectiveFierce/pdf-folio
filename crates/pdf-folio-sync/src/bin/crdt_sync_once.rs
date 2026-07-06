use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use pdf_folio_db::Db;
use pdf_folio_sync::{cached_session, BlobCache, SyncClient};

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
