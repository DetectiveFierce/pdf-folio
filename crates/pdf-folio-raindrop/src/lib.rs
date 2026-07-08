//! Raindrop.io import support for PDF-Folio.
//!
//! This crate keeps Raindrop-specific HTTP, OAuth, download, and metadata
//! mirroring logic out of the UI and database crates.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use blake3::Hasher;
use directories::ProjectDirs;
use pdf_folio_core::PdfDoc;
use pdf_folio_db::{
    Db, EntryId, FolderId, ImportSummary, ImportedEntry, IndexDocument, NewLibraryEntry,
    RaindropEntryMapping, SearchIndex,
};
use reqwest::header::AUTHORIZATION;
use serde::Deserialize;
use url::Url;

const API_BASE: &str = "https://api.raindrop.io/rest/v1";
const MAX_PER_PAGE: u16 = 50;
const ZIP_IMPORT_THRESHOLD: usize = 12;
const ZIP_PREPARING_PROGRESS_BASIS_POINTS: u16 = 1_250;
const ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS: u16 = 3_750;
const ZIP_EXTRACTED_PROGRESS_BASIS_POINTS: u16 = 5_000;
const ZIP_IMPORTING_PROGRESS_BASIS_POINTS: u16 = 5_000;
const PROGRESS_BASIS_POINTS_MAX: u16 = 10_000;
const IMPORT_PROGRESS_UNITS_PER_PDF: u32 = 1_000;

mod auth;
mod types;
pub use auth::OAUTH_CALLBACK_URL;
use auth::{bundled_or_env_oauth_config, cached_access_token, resolve_access_token};
pub use types::*;

impl RaindropPdfCandidate {
    fn to_raindrop(&self) -> Raindrop {
        Raindrop {
            id: self.id,
            title: Some(self.title.clone()),
            link: self.download_link.clone(),
            cover: self.thumbnail_url.clone(),
            media: Vec::new(),
            item_type: Some(String::from("document")),
            collection: self.collection_id.map(|id| RaindropRef { id }),
            tags: self.tags.clone(),
            file: Some(RaindropFile {
                name: self.file_name.clone(),
                link: Some(self.download_link.clone()),
                size: self.file_size,
                mime_type: Some(String::from("application/pdf")),
            }),
            last_update: self.remote_updated_at.clone(),
            uploaded_file: self.uploaded_file,
        }
    }
}

/// Returns true when the importer can run without asking the user for OAuth app credentials.
pub fn can_import_without_prompt() -> bool {
    std::env::var("PDF_FOLIO_RAINDROP_TOKEN")
        .ok()
        .is_some_and(|token| !token.trim().is_empty())
        || cached_access_token().is_ok_and(|token| !token.trim().is_empty())
        || bundled_or_env_oauth_config().is_some()
}

/// Imports all PDFs from the authenticated Raindrop.io account.
///
/// Authentication is resolved in this order:
///
/// 1. `PDF_FOLIO_RAINDROP_TOKEN` bearer token, useful with Raindrop test tokens.
/// 2. OAuth browser sign-in using `PDF_FOLIO_RAINDROP_CLIENT_ID` and
///    `PDF_FOLIO_RAINDROP_CLIENT_SECRET`.
///
/// # Errors
///
/// Returns an error when authentication, API access, PDF download, or database writes fail.
pub async fn import_all_pdfs(db: &Db) -> Result<RaindropImportSummary> {
    import_all_pdfs_with_auth(db, None).await
}

/// Loads the authenticated user's remote PDFs without importing them.
///
/// # Errors
///
/// Returns an error when authentication or API access fails.
pub async fn import_preview() -> Result<RaindropImportPreview> {
    import_preview_with_auth(None).await
}

/// Loads the authenticated user's remote PDFs using an optional OAuth config.
///
/// # Errors
///
/// Returns an error when authentication or API access fails.
pub async fn import_preview_with_auth(
    oauth_config: Option<RaindropOAuthConfig>,
) -> Result<RaindropImportPreview> {
    let token = resolve_access_token(oauth_config).await?;
    let client = RaindropClient::new(token)?;
    let user = client.user().await?;
    let account_id = user.id.to_string();
    let account_label = account_label(&user);
    let collections = client.collections().await?;
    let collection_titles = collections
        .iter()
        .map(|collection| (collection.id, collection.title()))
        .collect::<HashMap<_, _>>();
    let mut pdfs = client
        .pdf_raindrops()
        .await?
        .into_iter()
        .map(|raindrop| raindrop.to_candidate(&collection_titles))
        .collect::<Vec<_>>();
    pdfs.sort_by_key(|pdf| pdf.title.to_lowercase());
    Ok(RaindropImportPreview {
        account_id,
        account_label,
        pdfs,
    })
}

/// Imports selected Raindrop PDF ids into the requested destination.
///
/// # Errors
///
/// Returns an error when authentication, API access, PDF download, or database writes fail.
pub async fn import_selected_pdfs(
    db: &Db,
    selected_ids: HashSet<i64>,
    destination: RaindropImportDestination,
) -> Result<RaindropImportSummary> {
    import_selected_pdfs_with_auth(db, selected_ids, destination, None).await
}

/// Imports selected Raindrop PDF ids and reports progress after each PDF attempt.
///
/// # Errors
///
/// Returns an error when authentication, API access, or setup database writes fail.
pub async fn import_selected_pdfs_with_progress(
    db: &Db,
    selected_ids: HashSet<i64>,
    destination: RaindropImportDestination,
    mut progress: impl FnMut(RaindropImportProgress) + Send,
) -> Result<RaindropImportSummary> {
    import_pdfs_with_auth(
        db,
        None,
        Some(selected_ids),
        destination,
        Some(&mut progress),
    )
    .await
}

/// Imports already-previewed Raindrop PDF candidates and reports progress.
///
/// This avoids re-fetching and hydrating selected Raindrops during import.
///
/// # Errors
///
/// Returns an error when authentication, API access, PDF download, or database writes fail.
pub async fn import_preview_pdfs_with_progress(
    db: &Db,
    preview: RaindropImportPreview,
    destination: RaindropImportDestination,
    mut progress: impl FnMut(RaindropImportProgress) + Send,
) -> Result<RaindropImportSummary> {
    import_pdfs_with_preview(db, preview, destination, Some(&mut progress)).await
}

/// Imports selected Raindrop PDF ids with an optional OAuth config.
///
/// # Errors
///
/// Returns an error when authentication, API access, PDF download, or database writes fail.
pub async fn import_selected_pdfs_with_auth(
    db: &Db,
    selected_ids: HashSet<i64>,
    destination: RaindropImportDestination,
    oauth_config: Option<RaindropOAuthConfig>,
) -> Result<RaindropImportSummary> {
    import_pdfs_with_auth(db, oauth_config, Some(selected_ids), destination, None).await
}

/// Imports all PDFs from Raindrop.io using saved credentials, env vars, or the supplied OAuth app.
///
/// # Errors
///
/// Returns an error when authentication, API access, PDF download, or database writes fail.
pub async fn import_all_pdfs_with_auth(
    db: &Db,
    oauth_config: Option<RaindropOAuthConfig>,
) -> Result<RaindropImportSummary> {
    import_pdfs_with_auth(
        db,
        oauth_config,
        None,
        RaindropImportDestination::PreserveRaindropFolders,
        None,
    )
    .await
}

async fn import_pdfs_with_preview(
    db: &Db,
    preview: RaindropImportPreview,
    destination: RaindropImportDestination,
    progress: Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> Result<RaindropImportSummary> {
    let token = resolve_access_token(None).await?;
    let client = RaindropClient::new(token)?;
    let source_id = format!("raindrop:{}", preview.account_id);
    db.upsert_import_source(
        &source_id,
        "raindrop",
        Some(&preview.account_id),
        Some(&preview.account_label),
    )?;

    let collections = if destination_preserves_raindrop_folders(&destination) {
        client.collections().await?
    } else {
        Vec::new()
    };
    let raindrops = preview
        .pdfs
        .iter()
        .map(RaindropPdfCandidate::to_raindrop)
        .collect::<Vec<_>>();

    let import = import_prepared_raindrops(
        db,
        &client,
        &source_id,
        &preview.account_label,
        collections,
        raindrops,
        destination,
        progress,
    )
    .await?;
    Ok(import)
}

async fn import_pdfs_with_auth(
    db: &Db,
    oauth_config: Option<RaindropOAuthConfig>,
    selected_ids: Option<HashSet<i64>>,
    destination: RaindropImportDestination,
    progress: Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> Result<RaindropImportSummary> {
    let token = resolve_access_token(oauth_config).await?;
    let client = RaindropClient::new(token)?;
    let user = client.user().await?;
    let account_id = user.id.to_string();
    let account_label = account_label(&user);
    let source_id = format!("raindrop:{account_id}");
    db.upsert_import_source(
        &source_id,
        "raindrop",
        Some(&account_id),
        Some(&account_label),
    )?;

    let collections = client.collections().await?;
    let mut raindrops = client.pdf_raindrops().await?;
    if let Some(selected_ids) = selected_ids {
        raindrops.retain(|raindrop| selected_ids.contains(&raindrop.id));
        let mut hydrated = Vec::with_capacity(raindrops.len());
        for raindrop in raindrops {
            match client.raindrop(raindrop.id).await {
                Ok(full_raindrop) => hydrated.push(full_raindrop),
                Err(_) => hydrated.push(raindrop),
            }
        }
        raindrops = hydrated;
    }

    import_prepared_raindrops(
        db,
        &client,
        &source_id,
        &account_label,
        collections,
        raindrops,
        destination,
        progress,
    )
    .await
}

async fn import_prepared_raindrops(
    db: &Db,
    client: &RaindropClient,
    source_id: &str,
    account_label: &str,
    collections: Vec<RaindropCollection>,
    raindrops: Vec<Raindrop>,
    destination: RaindropImportDestination,
    mut progress: Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> Result<RaindropImportSummary> {
    let created_folders = match &destination {
        RaindropImportDestination::PreserveRaindropFolders => {
            mirror_collections(db, &source_id, &collections, None)?
        }
        RaindropImportDestination::PreserveRaindropFoldersUnder(root_folder_id) => {
            mirror_collections(db, &source_id, &collections, root_folder_id.as_ref())?
        }
        RaindropImportDestination::LibraryRoot | RaindropImportDestination::LocalFolder(_) => {
            Vec::new()
        }
    };
    let total = raindrops.len();
    let import_strategy = choose_import_strategy(&raindrops);
    if let Some(progress) = progress.as_deref_mut() {
        progress(RaindropImportProgress {
            completed: 0,
            total,
            current_title: String::from("Preparing Imports"),
            phase: RaindropImportPhase::PreparingImports,
            progress_basis_points: match import_strategy {
                RaindropImportStrategy::ZipExport => Some(ZIP_PREPARING_PROGRESS_BASIS_POINTS),
                RaindropImportStrategy::IndividualFiles => None,
            },
            failed: false,
            entry: None,
            created_folders: created_folders.clone(),
        });
        if !created_folders.is_empty() {
            progress(RaindropImportProgress {
                completed: 0,
                total,
                current_title: String::from("Preparing folders..."),
                phase: RaindropImportPhase::PreparingImports,
                progress_basis_points: match import_strategy {
                    RaindropImportStrategy::ZipExport => Some(ZIP_PREPARING_PROGRESS_BASIS_POINTS),
                    RaindropImportStrategy::IndividualFiles => None,
                },
                failed: false,
                entry: None,
                created_folders: created_folders.clone(),
            });
        }
    }
    let storage_dir = raindrop_storage_dir(&source_id)?;
    let import = match import_strategy {
        RaindropImportStrategy::ZipExport => {
            let (uploaded_raindrops, linked_raindrops): (Vec<_>, Vec<_>) = raindrops
                .iter()
                .cloned()
                .partition(Raindrop::has_uploaded_file);
            let mut import = match import_raindrop_pdfs_from_zip(
                db,
                &client,
                &source_id,
                &storage_dir,
                &uploaded_raindrops,
                &destination,
                &mut progress,
            )
            .await
            {
                Ok(import) => import,
                Err(error) => {
                    if let Some(progress) = progress.as_deref_mut() {
                        progress(RaindropImportProgress {
                            completed: 0,
                            total,
                            current_title: format!("ZIP export unavailable: {error}"),
                            phase: RaindropImportPhase::DownloadingImportFiles,
                            progress_basis_points: Some(ZIP_PREPARING_PROGRESS_BASIS_POINTS),
                            failed: true,
                            entry: None,
                            created_folders: Vec::new(),
                        });
                    }
                    ImportSummary {
                        entries: Vec::new(),
                        errors: uploaded_raindrops
                            .iter()
                            .map(|raindrop| format!("{}: {error}", raindrop.display_label()))
                            .collect(),
                    }
                }
            };
            if !linked_raindrops.is_empty() {
                let linked_import = import_raindrop_pdfs_individually(
                    db,
                    &client,
                    &source_id,
                    &storage_dir,
                    &linked_raindrops,
                    &destination,
                    &mut progress,
                )
                .await;
                merge_import_summary(&mut import, linked_import);
            }
            import
        }
        RaindropImportStrategy::IndividualFiles => {
            import_raindrop_pdfs_individually(
                db,
                &client,
                &source_id,
                &storage_dir,
                &raindrops,
                &destination,
                &mut progress,
            )
            .await
        }
    };

    Ok(RaindropImportSummary {
        import,
        remote_pdf_count: raindrops.len(),
        collection_count: collections.len(),
        account_label: account_label.to_owned(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RaindropImportStrategy {
    IndividualFiles,
    ZipExport,
}

fn choose_import_strategy(raindrops: &[Raindrop]) -> RaindropImportStrategy {
    if raindrops
        .iter()
        .filter(|raindrop| raindrop.has_uploaded_file())
        .count()
        >= ZIP_IMPORT_THRESHOLD
    {
        RaindropImportStrategy::ZipExport
    } else {
        RaindropImportStrategy::IndividualFiles
    }
}

fn merge_import_summary(target: &mut ImportSummary, source: ImportSummary) {
    let mut seen_entry_ids = target
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<HashSet<_>>();
    for entry in source.entries {
        if seen_entry_ids.insert(entry.id.clone()) {
            target.entries.push(entry);
        }
    }
    target.errors.extend(source.errors);
}

fn destination_preserves_raindrop_folders(destination: &RaindropImportDestination) -> bool {
    matches!(
        destination,
        RaindropImportDestination::PreserveRaindropFolders
            | RaindropImportDestination::PreserveRaindropFoldersUnder(_)
    )
}

#[cfg(test)]
fn zip_import_progress_basis_points(completed: usize, total: usize) -> u16 {
    if total == 0 {
        return PROGRESS_BASIS_POINTS_MAX;
    }

    zip_import_progress_basis_points_for_units(
        (completed.min(total) as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
        (total as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
    )
}

fn zip_import_progress_basis_points_for_units(completed_units: u32, total_units: u32) -> u16 {
    if total_units == 0 {
        return PROGRESS_BASIS_POINTS_MAX;
    }

    let base = u32::from(ZIP_EXTRACTED_PROGRESS_BASIS_POINTS);
    let importing = u32::from(ZIP_IMPORTING_PROGRESS_BASIS_POINTS);
    (base + importing * completed_units.min(total_units) / total_units)
        .min(u32::from(PROGRESS_BASIS_POINTS_MAX)) as u16
}

fn zip_extract_progress_basis_points(processed_entries: usize, total_entries: usize) -> u16 {
    if total_entries == 0 {
        return ZIP_EXTRACTED_PROGRESS_BASIS_POINTS;
    }

    let processed_entries = processed_entries.min(total_entries) as u32;
    let total_entries = total_entries as u32;
    let base = u32::from(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS);
    let extracting =
        u32::from(ZIP_EXTRACTED_PROGRESS_BASIS_POINTS - ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS);
    (base + extracting * processed_entries / total_entries)
        .min(u32::from(ZIP_EXTRACTED_PROGRESS_BASIS_POINTS)) as u16
}

fn zip_download_progress_basis_points(downloaded: u64, total: u64) -> u16 {
    if total == 0 {
        return ZIP_PREPARING_PROGRESS_BASIS_POINTS;
    }

    let base = u64::from(ZIP_PREPARING_PROGRESS_BASIS_POINTS);
    let downloading =
        u64::from(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS - ZIP_PREPARING_PROGRESS_BASIS_POINTS);
    (base + downloading * downloaded.min(total) / total)
        .min(u64::from(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS)) as u16
}

fn report_raindrop_progress(
    progress: &mut Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
    event: RaindropImportProgress,
) {
    if let Some(progress) = progress.as_deref_mut() {
        progress(event);
    }
}

struct ImportedRaindropPdf {
    entry: ImportedEntry,
    index_documents: Vec<IndexDocument>,
}

async fn import_raindrop_pdfs_individually(
    db: &Db,
    client: &RaindropClient,
    source_id: &str,
    storage_dir: &Path,
    raindrops: &[Raindrop],
    destination: &RaindropImportDestination,
    progress: &mut Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> ImportSummary {
    let total = raindrops.len();
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut seen_entry_ids = HashSet::new();
    let search_index = match SearchIndex::open_default() {
        Ok(search_index) => search_index,
        Err(error) => {
            return ImportSummary {
                entries,
                errors: raindrops
                    .iter()
                    .map(|raindrop| format!("{}: {error}", raindrop.display_label()))
                    .collect(),
            };
        }
    };

    for (index, raindrop) in raindrops.iter().enumerate() {
        if let Some(progress) = progress.as_deref_mut() {
            progress(RaindropImportProgress {
                completed: index,
                total,
                current_title: format!("Downloading Import Files: {}", raindrop.display_label()),
                phase: RaindropImportPhase::DownloadingImportFiles,
                progress_basis_points: None,
                failed: false,
                entry: None,
                created_folders: Vec::new(),
            });
        }

        let mut failed = false;
        let mut current_entry = None;
        match import_raindrop_pdf(
            db,
            client,
            source_id,
            storage_dir,
            raindrop,
            destination,
            |_| {},
        )
        .await
        {
            Ok(imported) => {
                if let Err(error) = search_index.replace_entry_pages(imported.index_documents) {
                    failed = true;
                    errors.push(format!(
                        "{}: search index: {error}",
                        raindrop.display_label()
                    ));
                } else {
                    current_entry = Some(imported.entry.clone());
                    if seen_entry_ids.insert(imported.entry.id.clone()) {
                        entries.push(imported.entry);
                    }
                }
            }
            Err(error) => {
                failed = true;
                errors.push(format!("{}: {error}", raindrop.display_label()));
            }
        }
        if let Some(progress) = progress.as_deref_mut() {
            progress(RaindropImportProgress {
                completed: index + 1,
                total,
                current_title: raindrop.display_label(),
                phase: RaindropImportPhase::ImportingDownloadedFiles,
                progress_basis_points: None,
                failed,
                entry: current_entry,
                created_folders: Vec::new(),
            });
        }
    }

    ImportSummary { entries, errors }
}

async fn import_raindrop_pdfs_from_zip(
    db: &Db,
    client: &RaindropClient,
    source_id: &str,
    storage_dir: &Path,
    raindrops: &[Raindrop],
    destination: &RaindropImportDestination,
    progress: &mut Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> Result<ImportSummary> {
    let total = raindrops.len();
    report_raindrop_progress(
        progress,
        RaindropImportProgress {
            completed: 0,
            total,
            current_title: String::from("Preparing ZIP export"),
            phase: RaindropImportPhase::PreparingImports,
            progress_basis_points: Some(ZIP_PREPARING_PROGRESS_BASIS_POINTS),
            failed: false,
            entry: None,
            created_folders: Vec::new(),
        },
    );

    let archive = client
        .download_pdf_export_zip(|basis_points| {
            report_raindrop_progress(
                progress,
                RaindropImportProgress {
                    completed: 0,
                    total,
                    current_title: String::from("Downloading Import Files"),
                    phase: RaindropImportPhase::DownloadingImportFiles,
                    progress_basis_points: Some(basis_points),
                    failed: false,
                    entry: None,
                    created_folders: Vec::new(),
                },
            );
        })
        .await?;
    report_raindrop_progress(
        progress,
        RaindropImportProgress {
            completed: 0,
            total,
            current_title: String::from("Downloading Import Files"),
            phase: RaindropImportPhase::DownloadingImportFiles,
            progress_basis_points: Some(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS),
            failed: false,
            entry: None,
            created_folders: Vec::new(),
        },
    );
    let extracted = extract_selected_pdfs_from_zip(storage_dir, &archive, raindrops, progress)?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut seen_entry_ids = HashSet::new();
    let search_index = SearchIndex::open_default()?;
    let batch_commit_units = IMPORT_PROGRESS_UNITS_PER_PDF;
    let total_import_units = (total as u32) * IMPORT_PROGRESS_UNITS_PER_PDF + batch_commit_units;
    let db_path = db.path().to_path_buf();
    let mut batch_index_documents = Vec::new();

    for (index, raindrop) in raindrops.iter().enumerate() {
        report_raindrop_progress(
            progress,
            RaindropImportProgress {
                completed: index,
                total,
                current_title: raindrop.display_label(),
                phase: RaindropImportPhase::ImportingDownloadedFiles,
                progress_basis_points: Some(zip_import_progress_basis_points_for_units(
                    (index as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
                    total_import_units,
                )),
                failed: false,
                entry: None,
                created_folders: Vec::new(),
            },
        );

        let mut failed = false;
        let mut current_entry = None;
        match extracted.get(&raindrop.id) {
            Some(path) => {
                let (page_progress_sender, mut page_progress_receiver) =
                    tokio::sync::mpsc::unbounded_channel::<u32>();
                let import_db_path = db_path.clone();
                let import_source_id = source_id.to_owned();
                let import_path = path.clone();
                let import_raindrop = raindrop.clone();
                let import_destination = destination.clone();
                let mut import_task = tokio::task::spawn_blocking(move || {
                    let import_db = Db::open(import_db_path)?;
                    import_downloaded_raindrop_pdf(
                        &import_db,
                        &import_source_id,
                        &import_path,
                        &import_raindrop,
                        &import_destination,
                        |completed_units| {
                            let _ = page_progress_sender.send(completed_units);
                        },
                    )
                });
                let import_result = loop {
                    tokio::select! {
                        Some(completed_units) = page_progress_receiver.recv() => {
                            report_raindrop_progress(
                                progress,
                                RaindropImportProgress {
                                    completed: index,
                                    total,
                                    current_title: raindrop.display_label(),
                                    phase: RaindropImportPhase::ImportingDownloadedFiles,
                                    progress_basis_points: Some(
                                        zip_import_progress_basis_points_for_units(
                                            (index as u32) * IMPORT_PROGRESS_UNITS_PER_PDF
                                                + completed_units,
                                            total_import_units,
                                        ),
                                    ),
                                    failed: false,
                                    entry: None,
                                    created_folders: Vec::new(),
                                },
                            );
                        }
                        result = &mut import_task => {
                            break result
                                .map_err(anyhow::Error::from)
                                .and_then(|result| result);
                        }
                    }
                };
                while let Ok(completed_units) = page_progress_receiver.try_recv() {
                    report_raindrop_progress(
                        progress,
                        RaindropImportProgress {
                            completed: index,
                            total,
                            current_title: raindrop.display_label(),
                            phase: RaindropImportPhase::ImportingDownloadedFiles,
                            progress_basis_points: Some(
                                zip_import_progress_basis_points_for_units(
                                    (index as u32) * IMPORT_PROGRESS_UNITS_PER_PDF
                                        + completed_units,
                                    total_import_units,
                                ),
                            ),
                            failed: false,
                            entry: None,
                            created_folders: Vec::new(),
                        },
                    );
                }
                match import_result {
                    Ok(imported) => {
                        batch_index_documents.extend(imported.index_documents);
                        current_entry = Some(imported.entry.clone());
                        if seen_entry_ids.insert(imported.entry.id.clone()) {
                            entries.push(imported.entry);
                        }
                    }
                    Err(error) => {
                        failed = true;
                        errors.push(format!("{}: {error}", raindrop.display_label()));
                    }
                }
            }
            None => {
                failed = true;
                errors.push(format!(
                    "{}: PDF was not found in the Raindrop ZIP export and was not downloaded individually",
                    raindrop.display_label()
                ));
            }
        }

        if let Some(progress) = progress.as_deref_mut() {
            progress(RaindropImportProgress {
                completed: index + 1,
                total,
                current_title: raindrop.display_label(),
                phase: RaindropImportPhase::ImportingDownloadedFiles,
                progress_basis_points: Some(zip_import_progress_basis_points_for_units(
                    ((index + 1) as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
                    total_import_units,
                )),
                failed,
                entry: current_entry,
                created_folders: Vec::new(),
            });
        }
    }

    if let Err(error) = search_index.replace_entries_pages(batch_index_documents) {
        errors.push(format!("search index: {error}"));
    }
    report_raindrop_progress(
        progress,
        RaindropImportProgress {
            completed: total,
            total,
            current_title: String::from("Indexed downloaded files"),
            phase: RaindropImportPhase::ImportingDownloadedFiles,
            progress_basis_points: Some(PROGRESS_BASIS_POINTS_MAX),
            failed: false,
            entry: None,
            created_folders: Vec::new(),
        },
    );

    Ok(ImportSummary { entries, errors })
}

fn account_label(user: &RaindropUser) -> String {
    user.full_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("Raindrop user {}", user.id))
}

fn mirror_collections(
    db: &Db,
    source_id: &str,
    collections: &[RaindropCollection],
    root_folder_id: Option<&FolderId>,
) -> Result<Vec<FolderId>> {
    let mut created_folders = Vec::new();
    let mut remaining = collections.to_vec();
    let mut mapped = HashSet::new();

    while !remaining.is_empty() {
        let before = remaining.len();
        let mut deferred = Vec::new();

        for collection in remaining {
            let parent_id = collection.parent_id();
            if parent_id.is_some_and(|id| !mapped.contains(&id)) {
                deferred.push(collection);
                continue;
            }

            let (folder_id, created) = db.upsert_raindrop_collection_mapping(
                source_id,
                collection.id,
                parent_id,
                &collection.title(),
                root_folder_id,
            )?;
            if created {
                created_folders.push(folder_id);
            }
            mapped.insert(collection.id);
        }

        if deferred.len() == before {
            for collection in deferred {
                let parent_collection_id = if root_folder_id.is_some() {
                    None
                } else {
                    collection.parent_id()
                };
                let (folder_id, created) = db.upsert_raindrop_collection_mapping(
                    source_id,
                    collection.id,
                    parent_collection_id,
                    &collection.title(),
                    root_folder_id,
                )?;
                if created {
                    created_folders.push(folder_id);
                }
                mapped.insert(collection.id);
            }
            break;
        }

        remaining = deferred;
    }

    Ok(created_folders)
}

async fn import_raindrop_pdf(
    db: &Db,
    client: &RaindropClient,
    source_id: &str,
    storage_dir: &Path,
    raindrop: &Raindrop,
    destination: &RaindropImportDestination,
    page_progress: impl FnMut(u32),
) -> Result<ImportedRaindropPdf> {
    fs::create_dir_all(storage_dir)
        .with_context(|| format!("Could not create {}.", storage_dir.display()))?;
    let path = storage_dir.join(format!(
        "{}-{}",
        raindrop.id,
        safe_pdf_file_name(
            &raindrop
                .file_name()
                .unwrap_or_else(|| format!("{}.pdf", raindrop.id))
        )
    ));

    let remote_link = raindrop.download_link().to_owned();
    let pdf = client
        .download_pdf_for_raindrop(raindrop)
        .await
        .with_context(|| format!("Could not download {remote_link}"))?;
    tokio::fs::write(&path, pdf)
        .await
        .with_context(|| format!("Could not save {}.", path.display()))?;

    import_downloaded_raindrop_pdf(db, source_id, &path, raindrop, destination, page_progress)
}

fn import_downloaded_raindrop_pdf(
    db: &Db,
    source_id: &str,
    path: &Path,
    raindrop: &Raindrop,
    destination: &RaindropImportDestination,
    page_progress: impl FnMut(u32),
) -> Result<ImportedRaindropPdf> {
    let remote_link = raindrop.download_link().to_owned();
    let imported = import_pdf_with_metadata(db, path, raindrop.title.as_deref(), page_progress)?;

    for tag in &raindrop.tags {
        let tag = tag.trim();
        if !tag.is_empty() {
            db.add_tag(&imported.entry.id, tag)?;
        }
    }

    match destination {
        RaindropImportDestination::PreserveRaindropFolders
        | RaindropImportDestination::PreserveRaindropFoldersUnder(_) => {
            if let Some(collection_id) = raindrop.collection_id() {
                if let Some(folder_id) = db.raindrop_collection_folder(source_id, collection_id)? {
                    add_entry_to_raindrop_folder(db, &imported.entry.id, &folder_id)?;
                }
            }
        }
        RaindropImportDestination::LibraryRoot => {}
        RaindropImportDestination::LocalFolder(folder_id) => {
            db.add_entry_to_folder(&imported.entry.id, folder_id)?;
        }
    }

    db.upsert_raindrop_entry_mapping(&RaindropEntryMapping {
        source_id: source_id.to_owned(),
        raindrop_id: raindrop.id,
        entry_id: imported.entry.id.clone(),
        collection_id: raindrop.collection_id(),
        remote_link: Some(remote_link),
        remote_title: raindrop.title.clone(),
        remote_updated_at: raindrop.last_update.clone(),
        file_name: raindrop.file_name(),
        file_size: raindrop.file_size(),
    })?;

    Ok(imported)
}

fn extract_selected_pdfs_from_zip(
    storage_dir: &Path,
    archive: &[u8],
    raindrops: &[Raindrop],
    progress: &mut Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
) -> Result<HashMap<i64, PathBuf>> {
    fs::create_dir_all(storage_dir)
        .with_context(|| format!("Could not create {}.", storage_dir.display()))?;
    let mut zip =
        zip::ZipArchive::new(Cursor::new(archive)).context("Could not read export ZIP.")?;
    let match_index = ZipMatchIndex::new(raindrops);
    let mut remaining = (0..raindrops.len()).collect::<HashSet<_>>();
    let mut extracted = HashMap::new();
    let zip_entries = zip.len();

    for index in 0..zip.len() {
        let mut file = zip
            .by_index(index)
            .with_context(|| format!("Could not read ZIP entry {index}."))?;
        report_raindrop_progress(
            progress,
            RaindropImportProgress {
                completed: extracted.len(),
                total: raindrops.len(),
                current_title: String::from("Extracting ZIP export"),
                phase: RaindropImportPhase::DownloadingImportFiles,
                progress_basis_points: Some(zip_extract_progress_basis_points(index, zip_entries)),
                failed: false,
                entry: None,
                created_folders: Vec::new(),
            },
        );
        if file.is_dir() || !file.name().to_lowercase().ends_with(".pdf") {
            continue;
        }

        let entry_name = file
            .enclosed_name()
            .and_then(|path| path.file_name().map(|name| name.to_owned()))
            .and_then(|name| name.to_str().map(str::to_owned))
            .unwrap_or_else(|| {
                file.name()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            });
        let Some(raindrop_index) = match_index.match_entry(&remaining, &entry_name, file.size())
        else {
            continue;
        };
        remaining.remove(&raindrop_index);
        let raindrop = &raindrops[raindrop_index];
        let path = storage_dir.join(format!(
            "{}-{}",
            raindrop.id,
            safe_pdf_file_name(
                &raindrop
                    .file_name()
                    .unwrap_or_else(|| format!("{}.pdf", raindrop.id))
            )
        ));

        let mut bytes = Vec::with_capacity(file.size().try_into().unwrap_or_default());
        file.read_to_end(&mut bytes)
            .with_context(|| format!("Could not extract {}.", file.name()))?;
        ensure_pdf_response(file.name(), &bytes)?;
        fs::write(&path, bytes).with_context(|| format!("Could not save {}.", path.display()))?;
        extracted.insert(raindrop.id, path);
        report_raindrop_progress(
            progress,
            RaindropImportProgress {
                completed: extracted.len(),
                total: raindrops.len(),
                current_title: raindrop.display_label(),
                phase: RaindropImportPhase::DownloadingImportFiles,
                progress_basis_points: Some(zip_extract_progress_basis_points(
                    index + 1,
                    zip_entries,
                )),
                failed: false,
                entry: None,
                created_folders: Vec::new(),
            },
        );
    }

    report_raindrop_progress(
        progress,
        RaindropImportProgress {
            completed: extracted.len(),
            total: raindrops.len(),
            current_title: String::from("Importing Downloaded Files"),
            phase: RaindropImportPhase::ImportingDownloadedFiles,
            progress_basis_points: Some(ZIP_EXTRACTED_PROGRESS_BASIS_POINTS),
            failed: false,
            entry: None,
            created_folders: Vec::new(),
        },
    );

    Ok(extracted)
}

struct ZipMatchIndex {
    names: HashMap<String, Vec<usize>>,
    stems: HashMap<String, Vec<usize>>,
    sizes: HashMap<u64, Vec<usize>>,
    name_by_index: Vec<Option<String>>,
    stem_by_index: Vec<Option<String>>,
    size_by_index: Vec<Option<u64>>,
}

impl ZipMatchIndex {
    fn new(raindrops: &[Raindrop]) -> Self {
        let mut names: HashMap<String, Vec<usize>> = HashMap::new();
        let mut stems: HashMap<String, Vec<usize>> = HashMap::new();
        let mut sizes: HashMap<u64, Vec<usize>> = HashMap::new();
        let mut name_by_index = Vec::with_capacity(raindrops.len());
        let mut stem_by_index = Vec::with_capacity(raindrops.len());
        let mut size_by_index = Vec::with_capacity(raindrops.len());

        for (index, raindrop) in raindrops.iter().enumerate() {
            if let Some(file_name) = raindrop.file_name() {
                let name = normalized_zip_file_name(&file_name);
                let stem = normalized_zip_file_stem(&name);
                names.entry(name).or_default().push(index);
                stems.entry(stem).or_default().push(index);
                name_by_index.push(Some(normalized_zip_file_name(&file_name)));
                stem_by_index.push(Some(normalized_zip_file_stem(&file_name)));
            } else {
                name_by_index.push(None);
                stem_by_index.push(None);
            }
            if let Some(size) = raindrop.file_size().filter(|size| *size > 0) {
                sizes.entry(size).or_default().push(index);
                size_by_index.push(Some(size));
            } else {
                size_by_index.push(None);
            }
        }

        Self {
            names,
            stems,
            sizes,
            name_by_index,
            stem_by_index,
            size_by_index,
        }
    }

    fn match_entry(
        &self,
        remaining: &HashSet<usize>,
        entry_name: &str,
        entry_size: u64,
    ) -> Option<usize> {
        let entry_name = normalized_zip_file_name(entry_name);
        let entry_stem = normalized_zip_file_stem(&entry_name);

        if entry_size > 0 {
            let exact_name_and_size = self.unique_remaining_by_predicate(remaining, |index| {
                self.name_by_index[index].as_deref() == Some(entry_name.as_str())
                    && self.size_by_index[index] == Some(entry_size)
            });
            if exact_name_and_size.is_some() {
                return exact_name_and_size;
            }
        }

        let name_matches = unique_remaining_match(self.names.get(&entry_name), remaining);
        if name_matches.is_some() {
            return name_matches;
        }

        if entry_size > 0 {
            let size_matches = unique_remaining_match(self.sizes.get(&entry_size), remaining);
            if size_matches.is_some() {
                return size_matches;
            }
        }

        let stem_matches = unique_remaining_match(self.stems.get(&entry_stem), remaining);
        if stem_matches.is_some() {
            return stem_matches;
        }

        self.unique_remaining_by_predicate(remaining, |index| {
            self.name_by_index[index]
                .as_deref()
                .is_some_and(|name| entry_name.ends_with(name))
        })
        .or_else(|| {
            self.unique_remaining_by_predicate(remaining, |index| {
                self.stem_by_index[index]
                    .as_deref()
                    .is_some_and(|stem| entry_stem.ends_with(stem))
            })
        })
    }

    fn unique_remaining_by_predicate(
        &self,
        remaining: &HashSet<usize>,
        predicate: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        let mut matched = None;
        for index in remaining {
            if !predicate(*index) {
                continue;
            }
            if matched.is_some() {
                return None;
            }
            matched = Some(*index);
        }
        matched
    }
}

fn unique_remaining_match(
    indexes: Option<&Vec<usize>>,
    remaining: &HashSet<usize>,
) -> Option<usize> {
    let mut matched = None;
    for index in indexes? {
        if !remaining.contains(index) {
            continue;
        }
        if matched.is_some() {
            return None;
        }
        matched = Some(*index);
    }
    matched
}

fn normalized_zip_file_name(name: &str) -> String {
    safe_pdf_file_name(name).to_lowercase()
}

fn normalized_zip_file_stem(name: &str) -> String {
    normalized_zip_file_name(name)
        .trim_end_matches(".pdf")
        .to_owned()
}

fn add_entry_to_raindrop_folder(db: &Db, entry_id: &EntryId, folder_id: &FolderId) -> Result<()> {
    db.add_entry_to_folder(entry_id, folder_id)
}

fn import_pdf_with_metadata(
    db: &Db,
    path: &Path,
    remote_title: Option<&str>,
    mut page_progress: impl FnMut(u32),
) -> Result<ImportedRaindropPdf> {
    let id = EntryId::new(hash_file(path)?);
    page_progress(IMPORT_PROGRESS_UNITS_PER_PDF / 10);
    let inserted = db.entry_by_path(path)?.is_none();
    let doc = PdfDoc::open(path)?;
    let page_count = doc.page_count();
    page_progress(IMPORT_PROGRESS_UNITS_PER_PDF / 5);
    let title = clean_import_title(remote_title)
        .or_else(|| attributed_title(&doc))
        .or_else(|| title_from_path(path));
    let author = attributed_author(&doc);

    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.to_path_buf(),
        title: title.clone(),
        author: author.clone(),
        author_attributed: true,
        page_count_attributed: true,
        page_count: Some(page_count),
        file_size: file_size(path),
        cover_hash: None,
    })?;

    let mut documents = Vec::with_capacity(usize::from(page_count));
    for page in 0..page_count {
        documents.push(IndexDocument {
            id: id.as_str().to_owned(),
            title: title.clone().unwrap_or_default(),
            author: author.clone().unwrap_or_default(),
            body: doc.text_on_page(page).unwrap_or_default(),
            page: u64::from(page),
        });
        let completed_pages = u32::from(page) + 1;
        let total_pages = u32::from(page_count).max(1);
        page_progress(200 + (700 * completed_pages / total_pages));
    }
    page_progress(IMPORT_PROGRESS_UNITS_PER_PDF);

    Ok(ImportedRaindropPdf {
        entry: ImportedEntry {
            id,
            path: path.to_path_buf(),
            inserted,
        },
        index_documents: documents,
    })
}

fn clean_import_title(value: Option<&str>) -> Option<String> {
    let title = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title.eq_ignore_ascii_case("untitled") {
        None
    } else {
        Some(title.chars().take(512).collect())
    }
}

fn title_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|value| clean_import_title(Some(value)))
}

fn attributed_title(doc: &PdfDoc) -> Option<String> {
    doc.metadata_title()
        .ok()
        .flatten()
        .and_then(|title| clean_import_title(Some(&title)))
}

fn attributed_author(doc: &PdfDoc) -> Option<String> {
    doc.metadata_author()
        .ok()
        .flatten()
        .and_then(|author| clean_import_title(Some(&author)))
}

fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("Could not open {}.", path.display()))?;
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("Could not read {}.", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn raindrop_storage_dir(source_id: &str) -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs
        .data_dir()
        .join("raindrop")
        .join(safe_path_component(source_id))
        .join("files"))
}

fn safe_pdf_file_name(name: &str) -> String {
    let mut name = safe_path_component(name);
    if !name.to_lowercase().ends_with(".pdf") {
        name.push_str(".pdf");
    }
    name
}

fn safe_path_component(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim_matches(['.', '_', '-', ' ']);
    if cleaned.is_empty() {
        String::from("untitled")
    } else {
        cleaned.chars().take(180).collect()
    }
}

struct RaindropClient {
    http: reqwest::Client,
    token: String,
}

impl RaindropClient {
    fn new(token: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("PDF-Folio Raindrop Import")
                .build()?,
            token,
        })
    }

    async fn user(&self) -> Result<RaindropUser> {
        let response = self
            .get_json::<UserResponse>(&format!("{API_BASE}/user"))
            .await?;
        Ok(response.user)
    }

    async fn collections(&self) -> Result<Vec<RaindropCollection>> {
        let mut collections = self
            .get_json::<CollectionsResponse>(&format!("{API_BASE}/collections"))
            .await?
            .items;
        collections.extend(
            self.get_json::<CollectionsResponse>(&format!("{API_BASE}/collections/childrens"))
                .await?
                .items,
        );
        collections
            .sort_by_key(|collection| (collection.parent_id().unwrap_or(0), collection.sort));
        collections.dedup_by_key(|collection| collection.id);
        Ok(collections)
    }

    async fn raindrop(&self, id: i64) -> Result<Raindrop> {
        let response = self
            .get_json::<RaindropResponse>(&format!("{API_BASE}/raindrop/{id}"))
            .await?;
        Ok(response.item)
    }

    async fn pdf_raindrops(&self) -> Result<Vec<Raindrop>> {
        let mut page = 0_u32;
        let mut raindrops = Vec::new();

        loop {
            let mut url = Url::parse(&format!("{API_BASE}/raindrops/0"))?;
            url.query_pairs_mut()
                .append_pair("page", &page.to_string())
                .append_pair("perpage", &MAX_PER_PAGE.to_string())
                .append_pair("sort", "-created");
            let response = self.get_json::<RaindropsResponse>(url.as_str()).await?;
            let count = response.items.len();
            raindrops.extend(response.items.into_iter().filter(Raindrop::is_pdf));
            if count < usize::from(MAX_PER_PAGE) {
                break;
            }
            page += 1;
        }

        Ok(raindrops)
    }

    async fn download_pdf(&self, link: &str) -> Result<Vec<u8>> {
        let mut request = self.http.get(link).header("Accept", "application/pdf,*/*");
        if download_requires_raindrop_auth(link) {
            request = request.header(AUTHORIZATION, format!("Bearer {}", self.token));
        }
        let response = request.send().await?.error_for_status()?;
        let bytes = response.bytes().await?.to_vec();
        ensure_pdf_response(link, &bytes)?;
        Ok(bytes)
    }

    async fn download_pdf_for_raindrop(&self, raindrop: &Raindrop) -> Result<Vec<u8>> {
        if raindrop.has_uploaded_file() {
            return self
                .download_pdf(&format!("{API_BASE}/raindrop/{}/cache", raindrop.id))
                .await;
        }

        self.download_pdf(raindrop.download_link()).await
    }

    async fn download_pdf_export_zip(&self, mut progress: impl FnMut(u16)) -> Result<Vec<u8>> {
        let mut url = Url::parse(&format!("{API_BASE}/raindrops/0/export.zip"))?;
        url.query_pairs_mut().append_pair("search", "file:true");
        let response = self
            .http
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header("Accept", "application/zip,application/octet-stream,*/*")
            .send()
            .await?
            .error_for_status()?;
        let content_length = response.content_length();
        progress(ZIP_PREPARING_PROGRESS_BASIS_POINTS);

        let mut bytes = Vec::with_capacity(
            content_length
                .and_then(|length| length.try_into().ok())
                .unwrap_or_default(),
        );
        let mut downloaded = 0_u64;
        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            bytes.extend_from_slice(&chunk);
            if let Some(content_length) = content_length {
                progress(zip_download_progress_basis_points(
                    downloaded,
                    content_length,
                ));
            } else {
                progress(ZIP_PREPARING_PROGRESS_BASIS_POINTS);
            }
        }
        progress(ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS);
        if !bytes.starts_with(b"PK") {
            let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
                .replace('\n', " ")
                .replace('\r', " ");
            bail!("Raindrop export response was not a ZIP archive. Response preview: {preview}");
        }
        Ok(bytes)
    }

    async fn get_json<T>(&self, url: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let body = self
            .http
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
            .with_context(|| format!("Could not read Raindrop response from {url}."))?;
        serde_json::from_str::<T>(&body).map_err(|error| {
            let preview = body.chars().take(240).collect::<String>();
            anyhow!("Could not decode Raindrop response from {url}: {error}. Response preview: {preview}")
        })
    }
}

fn download_requires_raindrop_auth(link: &str) -> bool {
    Url::parse(link)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "raindrop.io" || host.ends_with(".raindrop.io"))
}

fn ensure_pdf_response(link: &str, bytes: &[u8]) -> Result<()> {
    if bytes
        .windows(b"%PDF-".len())
        .take(1024)
        .any(|window| window == b"%PDF-")
    {
        return Ok(());
    }

    let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(160)])
        .replace('\n', " ")
        .replace('\r', " ");
    bail!("Downloaded content from {link} was not a PDF. Response preview: {preview}");
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    user: RaindropUser,
}

#[derive(Debug, Deserialize)]
struct RaindropUser {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    id: i64,
    #[serde(rename = "fullName")]
    full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CollectionsResponse {
    items: Vec<RaindropCollection>,
}

#[derive(Debug, Clone, Deserialize)]
struct RaindropCollection {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "i64_or_default")]
    sort: i64,
    #[serde(default, deserialize_with = "optional_ref")]
    parent: Option<RaindropRef>,
}

impl RaindropCollection {
    fn title(&self) -> String {
        self.title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Raindrop collection {}", self.id))
    }

    fn parent_id(&self) -> Option<i64> {
        self.parent.as_ref().map(|parent| parent.id)
    }
}

#[derive(Debug, Deserialize)]
struct RaindropsResponse {
    items: Vec<Raindrop>,
}

#[derive(Debug, Deserialize)]
struct RaindropResponse {
    item: Raindrop,
}

#[derive(Debug, Clone, Deserialize)]
struct Raindrop {
    #[serde(rename = "_id", deserialize_with = "i64_from_json")]
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    link: String,
    #[serde(default)]
    cover: Option<String>,
    #[serde(default)]
    media: Vec<RaindropMedia>,
    #[serde(rename = "type")]
    item_type: Option<String>,
    #[serde(default, deserialize_with = "optional_ref")]
    collection: Option<RaindropRef>,
    #[serde(default)]
    tags: Vec<String>,
    file: Option<RaindropFile>,
    #[serde(rename = "lastUpdate")]
    last_update: Option<String>,
    #[serde(skip)]
    uploaded_file: bool,
}

impl Raindrop {
    fn is_pdf(&self) -> bool {
        self.file.as_ref().is_some_and(|file| file.is_pdf())
            || self
                .item_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("document"))
                && self.link.to_lowercase().contains(".pdf")
            || self.link.to_lowercase().ends_with(".pdf")
    }

    fn collection_id(&self) -> Option<i64> {
        self.collection.as_ref().map(|collection| collection.id)
    }

    fn file_name(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|file| file.name.clone())
            .or_else(|| self.title.clone())
    }

    fn file_size(&self) -> Option<u64> {
        self.file.as_ref().and_then(|file| file.size)
    }

    fn download_link(&self) -> &str {
        self.file
            .as_ref()
            .and_then(|file| file.link.as_deref())
            .filter(|link| !link.trim().is_empty())
            .unwrap_or(&self.link)
    }

    fn has_uploaded_file(&self) -> bool {
        if self.uploaded_file {
            return true;
        }
        self.file.as_ref().is_some_and(|file| {
            file.is_pdf()
                || file
                    .link
                    .as_deref()
                    .is_some_and(|link| download_requires_raindrop_auth(link))
        })
    }

    fn thumbnail_url(&self) -> Option<String> {
        self.cover
            .as_ref()
            .filter(|cover| !cover.trim().is_empty())
            .cloned()
            .or_else(|| {
                self.media
                    .iter()
                    .find_map(|media| media.link.as_ref().filter(|link| !link.trim().is_empty()))
                    .cloned()
            })
    }

    fn display_label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.file_name())
            .unwrap_or_else(|| format!("Raindrop {}", self.id))
    }

    fn to_candidate(&self, collection_titles: &HashMap<i64, String>) -> RaindropPdfCandidate {
        let collection_id = self.collection_id();
        RaindropPdfCandidate {
            id: self.id,
            title: self.display_label(),
            file_name: self.file_name(),
            file_size: self.file_size(),
            thumbnail_url: self.thumbnail_url(),
            download_link: self.download_link().to_owned(),
            uploaded_file: self.has_uploaded_file(),
            tags: self.tags.clone(),
            collection_id,
            collection_title: collection_id.and_then(|id| collection_titles.get(&id).cloned()),
            remote_updated_at: self.last_update.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct RaindropRef {
    #[serde(rename = "$id", deserialize_with = "i64_from_json")]
    id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RaindropFile {
    name: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default, deserialize_with = "optional_u64")]
    size: Option<u64>,
    #[serde(rename = "type")]
    mime_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RaindropMedia {
    #[serde(default)]
    link: Option<String>,
}

fn optional_u64<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_u64(),
        Some(serde_json::Value::String(value)) => value.parse::<u64>().ok(),
        _ => None,
    })
}

fn optional_ref<'de, D>(deserializer: D) -> std::result::Result<Option<RaindropRef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };
    let id = match value {
        serde_json::Value::Object(mut object) => object.remove("$id"),
        value @ (serde_json::Value::Number(_) | serde_json::Value::String(_)) => Some(value),
        _ => None,
    };
    match id {
        Some(serde_json::Value::Number(number)) => Ok(number.as_i64().map(|id| RaindropRef { id })),
        Some(serde_json::Value::String(value)) => {
            Ok(value.parse::<i64>().ok().map(|id| RaindropRef { id }))
        }
        _ => Ok(None),
    }
}

fn i64_from_json<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("id was not a signed integer")),
        serde_json::Value::String(value) => value.parse::<i64>().map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("id was not a number or string")),
    }
}

fn i64_or_default<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or_default(),
        Some(serde_json::Value::String(value)) => value.parse::<i64>().unwrap_or_default(),
        _ => 0,
    })
}

impl RaindropFile {
    fn is_pdf(&self) -> bool {
        self.mime_type
            .as_deref()
            .is_some_and(|mime| mime.eq_ignore_ascii_case("application/pdf"))
            || self
                .name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().ends_with(".pdf"))
    }
}

#[cfg(test)]
mod tests;
