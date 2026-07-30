//! `pdf-folio` binary entrypoint: tracing, CLI parse, then UI or sync.
//!
//! This crate must stay thin. It owns process startup only — no database,
//! Pdfium, or domain logic. Real work lives in `pdf-folio-ui` (desktop) and
//! `pdf-folio-cloud` (sync CLI).
//!
//! # CLI surface
//!
//! ```text
//! pdf-folio                 # open the library manager UI
//! pdf-folio document.pdf    # open the UI and that PDF
//! pdf-folio sync <COMMAND>  # cloud sync maintenance (async Tokio)
//! ```
//!
//! Sync subcommands (`auth`, `push`, `pull`, `sync-once`, …) are defined in
//! `pdf_folio_cloud::sync::cli` and re-exported via the local [`cli`] module.
//! See the operations CLI reference for flags (`--server`, `--library-id`, …).
//!
//! # Tracing setup
//!
//! On startup the binary installs a `tracing_subscriber` fmt layer with an
//! [`EnvFilter`](tracing_subscriber::EnvFilter):
//!
//! - `RUST_LOG` when set (standard env-filter syntax)
//! - otherwise `info` for the whole process
//!
//! # Dispatch
//!
//! | Parsed args | Runtime | Destination |
//! | --- | --- | --- |
//! | no subcommand | iced event loop | [`pdf_folio_ui::run`] with optional file path |
//! | `sync …` | `tokio::Runtime::block_on` | [`cli::run_sync_command`] |
//!
//! [`clap`]: https://docs.rs/clap

/// Re-export of cloud sync CLI args and runner for `pdf-folio sync …`.
mod cli;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use cli::{run_sync_command, SyncArgs};
use tracing_subscriber::EnvFilter;

/// Top-level command-line arguments for the `pdf-folio` binary.
///
/// Either a sync subcommand **or** an optional PDF path for the desktop UI
/// (not both as a combined mode — subcommand wins when present).
#[derive(Debug, Parser)]
#[command(
    name = "pdf-folio",
    version,
    about = "Native PDF viewer and library manager"
)]
struct Args {
    /// Maintenance and sync commands (`pdf-folio sync …`).
    #[command(subcommand)]
    command: Option<Command>,
    /// PDF file to open when launching the desktop UI.
    file: Option<PathBuf>,
}

/// Top-level subcommands of `pdf-folio`.
#[derive(Debug, Subcommand)]
enum Command {
    /// Cloud sync client commands (auth, push/pull, blobs, sync-once, …).
    Sync(SyncArgs),
}

/// Process entry: install tracing, parse CLI, run UI or sync subcommand.
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
