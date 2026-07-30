//! PDF-Folio sync client primitives.
//!
//! This crate keeps Google sign-in, sync-server sessions, R2 blob transfer, and
//! Turso credential retrieval out of the UI crate.

pub mod auth;
pub mod blobs;
pub mod cli;
pub mod client;
pub mod crdt;
pub mod remote;
pub mod run;
pub mod session;
pub mod status;

pub use auth::{sign_in_with_google, GoogleAuthConfig};
pub use blobs::BlobCache;
pub use blobs::{R2Client, R2DownloadResponse, R2UploadResponse};
pub use client::SyncClient;
pub use remote::{TursoClient, TursoToken};
pub use session::{cached_session, save_session, Session};
pub use status::{
    SyncBlobUploadReport, SyncCheckpoint, SyncCrdtPreflight, SyncCrdtReport, SyncHydrationReport,
    SyncLibraryRow, SyncPlan, SyncRunReport, REGISTRY_LIBRARY_ID,
};
