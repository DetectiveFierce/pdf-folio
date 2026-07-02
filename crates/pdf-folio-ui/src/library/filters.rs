//! Library filtering and search matching helpers.

use pdf_folio_library::{FolderId, LibraryEntry};

use super::metadata::{entry_author, entry_title};
use super::state::LibraryReadingFilter;

pub(crate) fn entry_matches_query(entry: &LibraryEntry, normalized_query: &str) -> bool {
    entry_search_fields_match(
        &entry_title(entry),
        &entry_author(entry),
        &entry.path.to_string_lossy(),
        entry.tags.iter().map(String::as_str),
        entry.folders.iter().map(|folder| folder.name.as_str()),
        normalized_query,
    )
}

pub(crate) fn search_match_source_label(
    entry: &LibraryEntry,
    normalized_query: &str,
) -> Option<String> {
    search_match_source_label_for_fields(
        &entry_title(entry),
        &entry_author(entry),
        &entry.path.to_string_lossy(),
        entry.tags.iter().map(String::as_str),
        entry.folders.iter().map(|folder| folder.name.as_str()),
        normalized_query,
    )
}

pub(crate) fn search_match_source_label_for_fields<'a>(
    title: &str,
    author: &str,
    path: &str,
    tags: impl IntoIterator<Item = &'a str>,
    folder_names: impl IntoIterator<Item = &'a str>,
    normalized_query: &str,
) -> Option<String> {
    if normalized_query.is_empty() {
        return None;
    }

    if title.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in title"))
    } else if author.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in author"))
    } else if tags
        .into_iter()
        .any(|tag| tag.to_lowercase().contains(normalized_query))
    {
        Some(String::from("Match in tag"))
    } else if folder_names
        .into_iter()
        .any(|folder| folder.to_lowercase().contains(normalized_query))
    {
        Some(String::from("Match in folder"))
    } else if path.to_lowercase().contains(normalized_query) {
        Some(String::from("Match in path"))
    } else {
        None
    }
}

pub(crate) fn entry_search_fields_match<'a>(
    title: &str,
    author: &str,
    path: &str,
    tags: impl IntoIterator<Item = &'a str>,
    folder_names: impl IntoIterator<Item = &'a str>,
    normalized_query: &str,
) -> bool {
    title.to_lowercase().contains(normalized_query)
        || author.to_lowercase().contains(normalized_query)
        || path.to_lowercase().contains(normalized_query)
        || tags
            .into_iter()
            .any(|tag| tag.to_lowercase().contains(normalized_query))
        || folder_names
            .into_iter()
            .any(|folder| folder.to_lowercase().contains(normalized_query))
}

pub(crate) fn library_entry_reading_state(entry: &LibraryEntry) -> LibraryReadingFilter {
    library_reading_state(entry.last_page, entry.page_count)
}

pub(crate) fn entry_visible_in_folder_scope(
    entry: &LibraryEntry,
    selected_folder: Option<&FolderId>,
) -> bool {
    match selected_folder {
        Some(folder_id) => entry.folders.iter().any(|folder| &folder.id == folder_id),
        None => entry.folders.is_empty(),
    }
}

pub(crate) fn library_reading_state(
    last_page: u16,
    page_count: Option<u16>,
) -> LibraryReadingFilter {
    if page_count.is_some_and(|count| count > 0 && last_page.saturating_add(1) >= count) {
        LibraryReadingFilter::Finished
    } else if last_page > 0 {
        LibraryReadingFilter::Reading
    } else {
        LibraryReadingFilter::Unread
    }
}
