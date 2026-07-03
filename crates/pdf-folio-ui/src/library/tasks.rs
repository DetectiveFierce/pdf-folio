//! Async task constructors and blocking helpers for library operations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::Task;
use pdf_folio_core::PdfDoc;
use pdf_folio_db::{
    hash_file, scan_pdf_files, Db, EntryId, FolderId, ImportSummary, ImportedEntry, IndexDocument,
    LibraryEntry, LibrarySortMode, LibraryWatchEvent, NewLibraryEntry, SearchIndex,
};

use crate::library::filters::entry_matches_query;
use crate::library::metadata::{entry_author, entry_title, file_size};
use crate::messages::Message;

pub(crate) fn persist_manual_entry_order_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.set_manual_entry_order(&entry_ids)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::ManualEntryOrderSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn persist_manual_folder_order_task(
    db: Arc<Db>,
    parent_id: Option<FolderId>,
    folder_ids: Vec<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.set_manual_folder_order(parent_id.as_ref(), &folder_ids)
            })
            .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_operation_task<F>(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    label: String,
    operation: F,
) -> Task<Message>
where
    F: Fn(&Db, &EntryId) -> anyhow::Result<()> + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match operation(&db, &entry_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                (label, updated, errors)
            })
            .await
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn rename_folder_task(db: Arc<Db>, folder_id: FolderId, name: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.rename_folder(&folder_id, &name)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn move_folder_task(
    db: Arc<Db>,
    folder_id: FolderId,
    new_parent_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.move_folder(&folder_id, new_parent_id.as_ref()))
                .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn delete_folder_task(db: Arc<Db>, folder_id: FolderId) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.delete_folder(&folder_id)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::FolderUpdated,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn move_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: FolderId,
) -> Task<Message> {
    Task::perform(
        async move {
            let completed_folder_id = folder_id.clone();
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match db.move_entry_to_folder(&entry_id, &folder_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                Ok::<_, anyhow::Error>((
                    completed_folder_id,
                    String::from("Moved to folder"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((folder_id, label, updated, errors)) => Message::FolderAssignmentFinished {
                folder_id,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn edit_metadata_task(
    db: Arc<Db>,
    entry: LibraryEntry,
    display_title: String,
    display_author: String,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.update_display_metadata(&entry.id, Some(&display_title), Some(&display_author))?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let label = format!("Saved metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn reset_metadata_task(db: Arc<Db>, entry: LibraryEntry) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.reset_display_metadata(&entry.id)?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let label = format!("Reset metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                label,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn relink_entry_task(db: Arc<Db>, entry_id: EntryId, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking({
                let entry_id = entry_id.clone();
                let path = path.clone();
                move || {
                    db.relink_entry_path(&entry_id, &path)?;
                    Ok::<_, anyhow::Error>((entry_id, path))
                }
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, path)) => Message::RelinkFinished { entry_id, path },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_reset_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for mut entry in entries {
                    entry.display_title = None;
                    entry.display_author = None;
                    entry.metadata_locked = false;
                    match db
                        .reset_display_metadata(&entry.id)
                        .and_then(|()| reindex_entry(&search_index, &entry))
                    {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Reset metadata for"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_refresh_metadata_task(db: Arc<Db>, entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for mut entry in entries {
                    match refresh_entry_metadata(&db, &search_index, &mut entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Refreshed metadata for"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_delete_metadata_task(db: Arc<Db>, entry_ids: Vec<EntryId>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match db
                        .delete_entry(&entry_id)
                        .and_then(|()| search_index.delete_entry(entry_id.as_str()))
                    {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Deleted from library"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_reindex_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry in entries {
                    match reindex_entry(&search_index, &entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                Ok::<_, anyhow::Error>((String::from("Reindexed"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn attribute_pending_metadata_task(db: Arc<Db>) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || attribute_pending_metadata(&db)).await? },
        |result| match result {
            Ok(()) => Message::AuthorAttributionFinished,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) async fn search_library_task(
    db: Arc<Db>,
    query: String,
    sort_mode: LibrarySortMode,
) -> anyhow::Result<(Vec<LibraryEntry>, HashMap<EntryId, u16>)> {
    tokio::task::spawn_blocking(move || {
        let entries = db.get_entries_sorted(sort_mode)?;
        let normalized = query.trim().to_lowercase();
        let search_index = SearchIndex::open_default()?;
        let hits = search_index.search(&query, 200).unwrap_or_default();
        let mut hit_pages = HashMap::new();
        let mut ordered_entries = Vec::new();

        for hit in hits {
            let id = EntryId::new(hit.id);
            if hit_pages.contains_key(&id) {
                continue;
            }
            hit_pages.insert(id.clone(), hit.page.min(u64::from(u16::MAX)) as u16);
            if let Some(entry) = entries.iter().find(|entry| entry.id == id) {
                ordered_entries.push(entry.clone());
            }
        }

        for entry in entries {
            if hit_pages.contains_key(&entry.id) || !entry_matches_query(&entry, &normalized) {
                continue;
            }
            ordered_entries.push(entry);
        }

        Ok((ordered_entries, hit_pages))
    })
    .await?
}

pub(crate) fn import_folder_with_index(db: &Db, root: &Path) -> anyhow::Result<ImportSummary> {
    let paths = scan_pdf_files(root)?;
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for path in paths {
        match import_pdf_with_index(db, path.clone()) {
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    Ok(ImportSummary { entries, errors })
}

pub(crate) fn apply_watch_event(db: &Db, event: LibraryWatchEvent) -> anyhow::Result<()> {
    match event {
        LibraryWatchEvent::PdfCreated(path) => {
            if path.exists() {
                import_pdf_with_index(db, path)?;
            }
        }
        LibraryWatchEvent::PdfRemoved(path) => {
            db.set_missing_by_path(&path, true)?;
        }
    }
    Ok(())
}

fn refresh_entry_metadata(
    db: &Db,
    search_index: &SearchIndex,
    entry: &mut LibraryEntry,
) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let author = attributed_author(&doc);
    let page_count = Some(doc.page_count());
    db.update_author_attribution(&entry.id, author.as_deref())?;
    db.update_page_count_attribution(&entry.id, page_count)?;
    entry.author = author;
    entry.page_count = page_count;
    entry.author_attributed = true;
    entry.page_count_attributed = true;
    reindex_entry(search_index, entry)
}

fn reindex_entry(search_index: &SearchIndex, entry: &LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let title = entry_title(entry);
    let author = entry_author(entry);
    let mut documents = Vec::with_capacity(usize::from(doc.page_count()));
    for page in 0..doc.page_count() {
        documents.push(IndexDocument {
            id: entry.id.as_str().to_owned(),
            title: title.clone(),
            author: author.clone(),
            body: doc.text_on_page(page).unwrap_or_default(),
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;
    Ok(())
}

fn import_pdf_with_index(db: &Db, path: PathBuf) -> anyhow::Result<ImportedEntry> {
    let id = EntryId::new(hash_file(&path)?);
    let inserted = db.entry_by_path(&path)?.is_none();
    let doc = PdfDoc::open(&path)?;
    let title = attributed_title(&doc).or_else(|| title_from_path(&path));
    let page_count = doc.page_count();
    let author = attributed_author(&doc);

    db.insert_entry(&NewLibraryEntry {
        id: id.clone(),
        path: path.clone(),
        title: title.clone(),
        author: author.clone(),
        author_attributed: true,
        page_count_attributed: true,
        page_count: Some(page_count),
        file_size: file_size(&path),
        cover_hash: None,
    })?;

    let search_index = SearchIndex::open_default()?;
    let mut documents = Vec::with_capacity(usize::from(page_count));
    for page in 0..page_count {
        let body = doc.text_on_page(page).unwrap_or_default();
        documents.push(IndexDocument {
            id: id.as_str().to_owned(),
            title: title.clone().unwrap_or_default(),
            author: author.clone().unwrap_or_default(),
            body,
            page: u64::from(page),
        });
    }
    search_index.replace_entry_pages(documents)?;

    Ok(ImportedEntry { id, path, inserted })
}

fn attribute_pending_metadata(db: &Db) -> anyhow::Result<()> {
    for entry in db.get_all_entries()?.into_iter().filter(|entry| {
        !entry.missing && (!entry.author_attributed || !entry.page_count_attributed)
    }) {
        let doc = open_entry_doc(&entry);
        if !entry.author_attributed {
            let author = doc.as_ref().and_then(attributed_author);
            db.update_author_attribution(&entry.id, author.as_deref())?;
        }
        if !entry.page_count_attributed {
            let page_count = doc.as_ref().map(|doc| doc.page_count());
            db.update_page_count_attribution(&entry.id, page_count)?;
        }
    }

    Ok(())
}

fn open_entry_doc(entry: &LibraryEntry) -> Option<PdfDoc> {
    entry
        .path
        .exists()
        .then(|| PdfDoc::open(&entry.path).ok())
        .flatten()
}

fn attributed_author(doc: &PdfDoc) -> Option<String> {
    doc.metadata_author()
        .ok()
        .flatten()
        .and_then(|author| clean_author_candidate(&author))
        .or_else(|| author_from_contents(doc))
}

fn attributed_title(doc: &PdfDoc) -> Option<String> {
    doc.metadata_title()
        .ok()
        .flatten()
        .and_then(clean_import_title)
}

#[cfg(test)]
pub(crate) fn title_from_path(path: &Path) -> Option<String> {
    title_from_path_inner(path)
}

#[cfg(not(test))]
fn title_from_path(path: &Path) -> Option<String> {
    title_from_path_inner(path)
}

fn title_from_path_inner(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(clean_import_title)
}

pub(crate) fn clean_import_title(value: impl AsRef<str>) -> Option<String> {
    let title = value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title.is_empty() || title.eq_ignore_ascii_case("untitled") {
        None
    } else {
        Some(title.chars().take(512).collect())
    }
}

fn author_from_contents(doc: &PdfDoc) -> Option<String> {
    let pages_to_scan = doc.page_count().min(3);
    for page in 0..pages_to_scan {
        let text = doc.text_on_page(page).ok()?;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(author) = author_from_line(line) {
                return Some(author);
            }
        }
    }
    None
}

fn author_from_line(line: &str) -> Option<String> {
    let normalized = line.trim().trim_matches(['.', ',', ';', ':']);
    for prefix in ["Author:", "Authors:", "By:", "Written by "] {
        if let Some(candidate) = normalized.strip_prefix(prefix) {
            return clean_author_candidate(candidate);
        }
    }

    normalized
        .strip_prefix("By ")
        .and_then(clean_author_candidate)
}

fn clean_author_candidate(candidate: &str) -> Option<String> {
    let candidate = candidate
        .trim()
        .trim_matches(['.', ',', ';', ':', '-', ' '])
        .replace('\n', " ");
    let candidate = candidate.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = candidate.to_lowercase();
    let digit_count = candidate.chars().filter(|ch| ch.is_ascii_digit()).count();

    if candidate.len() < 2
        || candidate.len() > 80
        || lower == "anonymous"
        || lower == "unknown"
        || lower.contains("http")
        || lower.contains("www.")
        || lower.contains("copyright")
        || digit_count > 4
    {
        return None;
    }

    Some(candidate)
}
