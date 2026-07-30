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

/// Aggregate result after a Raindrop import run finishes (UI toast / history).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportSummary {
    /// Local library upsert outcomes from hashing/indexing downloaded PDFs.
    pub import: ImportSummary,
    /// How many remote PDF raindrops were considered (selected or discovered).
    pub remote_pdf_count: usize,
    /// How many Raindrop collections were mirrored into local folders (when preserving structure).
    pub collection_count: usize,
    /// Display name for the Raindrop account / import source row.
    pub account_label: String,
}

/// Incremental progress event pushed to the UI during Raindrop import.
///
/// Phases are non-linear: ZIP download uses [`Self::progress_basis_points`] so the
/// bar can spend more time in download than in per-file import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportProgress {
    /// Number of PDF import attempts finished in the current phase.
    pub completed: usize,
    /// Total PDFs selected for this run (denominator for simple progress).
    pub total: usize,
    /// Title or fallback label of the PDF just processed (status line).
    pub current_title: String,
    /// High-level phase driving copy and basis-point interpretation.
    pub phase: RaindropImportPhase,
    /// Optional 0–10_000 display progress for ZIP/non-linear strategies (`None` = use completed/total).
    pub progress_basis_points: Option<u16>,
    /// `true` when the PDF named by [`Self::current_title`] failed this step.
    pub failed: bool,
    /// Local entry produced when the latest PDF imported successfully.
    pub entry: Option<ImportedEntry>,
    /// Folders created while preparing collection mirrors for this run (for undo/cleanup).
    pub created_folders: Vec<FolderId>,
}

/// High-level phase for Raindrop import progress (drives status copy and basis points).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaindropImportPhase {
    /// Fetching user/collections, planning destinations, and creating local folders.
    PreparingImports,
    /// Fetching PDF bytes or the bulk ZIP export from Raindrop.
    DownloadingImportFiles,
    /// Hashing downloaded files into the local library via `pdf_folio_core` import.
    ImportingDownloadedFiles,
}

/// Where imported Raindrop PDFs land in the local library folder tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaindropImportDestination {
    /// Recreate Raindrop collection nesting as local folders under the library root.
    PreserveRaindropFolders,
    /// Same as [`Self::PreserveRaindropFolders`], rooted under an existing local folder
    /// (`None` means library root — equivalent to preserve-at-root).
    PreserveRaindropFoldersUnder(Option<FolderId>),
    /// Import every PDF with no folder membership (library root only).
    LibraryRoot,
    /// Force every selected PDF into one existing local folder.
    LocalFolder(FolderId),
}

/// UI-facing PDF candidate discovered from Raindrop (preview list / selection).
///
/// Built from private wire DTOs in [`super::client`]; safe to pass across the
/// public cloud crate boundary without Raindrop HTTP types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropPdfCandidate {
    /// Raindrop item `_id`.
    pub id: i64,
    /// User-visible title or `Raindrop {id}` fallback.
    pub title: String,
    /// Remote filename when Raindrop supplied file metadata.
    pub file_name: Option<String>,
    /// Declared remote size in bytes when known.
    pub file_size: Option<u64>,
    /// Cover/thumbnail URL for the import picker.
    pub thumbnail_url: Option<String>,
    /// Preferred download URL (file link or bookmark link; uploads may still use cache API).
    pub download_link: String,
    /// `true` when the PDF is a Raindrop-hosted upload (authenticated cache download).
    pub uploaded_file: bool,
    /// Raindrop tags to copy onto the local entry after import.
    pub tags: Vec<String>,
    /// Owning Raindrop collection id when assigned.
    pub collection_id: Option<i64>,
    /// Resolved collection title for folder mirroring labels.
    pub collection_title: Option<String>,
    /// Raindrop `lastUpdate` timestamp string for staleness display/mapping.
    pub remote_updated_at: Option<String>,
}

/// Snapshot shown in the import dialog before the user confirms a Raindrop import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropImportPreview {
    /// Stable account id string used as the local `ImportSource` identity key.
    pub account_id: String,
    /// User-visible Raindrop account name for dialog chrome.
    pub account_label: String,
    /// Remote PDFs available to multi-select for import.
    pub pdfs: Vec<RaindropPdfCandidate>,
}

/// Raindrop OAuth application credentials for browser-based sign-in.
///
/// Loaded from env/config; never logged. Used only to start the OAuth code flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaindropOAuthConfig {
    /// Raindrop developer application client id.
    pub client_id: String,
    /// Raindrop developer application client secret.
    pub client_secret: String,
}
