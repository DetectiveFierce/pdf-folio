//! Async task constructors and blocking helpers for library operations.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::Task;
use pdf_folio_core::PdfDoc;
use pdf_folio_db::{
    hash_file, scan_pdf_files, Db, EntryId, Folder, FolderId, ImportSummary, ImportedEntry,
    IndexDocument, LibraryEntry, LibraryOrganizationSnapshot, LibrarySortMode, LibraryWatchEvent,
    NewLibraryEntry, SearchIndex,
};

use crate::library::filters::entry_matches_query;
use crate::library::metadata::{entry_author, entry_title, file_size};
use crate::messages::Message;
use crate::{LibraryClipboard, LibraryClipboardMode, LibraryClipboardTarget, LibraryHistoryAction};

pub(crate) fn persist_manual_entry_order_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.set_manual_entry_order(&entry_ids)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Manual PDF Order"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Manual PDF order saved"),
                    entry_ids.len(),
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
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
                let before = db.library_organization_snapshot()?;
                db.set_manual_folder_order(parent_id.as_ref(), &folder_ids)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Manual Folder Order"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Manual folder order saved"),
                    folder_ids.len(),
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_operation_task<F>(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    label: String,
    history_label: String,
    operation: F,
) -> Task<Message>
where
    F: Fn(&Db, &EntryId) -> anyhow::Result<()> + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    match operation(&db, &entry_id) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: history_label,
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    label,
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
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
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.rename_folder(&folder_id, &name)?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Rename Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Renamed folder"),
                    1,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
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
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                db.move_folder(&folder_id, new_parent_id.as_ref())?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved folder"),
                    1,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn delete_folder_task(db: Arc<Db>, folder_id: FolderId) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let folders = db.get_folders()?;
                let folder_ids = folder_subtree_ids(&folders, &folder_id);
                let mut entry_ids = HashSet::new();
                for folder_id in &folder_ids {
                    for entry in db.entries_in_folder(folder_id)? {
                        entry_ids.insert(entry.id);
                    }
                }
                db.trash_folder_tree(&folder_id)?;
                if !entry_ids.is_empty() {
                    let search_index = SearchIndex::open_default()?;
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))?;
                }
                let after = db.library_organization_snapshot()?;
                let updated = folder_ids.len() + entry_ids.len();
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move Folder to Trash"),
                        refresh_search_on_restore: before.trash_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved to trash"),
                    updated,
                    Vec::<String>::new(),
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn folder_subtree_ids(folders: &[Folder], folder_id: &FolderId) -> HashSet<FolderId> {
    let mut folder_ids = HashSet::new();
    collect_folder_subtree_ids(folders, folder_id, &mut folder_ids);
    folder_ids
}

fn collect_folder_subtree_ids(
    folders: &[Folder],
    folder_id: &FolderId,
    folder_ids: &mut HashSet<FolderId>,
) {
    if !folder_ids.insert(folder_id.clone()) {
        return;
    }
    for child in folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
    {
        collect_folder_subtree_ids(folders, &child.id, folder_ids);
    }
}

pub(crate) fn move_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry_id in entry_ids {
                    let result = if let Some(folder_id) = folder_id.as_ref() {
                        db.move_entry_to_folder(&entry_id, folder_id)
                    } else {
                        db.move_entry_to_root(&entry_id)
                    };
                    match result {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_id.as_str())),
                    }
                }
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Move PDFs"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Moved"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn add_entries_to_folder_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
    folder_id: FolderId,
) -> Task<Message> {
    bulk_operation_task(
        db,
        entry_ids,
        String::from("Added to folder"),
        String::from("Add PDFs to Folder"),
        move |db, entry_id| db.add_entry_to_folder(entry_id, &folder_id),
    )
}

pub(crate) fn create_folder_task(
    db: Arc<Db>,
    name: String,
    parent_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let folder_id = db.create_folder(&name, parent_id.as_ref())?;
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    folder_id,
                    LibraryHistoryAction {
                        label: String::from("Create Folder"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                ))
            })
            .await?
        },
        |result| match result {
            Ok((folder_id, action)) => Message::FolderCreated { folder_id, action },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn paste_library_clipboard_task(
    db: Arc<Db>,
    clipboard: LibraryClipboard,
    destination_folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let mut updated = 0;
                let mut errors = Vec::new();

                match &clipboard.target {
                    LibraryClipboardTarget::Entries(entry_ids) => {
                        for entry_id in entry_ids {
                            let result = match clipboard.mode {
                                LibraryClipboardMode::Cut => {
                                    if let Some(folder_id) = destination_folder_id.as_ref() {
                                        db.move_entry_to_folder(entry_id, folder_id)
                                    } else {
                                        db.move_entry_to_root(entry_id)
                                    }
                                }
                                LibraryClipboardMode::Copy => {
                                    if let Some(folder_id) = destination_folder_id.as_ref() {
                                        db.add_entry_to_folder(entry_id, folder_id)
                                    } else {
                                        anyhow::bail!(
                                            "Copied PDFs can only be pasted into a folder."
                                        );
                                    }
                                }
                            };

                            match result {
                                Ok(()) => updated += 1,
                                Err(error) => {
                                    errors.push(format!("{}: {error}", entry_id.as_str()));
                                }
                            }
                        }
                    }
                    LibraryClipboardTarget::Folder(folder_id) => match clipboard.mode {
                        LibraryClipboardMode::Cut => {
                            db.move_folder(folder_id, destination_folder_id.as_ref())?;
                            updated = 1;
                        }
                        LibraryClipboardMode::Copy => {
                            db.copy_folder_subtree(folder_id, destination_folder_id.as_ref())?;
                            updated = 1;
                        }
                    },
                }

                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: clipboard.paste_label().to_owned(),
                        refresh_search_on_restore: before.trash_state_differs_from(&after),
                        before,
                        after,
                    },
                    clipboard,
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, clipboard, updated, errors)) => Message::LibraryClipboardPasteFinished {
                action,
                clipboard,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn restore_library_history_snapshot_task(
    db: Arc<Db>,
    snapshot: LibraryOrganizationSnapshot,
    target_index: usize,
    status: String,
    search_changed_entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.restore_library_organization_snapshot(&snapshot)?;
                if !search_changed_entry_ids.is_empty() {
                    let search_index = SearchIndex::open_default()?;
                    let changed_ids = search_changed_entry_ids
                        .iter()
                        .map(|entry_id| entry_id.as_str())
                        .collect::<std::collections::HashSet<_>>();
                    let active_entries = db
                        .get_all_entries()?
                        .into_iter()
                        .filter(|entry| changed_ids.contains(entry.id.as_str()))
                        .collect::<Vec<_>>();
                    reindex_entries(&search_index, &active_entries)?;

                    let active_ids = active_entries
                        .iter()
                        .map(|entry| entry.id.as_str())
                        .collect::<std::collections::HashSet<_>>();
                    let deleted_ids = search_changed_entry_ids
                        .iter()
                        .map(EntryId::as_str)
                        .filter(|entry_id| !active_ids.contains(entry_id))
                        .collect::<Vec<_>>();
                    if !deleted_ids.is_empty() {
                        search_index.delete_entries(deleted_ids)?;
                    }
                }
                Ok::<_, anyhow::Error>((target_index, status))
            })
            .await?
        },
        |result| match result {
            Ok((target_index, status)) => Message::LibraryHistoryRestoreFinished {
                target_index,
                status,
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
                let before = db.library_organization_snapshot()?;
                db.update_display_metadata(&entry.id, Some(&display_title), Some(&display_author))?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Edit Metadata"),
                    refresh_search_on_restore: before.search_state_differs_from(&after),
                    before,
                    after,
                };
                let label = format!("Saved metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), action, label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, action, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                action,
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
                let before = db.library_organization_snapshot()?;
                db.reset_display_metadata(&entry.id)?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                if let Err(error) = reindex_entry(&search_index, &entry) {
                    errors.push(format!("{}: {error}", entry_title(&entry)));
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Reset Metadata"),
                    refresh_search_on_restore: before.search_state_differs_from(&after),
                    before,
                    after,
                };
                let label = format!("Reset metadata for {}.", entry_title(&entry));
                Ok::<_, anyhow::Error>((entry.id.clone(), action, label, errors))
            })
            .await?
        },
        |result| match result {
            Ok((entry_id, action, label, errors)) => Message::MetadataEditFinished {
                entry_id,
                action,
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
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let mut reset_entries = Vec::new();
                for mut entry in entries {
                    entry.display_title = None;
                    entry.display_author = None;
                    entry.metadata_locked = false;
                    match db.reset_display_metadata(&entry.id) {
                        Ok(()) => reset_entries.push(entry),
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                let (updated, reindex_errors) =
                    reindex_entries_collecting_errors(&search_index, &reset_entries);
                errors.extend(reindex_errors);
                let after = db.library_organization_snapshot()?;
                Ok::<_, anyhow::Error>((
                    LibraryHistoryAction {
                        label: String::from("Reset Metadata"),
                        refresh_search_on_restore: before.search_state_differs_from(&after),
                        before,
                        after,
                    },
                    String::from("Reset metadata for"),
                    updated,
                    errors,
                ))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
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
                let mut errors = Vec::new();
                let mut refreshed_entries = Vec::new();
                for mut entry in entries {
                    match refresh_entry_metadata(&db, &mut entry) {
                        Ok(()) => refreshed_entries.push(entry),
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                let (updated, reindex_errors) =
                    reindex_entries_collecting_errors(&search_index, &refreshed_entries);
                errors.extend(reindex_errors);
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
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let updated = entry_ids.len();
                if let Err(error) = db.trash_entries(entry_ids.iter()) {
                    errors.push(error.to_string());
                } else if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }
                if !errors.is_empty() {
                    for entry_id in &entry_ids {
                        tracing::debug!(
                            entry_id = entry_id.as_str(),
                            "Bulk delete entry was part of a failed batch"
                        );
                    }
                }
                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Move to Trash"),
                    refresh_search_on_restore: before.trash_state_differs_from(&after),
                    before,
                    after,
                };
                Ok::<_, anyhow::Error>((action, String::from("Moved to trash"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_restore_trash_items_task(
    db: Arc<Db>,
    entries: Vec<LibraryEntry>,
    folder_id: Option<FolderId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let before = db.library_organization_snapshot()?;
                let search_index = SearchIndex::open_default()?;
                let mut entries_to_reindex = entries;
                let mut updated = entries_to_reindex.len();
                let restoring_folder = folder_id.is_some();

                if let Some(folder_id) = folder_id {
                    let folders = db.get_trashed_folders()?;
                    let folder_ids = folder_subtree_ids(&folders, &folder_id);
                    let trashed_entries = db.get_trashed_entries()?;
                    for entry in trashed_entries {
                        let in_restored_folder = entry
                            .folders
                            .iter()
                            .any(|folder| folder_ids.contains(&folder.id));
                        let already_selected = entries_to_reindex
                            .iter()
                            .any(|selected| selected.id == entry.id);
                        if in_restored_folder && !already_selected {
                            entries_to_reindex.push(entry);
                        }
                    }
                    updated += db.restore_folder_tree(&folder_id)?;
                }

                let entry_ids = entries_to_reindex
                    .iter()
                    .map(|entry| &entry.id)
                    .collect::<Vec<_>>();
                db.restore_entries(entry_ids.iter().copied())?;

                let (reindexed, errors) =
                    reindex_entries_collecting_errors(&search_index, &entries_to_reindex);
                if !restoring_folder {
                    updated = reindexed;
                }

                let after = db.library_organization_snapshot()?;
                let action = LibraryHistoryAction {
                    label: String::from("Restore from Trash"),
                    refresh_search_on_restore: before.trash_state_differs_from(&after),
                    before,
                    after,
                };
                Ok::<_, anyhow::Error>((action, String::from("Restored"), updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((action, label, updated, errors)) => Message::LibraryHistoryActionFinished {
                action,
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_permanently_delete_entries_task(
    db: Arc<Db>,
    entry_ids: Vec<EntryId>,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let updated = entry_ids.len();
                if let Err(error) = db.delete_entries(entry_ids.iter()) {
                    errors.push(error.to_string());
                } else if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }

                Ok::<_, anyhow::Error>((String::from("Permanently deleted"), updated, errors))
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

pub(crate) fn permanently_delete_folder_from_trash_task(
    db: Arc<Db>,
    folder_id: FolderId,
) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let mut errors = Vec::new();
                let (updated, entry_ids) = db.permanently_delete_trashed_folder_tree(&folder_id)?;
                if let Err(error) =
                    search_index.delete_entries(entry_ids.iter().map(EntryId::as_str))
                {
                    errors.push(format!("search index: {error}"));
                }
                Ok::<_, anyhow::Error>((updated, errors))
            })
            .await?
        },
        |result| match result {
            Ok((updated, errors)) => Message::TrashFolderPermanentlyDeleted { updated, errors },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

pub(crate) fn bulk_reindex_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let search_index = SearchIndex::open_default()?;
                let (updated, errors) = reindex_entries_collecting_errors(&search_index, &entries);
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
    trash_view_active: bool,
) -> anyhow::Result<(Vec<LibraryEntry>, HashMap<EntryId, u16>)> {
    tokio::task::spawn_blocking(move || {
        let entries = if trash_view_active {
            db.get_trashed_entries()?
        } else {
            db.get_entries_sorted(sort_mode)?
        };
        let normalized = query.trim().to_lowercase();
        let search_index = SearchIndex::open_default()?;
        let hits = if trash_view_active {
            Vec::new()
        } else {
            search_index.search(&query, 200).unwrap_or_default()
        };
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

fn refresh_entry_metadata(db: &Db, entry: &mut LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    let author = attributed_author(&doc);
    let page_count = Some(doc.page_count());
    db.update_author_attribution(&entry.id, author.as_deref())?;
    db.update_page_count_attribution(&entry.id, page_count)?;
    entry.author = author;
    entry.page_count = page_count;
    entry.author_attributed = true;
    entry.page_count_attributed = true;
    Ok(())
}

fn reindex_entry(search_index: &SearchIndex, entry: &LibraryEntry) -> anyhow::Result<()> {
    reindex_entries(search_index, std::slice::from_ref(entry))
}

fn reindex_entries(search_index: &SearchIndex, entries: &[LibraryEntry]) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }

    let mut documents = Vec::new();
    for entry in entries {
        documents.extend(index_documents_for_entry(entry)?);
    }
    search_index.replace_entries_pages(documents)?;
    Ok(())
}

fn reindex_entries_collecting_errors(
    search_index: &SearchIndex,
    entries: &[LibraryEntry],
) -> (usize, Vec<String>) {
    let mut documents = Vec::new();
    let mut updated = 0;
    let mut errors = Vec::new();

    for entry in entries {
        match index_documents_for_entry(entry) {
            Ok(entry_documents) => {
                documents.extend(entry_documents);
                updated += 1;
            }
            Err(error) => errors.push(format!("{}: {error}", entry_title(entry))),
        }
    }

    if !documents.is_empty() {
        if let Err(error) = search_index.replace_entries_pages(documents) {
            errors.push(format!("search index: {error}"));
            updated = 0;
        }
    }

    (updated, errors)
}

fn index_documents_for_entry(entry: &LibraryEntry) -> anyhow::Result<Vec<IndexDocument>> {
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
    Ok(documents)
}

pub(crate) fn import_pdf_with_index(db: &Db, path: PathBuf) -> anyhow::Result<ImportedEntry> {
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
