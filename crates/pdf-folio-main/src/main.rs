//! PDF-Folio binary entrypoint.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Command-line arguments for PDF-Folio.
#[derive(Debug, Parser)]
#[command(
    name = "pdf-folio",
    version,
    about = "Native PDF viewer and library manager"
)]
struct Args {
    /// PDF file to open at startup.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    pdf_folio_ui::run(args.file)
}
