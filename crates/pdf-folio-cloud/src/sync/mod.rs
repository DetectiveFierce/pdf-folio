//! PDF-Folio sync **client** primitives (product one of three in this crate).
//!
//! This module is the desktop-facing half of cross-device sync. It keeps Google
//! sign-in, control-plane session handling, Turso credential retrieval, R2 blob
//! transfer, and CRDT metadata exchange out of the UI crate. The companion
//! control plane lives in [`crate::server`]; Raindrop import is separate in
//! [`crate::raindrop`].
//!
//! # Key types
//!
//! - [`Session`] / [`cached_session`] / [`save_session`] — durable session JWT cache under XDG data
//! - [`GoogleAuthConfig`] / [`sign_in_with_google`] — browser PKCE against the control plane
//! - [`SyncClient`] — high-level coordinator holding a session plus [`TursoClient`] and [`R2Client`]
//! - [`BlobCache`] — local content-addressed PDF store used for managed uploads/downloads
//! - Report types in [`status`] (`SyncPlan`, `SyncCrdtReport`, `SyncRunReport`, …)
//!
//! # Data flow
//!
//! A typical automatic pass (`sync_library_if_needed` in [`run`]):
//!
//! 1. Load a valid [`Session`] (or prompt via [`sign_in_with_google`]).
//! 2. Seed local sync metadata from library rows (`pdf-folio-core`).
//! 3. Preflight: generate pending CRDT ops, compare remote head sequence to local cursor.
//! 4. If work remains, upload missing blobs to R2, push/pull CRDT ops via Turso, materialize LWW winners, hydrate local library rows.
//!
//! Manual CLI steps (`auth`, `plan`, `push`, `pull`, `upload-blobs`, `sync-once`, …)
//! are implemented in [`cli`] and wired from `pdf-folio-main`.
//!
//! # Related modules
//!
//! | Submodule | Responsibility |
//! | --- | --- |
//! | [`auth`] | Desktop Google OAuth (PKCE) loopback |
//! | [`session`] | Session JSON cache on disk |
//! | [`client`] | [`SyncClient`] constructor |
//! | [`remote`] | Turso/Hrana client + value types |
//! | [`blobs`] | R2 client + [`BlobCache`] |
//! | [`crdt`] | Op preparation, LWW, remote exchange, hydration methods on [`SyncClient`] |
//! | [`run`] | Preflight + conditional full pass |
//! | [`status`] | Report/checkpoint/plan types, `REGISTRY_LIBRARY_ID` |
//! | [`cli`] | `pdf-folio sync` subcommands |

/// Desktop Google OAuth (PKCE) against the control plane.
pub mod auth;
/// R2 client and local content-addressed [`BlobCache`].
pub mod blobs;
/// `pdf-folio sync` subcommands (auth, plan, push/pull, blobs, sync-once).
pub mod cli;
/// High-level [`SyncClient`] constructor holding session + Turso + R2 clients.
pub mod client;
/// CRDT op preparation, LWW materialization, remote exchange, and hydration.
pub mod crdt;
/// Turso/Hrana SQL-over-HTTP client and value types.
pub mod remote;
/// Preflight + conditional full automatic sync pass.
pub mod run;
/// Durable session JWT cache under the XDG data dir.
pub mod session;
/// Report, plan, and registry types returned by sync passes.
pub mod status;

pub use auth::{sign_in_with_google, GoogleAuthConfig};
pub use blobs::BlobCache;
pub use blobs::{R2Client, R2DownloadResponse, R2UploadResponse};
pub use client::SyncClient;
pub use remote::{TursoClient, TursoToken};
pub use session::{cached_session, save_session, Session};
pub use status::{
    SyncBlobUploadReport, SyncCrdtPreflight, SyncCrdtReport, SyncHydrationReport, SyncLibraryRow,
    SyncPlan, SyncRunReport, REGISTRY_LIBRARY_ID,
};
