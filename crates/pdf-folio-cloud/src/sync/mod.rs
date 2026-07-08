//! PDF-Folio sync client primitives.
//!
//! This crate keeps Google sign-in, sync-server sessions, R2 blob transfer, and
//! Turso credential retrieval out of the UI crate.

pub mod blobs;
pub mod auth;
pub mod session;
pub mod sync;
pub mod status;
pub mod remote;

pub use blobs::BlobCache;
pub use auth::{sign_in_with_google, GoogleAuthConfig};
pub use blobs::{R2Client, R2DownloadResponse, R2UploadResponse};
pub use session::{cached_session, save_session, Session};
pub use status::{
    SyncBlobUploadReport, SyncCheckpoint, SyncCrdtPreflight, SyncCrdtReport, SyncHydrationReport,
    SyncLibraryRow, SyncPlan, SyncRunReport, REGISTRY_LIBRARY_ID,
};
pub use sync::SyncClient;
pub use remote::{TursoClient, TursoToken};
