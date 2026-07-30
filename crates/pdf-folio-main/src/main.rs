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

mod cli;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use cli::{run_sync_command, SyncArgs};
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
