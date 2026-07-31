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

/// Soft stack limit requested for the iced UI main thread (bytes).
///
/// `ThemeTokens` is ~140 KiB and is passed by value through deep library view
/// builders. The default soft limit (~8 MiB) overflows on first paint with a
/// populated folder tree.
const UI_STACK_SOFT_LIMIT: u64 = 64 * 1024 * 1024;

/// Raises the process soft stack limit so the main thread can grow past the
/// default (~8 MiB) before iced builds the first frame.
///
/// No-ops on non-Unix targets or when the hard limit is already lower.
fn raise_stack_limit() {
    #[cfg(unix)]
    {
        // SAFETY: setrlimit is process-wide and only affects RLIMIT_STACK.
        unsafe {
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::getrlimit(libc::RLIMIT_STACK, &mut limit) != 0 {
                return;
            }
            let desired = UI_STACK_SOFT_LIMIT;
            if limit.rlim_cur >= desired {
                return;
            }
            limit.rlim_cur = desired.min(limit.rlim_max);
            let _ = libc::setrlimit(libc::RLIMIT_STACK, &limit);
        }
    }
}

/// Process entry: install tracing, parse CLI, run UI or sync subcommand.
///
/// The iced UI must run on the main thread (winit). Stack limit is raised
/// before the event loop so deep view builders do not overflow.
fn main() -> Result<()> {
    raise_stack_limit();

    let process_started_at = std::time::Instant::now();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let args = Args::parse();
    match args.command {
        Some(Command::Sync(sync)) => {
            tokio::runtime::Runtime::new()?.block_on(run_sync_command(sync))
        }
        None => pdf_folio_ui::run_with_process_start(args.file, process_started_at),
    }
}
