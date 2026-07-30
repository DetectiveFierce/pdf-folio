//! Import-facing DTOs for the Raindrop product (preview, progress, destinations).
//!
//! These types are the stable surface used by the UI and public import functions
//! in [`super::import`]. Wire/API shapes for Raindrop HTTP responses live in
//! [`super::client`] (`Raindrop`, `RaindropCollection`, …) and stay crate-private.
//!
//! # Related
//!
//! - Orchestration: [`super::import`]
//! - Progress phases drive non-linear ZIP progress basis points from [`super`]

use pdf_folio_core::{FolderId, ImportSummary, ImportedEntry};

/// Summary returned after importing PDFs from Raindrop.io.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportSummary {
    /// Underlying PDF import/index summary.
    pub import: ImportSummary,
    /// Number of remote PDF raindrops discovered.
    pub remote_pdf_count: usize,
    /// Number of remote collections mirrored locally.
    pub collection_count: usize,
    /// User-visible account/source name.
    pub account_label: String,
}

/// Progress reported while importing selected PDFs from Raindrop.io.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportProgress {
    /// Number of PDF import attempts completed.
    pub completed: usize,
    /// Total number of PDFs selected for import.
    pub total: usize,
    /// Title or fallback label of the most recently processed PDF.
    pub current_title: String,
    /// Current high-level import phase.
    pub phase: RaindropImportPhase,
    /// Optional display progress for non-linear import strategies, in 1/100ths of a percent.
    pub progress_basis_points: Option<u16>,
    /// Whether the most recently processed PDF failed to import.
    pub failed: bool,
    /// Imported entry details when the most recent PDF imported successfully.
    pub entry: Option<ImportedEntry>,
    /// Local folders newly created while preparing this import.
    pub created_folders: Vec<FolderId>,
}

/// High-level phase for Raindrop import progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaindropImportPhase {
    /// Preparing metadata, destination folders, and the import plan.
    PreparingImports,
    /// Downloading PDFs or a Raindrop ZIP export.
    DownloadingImportFiles,
    /// Importing already-downloaded files into the local library.
    ImportingDownloadedFiles,
}

/// Destination chosen for a Raindrop import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaindropImportDestination {
    /// Mirror the source Raindrop folder/collection structure.
    PreserveRaindropFolders,
    /// Mirror the source Raindrop folder/collection structure under a local root folder.
    PreserveRaindropFoldersUnder(Option<FolderId>),
    /// Import PDFs without assigning them to a local folder.
    LibraryRoot,
    /// Import every selected PDF into one existing local folder.
    LocalFolder(FolderId),
}

/// A PDF available for import from Raindrop.io.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropPdfCandidate {
    /// Raindrop item id.
    pub id: i64,
    /// User-visible title or fallback label.
    pub title: String,
    /// Remote filename, when supplied.
    pub file_name: Option<String>,
    /// Remote file size, when supplied.
    pub file_size: Option<u64>,
    /// Remote thumbnail/cover URL, when supplied by Raindrop.
    pub thumbnail_url: Option<String>,
    /// Remote download link or Raindrop file link.
    pub download_link: String,
    /// Whether this PDF is an uploaded Raindrop file rather than an external PDF link.
    pub uploaded_file: bool,
    /// Raindrop tags.
    pub tags: Vec<String>,
    /// Raindrop collection id.
    pub collection_id: Option<i64>,
    /// Raindrop collection title.
    pub collection_title: Option<String>,
    /// Raindrop last update timestamp.
    pub remote_updated_at: Option<String>,
}

/// Data shown before importing PDFs from Raindrop.io.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportPreview {
    /// Raindrop account id used for import source identity.
    pub account_id: String,
    /// User-visible account/source name.
    pub account_label: String,
    /// Remote PDFs available to import.
    pub pdfs: Vec<RaindropPdfCandidate>,
}

/// OAuth app credentials used to start browser sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropOAuthConfig {
    /// Raindrop application client id.
    pub client_id: String,
    /// Raindrop application client secret.
    pub client_secret: String,
}
