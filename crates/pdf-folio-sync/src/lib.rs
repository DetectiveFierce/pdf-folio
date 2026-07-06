//! PDF-Folio sync client primitives.
//!
//! This crate keeps Google sign-in, sync-server sessions, R2 blob transfer, and
//! Turso credential retrieval out of the UI crate.

pub mod blob_cache;
pub mod google_auth;
pub mod r2_client;
pub mod session;
pub mod sync;
pub mod turso_client;

pub use blob_cache::BlobCache;
pub use google_auth::{sign_in_with_google, GoogleAuthConfig};
pub use r2_client::{R2Client, R2DownloadResponse, R2UploadResponse};
pub use session::{cached_session, save_session, Session};
pub use sync::{
    SyncBlobUploadReport, SyncCheckpoint, SyncClient, SyncCrdtReport, SyncHydrationReport,
    SyncLibraryRow, SyncPlan, REGISTRY_LIBRARY_ID,
};
pub use turso_client::{TursoClient, TursoToken};
