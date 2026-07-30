//! Re-export of the sync CLI surface from `pdf_folio_cloud::sync::cli`.
//!
//! Keeping the import path local to this crate lets `main.rs` stay free of a
//! deep cloud dependency path while still wiring `pdf-folio sync …` to the
//! real implementation. Add new sync subcommands in the cloud crate, not here.
//!
//! - [`SyncArgs`] — clap args for the `sync` subcommand tree
//! - [`run_sync_command`] — async entry that executes the chosen subcommand

pub(crate) use pdf_folio_cloud::sync::cli::{run_sync_command, SyncArgs};
