//! High-level sync coordinator type.

use super::blobs::R2Client;
use super::remote::TursoClient;
use super::session::Session;

/// High-level sync coordinator.
#[derive(Debug, Clone)]
pub struct SyncClient {
    /// Session used for all control-plane calls.
    pub session: Session,
    /// Turso credential client.
    pub turso: TursoClient,
    /// R2 blob client.
    pub r2: R2Client,
}

impl SyncClient {
    /// Creates a sync coordinator from a cached session.
    pub fn new(session: Session) -> Self {
        Self {
            turso: TursoClient::new(session.clone()),
            r2: R2Client::new(session.clone()),
            session,
        }
    }
}
