//! High-level sync coordinator type.
//!
//! [`SyncClient`] is the primary entry point for library and UI code that needs
//! to talk to the control plane, Turso, and R2. Methods that perform CRDT
//! exchange, blob upload, hydration, and automatic passes are implemented on
//! this type in [`super::crdt`] and [`super::run`].
//!
//! # Related
//!
//! - Session source: [`super::session`]
//! - Credentials: [`super::remote::TursoClient`], blobs: [`super::blobs::R2Client`]
//! - CLI construction: [`super::cli`]

use super::blobs::R2Client;
use super::remote::TursoClient;
use super::session::Session;

/// High-level sync coordinator for one authenticated device session.
///
/// Holds a [`Session`] and clones of the Turso credential client and R2 blob
/// client. Construct with [`SyncClient::new`] after loading or obtaining a
/// session; then call methods such as `sync_crdt_metadata`,
/// `upload_local_blobs`, `hydrate_remote_library`, or
/// `sync_library_if_needed`.
#[derive(Debug, Clone)]
pub struct SyncClient {
    /// Session used for all control-plane calls.
    pub session: Session,
    /// Turso credential client (mints remote SQL access via the control plane).
    pub turso: TursoClient,
    /// R2 blob client (presigned upload/download via the control plane).
    pub r2: R2Client,
}

impl SyncClient {
    /// Creates a sync coordinator from a session (typically from [`super::cached_session`]).
    pub fn new(session: Session) -> Self {
        Self {
            turso: TursoClient::new(session.clone()),
            r2: R2Client::new(session.clone()),
            session,
        }
    }
}
