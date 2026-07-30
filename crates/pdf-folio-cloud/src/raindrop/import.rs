//! Raindrop import orchestration and local PDF metadata import.
//!
//! Public entry points used by the UI and CLI to preview and import PDFs from a
//! Raindrop.io account into a [`pdf_folio_core::Db`]. This module sequences
//! auth → REST listing → optional collection mirroring → download (individual
//! or ZIP) → local import + provenance mapping.
//!
//! # Public API surface
//!
//! - [`import_preview`] / [`import_preview_with_auth`] — list remote PDFs only
//! - [`import_all_pdfs`] / [`import_all_pdfs_with_auth`] — import every PDF
//! - [`import_selected_pdfs`] (+ `_with_progress` / `_with_auth`) — subset by id
//! - [`import_preview_pdfs_with_progress`] — import already-fetched candidates
//!
//! Destination folders are controlled by [`RaindropImportDestination`]: preserve
//! Raindrop structure (optionally under a local root), library root, or one folder.
//!
//! # Progress model
//!
//! Callbacks receive [`RaindropImportProgress`] with optional
//! `progress_basis_points` (0–[`PROGRESS_BASIS_POINTS_MAX`]) so ZIP download,
//! extract, and per-PDF import can share one non-linear progress bar.
//!
//! # Related
//!
//! - Auth: [`super::auth`]
//! - HTTP/download: [`super::client`]
//! - ZIP strategy: [`super::matching`]
//! - DTOs: [`super::types`]
//! - Provenance tables: `pdf-folio-core` raindrop mapping APIs

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use pdf_folio_core::PdfDoc;
use pdf_folio_core::{
    clean_import_title, hash_file, title_from_path, Db, EntryId, FolderId, ImportSummary,
    ImportedEntry, IndexDocument, NewLibraryEntry, RaindropEntryMapping, SearchIndex,
};

use super::auth::resolve_access_token;
use super::client::{Raindrop, RaindropClient, RaindropCollection, RaindropUser};
use super::matching::{
    choose_import_strategy, extract_selected_pdfs_from_zip, RaindropImportStrategy,
};
use super::*;

/// Basis-point span reserved for the per-PDF import phase after ZIP extract (5_000 → 10_000).
const ZIP_IMPORTING_PROGRESS_BASIS_POINTS: u16 = 5_000;
/// Full progress scale (100.00%) expressed in basis points (1/100 of a percent).
pub(crate) const PROGRESS_BASIS_POINTS_MAX: u16 = 10_000;
/// Fine-grained progress units allotted to one PDF during ZIP-backed import (page extract + index).
const IMPORT_PROGRESS_UNITS_PER_PDF: u32 = 1_000;

/// Imports all PDFs from the authenticated Raindrop.io account.
///
/// Authentication is resolved by [`super::auth::resolve_access_token`]:
/// env token → cached OAuth token → browser OAuth (env/bundled app credentials).
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

/// Imports candidates from an existing preview without re-listing the account PDFs.
///
/// Resolves auth, optionally loads collections for folder mirroring, then runs
/// [`import_prepared_raindrops`].
///
/// # Errors
///
/// Returns an error when auth, API, download, or local database work fails.
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

/// Shared import path: resolve token, list PDFs (optionally filtered), then import.
///
/// When `selected_ids` is set, each kept raindrop is re-fetched for fuller file metadata.
///
/// # Errors
///
/// Returns an error when auth, API, download, or local database work fails.
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

/// Mirrors collections when needed, picks ZIP vs individual strategy, and imports the list.
///
/// ZIP failures for uploaded files degrade to error rows for those raindrops; linked PDFs
/// still download individually after a partial ZIP pass.
///
/// # Errors
///
/// Returns an error when storage setup or a fatal strategy path fails.
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
            mirror_collections(db, source_id, &collections, None)?
        }
        RaindropImportDestination::PreserveRaindropFoldersUnder(root_folder_id) => {
            mirror_collections(db, source_id, &collections, root_folder_id.as_ref())?
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
    let storage_dir = raindrop_storage_dir(source_id)?;
    let import = match import_strategy {
        RaindropImportStrategy::ZipExport => {
            let (uploaded_raindrops, linked_raindrops): (Vec<_>, Vec<_>) = raindrops
                .iter()
                .cloned()
                .partition(Raindrop::has_uploaded_file);
            let mut import = match import_raindrop_pdfs_from_zip(
                db,
                client,
                source_id,
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
                    client,
                    source_id,
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
                client,
                source_id,
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

/// Appends `source` entries/errors into `target`, deduping by entry id.
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

/// True when the destination should mirror Raindrop collection folders locally.
fn destination_preserves_raindrop_folders(destination: &RaindropImportDestination) -> bool {
    matches!(
        destination,
        RaindropImportDestination::PreserveRaindropFolders
            | RaindropImportDestination::PreserveRaindropFoldersUnder(_)
    )
}

#[cfg(test)]
/// Basis points for the post-extract import phase of a ZIP-backed import.
pub(crate) fn zip_import_progress_basis_points(completed: usize, total: usize) -> u16 {
    if total == 0 {
        return PROGRESS_BASIS_POINTS_MAX;
    }

    zip_import_progress_basis_points_for_units(
        (completed.min(total) as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
        (total as u32) * IMPORT_PROGRESS_UNITS_PER_PDF,
    )
}

/// Maps completed import units into the post-extract basis-point range up to [`PROGRESS_BASIS_POINTS_MAX`].
fn zip_import_progress_basis_points_for_units(completed_units: u32, total_units: u32) -> u16 {
    if total_units == 0 {
        return PROGRESS_BASIS_POINTS_MAX;
    }

    let base = u32::from(ZIP_EXTRACTED_PROGRESS_BASIS_POINTS);
    let importing = u32::from(ZIP_IMPORTING_PROGRESS_BASIS_POINTS);
    (base + importing * completed_units.min(total_units) / total_units)
        .min(u32::from(PROGRESS_BASIS_POINTS_MAX)) as u16
}

/// Basis points while walking ZIP members during extract.
pub(crate) fn zip_extract_progress_basis_points(
    processed_entries: usize,
    total_entries: usize,
) -> u16 {
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

/// Invokes an optional progress callback (no-op when `None`).
pub(crate) fn report_raindrop_progress(
    progress: &mut Option<&mut (dyn FnMut(RaindropImportProgress) + Send)>,
    event: RaindropImportProgress,
) {
    if let Some(progress) = progress.as_deref_mut() {
        progress(event);
    }
}

/// One successfully imported raindrop PDF plus search documents for indexing.
struct ImportedRaindropPdf {
    /// Local library entry produced by the import.
    entry: ImportedEntry,
    /// Per-page search documents to commit into the search index.
    index_documents: Vec<IndexDocument>,
}

/// Downloads and imports each raindrop PDF one-by-one, reporting progress per item.
///
/// Individual failures become error strings; the function always returns a summary.
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

/// Downloads Raindrop’s bulk ZIP export, matches members to raindrops, and imports extracted PDFs.
///
/// Batches search-index writes after the import loop. Missing ZIP members become per-item errors.
///
/// # Errors
///
/// Returns an error when the ZIP download, extract, or search index open fails fatally.
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

/// Human-readable Raindrop account label (`fullName`, else `Raindrop user {id}`).
fn account_label(user: &RaindropUser) -> String {
    user.full_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| format!("Raindrop user {}", user.id))
}

/// Creates local folders that mirror Raindrop collections and returns id maps.
///
/// Writes raindrop↔folder provenance via `pdf-folio-core` mapping tables.
///
/// # Errors
///
/// Returns an error when folder creation or mapping persistence fails.
pub(crate) fn mirror_collections(
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

/// Downloads one raindrop PDF to `storage_dir` and imports it into the library.
///
/// # Errors
///
/// Returns an error when download, disk write, or library import fails.
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

/// Imports an already-downloaded PDF: metadata, tags, destination folder, provenance mapping.
///
/// # Errors
///
/// Returns an error when PDF open, library insert, or mapping writes fail.
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

/// Adds an entry to a mirrored Raindrop collection folder (thin wrapper for folder membership).
///
/// # Errors
///
/// Returns an error when the membership write fails.
fn add_entry_to_raindrop_folder(db: &Db, entry_id: &EntryId, folder_id: &FolderId) -> Result<()> {
    db.add_entry_to_folder(entry_id, folder_id)
}

/// Hashes, inserts, and extracts page text for a PDF at `path`, preferring `remote_title`.
///
/// Reports coarse page-progress units via `page_progress` (0–[`IMPORT_PROGRESS_UNITS_PER_PDF`]).
///
/// # Errors
///
/// Returns an error when hashing, PDF open, or database insert fails.
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
    let title = remote_title
        .and_then(clean_import_title)
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

/// Cleaned PDF document-info title when present.
fn attributed_title(doc: &PdfDoc) -> Option<String> {
    doc.metadata_title()
        .ok()
        .flatten()
        .and_then(clean_import_title)
}

/// Cleaned PDF document-info author when present.
fn attributed_author(doc: &PdfDoc) -> Option<String> {
    doc.metadata_author()
        .ok()
        .flatten()
        .and_then(clean_import_title)
}

/// File size in bytes, or `None` when metadata cannot be read.
fn file_size(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().map(|metadata| metadata.len())
}

/// XDG data dir for Raindrop downloads: `…/raindrop/<source>/files`.
///
/// # Errors
///
/// Returns an error when the platform data directory cannot be resolved.
fn raindrop_storage_dir(source_id: &str) -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs
        .data_dir()
        .join("raindrop")
        .join(safe_path_component(source_id))
        .join("files"))
}

/// Sanitizes a remote file name for local storage (path-safe, ends with `.pdf`).
pub(crate) fn safe_pdf_file_name(name: &str) -> String {
    let mut name = safe_path_component(name);
    if !name.to_lowercase().ends_with(".pdf") {
        name.push_str(".pdf");
    }
    name
}

/// Path-safe single component: alphanumerics, `.`/`-`/`_`, max 180 chars (else `untitled`).
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
