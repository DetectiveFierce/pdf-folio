//! ZIP export matching for Raindrop PDF imports.
//!
//! When many selected raindrops are **uploaded files**, downloading them one
//! API call at a time is slow. Raindrop’s bulk export ZIP is preferred once the
//! uploaded-file count reaches [`super::ZIP_IMPORT_THRESHOLD`]. This module
//! chooses the strategy and maps ZIP members back onto raindrop ids.
//!
//! # Matching priority ([`ZipMatchIndex::match_entry`])
//!
//! 1. Exact normalized name + size (unique remaining candidate)
//! 2. Exact name alone
//! 3. Exact size alone
//! 4. File stem match
//! 5. Suffix match on name or stem
//!
//! Ambiguous matches return `None` so a PDF is never attached to the wrong item.
//!
//! # Related
//!
//! - Threshold/progress constants: [`super`]
//! - ZIP download: [`super::client::RaindropClient::download_pdf_export_zip`]
//! - Orchestration: [`super::import`]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::client::{ensure_pdf_response, Raindrop};
use super::import::{
    report_raindrop_progress, safe_pdf_file_name, zip_extract_progress_basis_points,
};
use super::{
    RaindropImportPhase, RaindropImportProgress, ZIP_EXTRACTED_PROGRESS_BASIS_POINTS,
    ZIP_IMPORT_THRESHOLD,
};

/// How to fetch PDF bytes for a selected import set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RaindropImportStrategy {
    /// One guarded download per raindrop.
    IndividualFiles,
    /// Single Raindrop export ZIP + local matching.
    ZipExport,
}

/// Prefers ZIP export when enough selected items are uploaded Raindrop files.
pub(crate) fn choose_import_strategy(raindrops: &[Raindrop]) -> RaindropImportStrategy {
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

/// Extracts PDFs from a Raindrop export ZIP into `storage_dir`, keyed by raindrop id.
///
/// Only entries that uniquely match a remaining selected raindrop are written.
/// Reports extract progress via the optional callback.
///
/// # Errors
///
/// Returns an error when the archive is invalid or a matched file cannot be written.
pub(crate) fn extract_selected_pdfs_from_zip(
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

/// Indexes selected raindrops by normalized name, stem, and size for ZIP matching.
pub(crate) struct ZipMatchIndex {
    names: HashMap<String, Vec<usize>>,
    stems: HashMap<String, Vec<usize>>,
    sizes: HashMap<u64, Vec<usize>>,
    name_by_index: Vec<Option<String>>,
    stem_by_index: Vec<Option<String>>,
    size_by_index: Vec<Option<u64>>,
}

impl ZipMatchIndex {
    /// Builds lookup tables from the selected raindrop list (indices into that slice).
    pub(crate) fn new(raindrops: &[Raindrop]) -> Self {
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

    /// Finds a unique remaining raindrop index for a ZIP member, if any.
    pub(crate) fn match_entry(
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

/// Returns the sole remaining index from `indexes`, or `None` if zero or many match.
pub(crate) fn unique_remaining_match(
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

/// Lowercased, path-safe file name for ZIP matching.
pub(crate) fn normalized_zip_file_name(name: &str) -> String {
    safe_pdf_file_name(name).to_lowercase()
}

/// Normalized name with a trailing `.pdf` stripped.
pub(crate) fn normalized_zip_file_stem(name: &str) -> String {
    normalized_zip_file_name(name)
        .trim_end_matches(".pdf")
        .to_owned()
}
