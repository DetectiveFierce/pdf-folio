//! # Library filter and search matching
//!
//! Pure predicates under `components::library::filters` for free-text field
//! matching, reading-progress buckets, and folder-scope visibility. Layout and
//! search tasks call these without holding iced or `Db` state.
//!
//! Title/author strings come from [`super::metadata`]; reading buckets pair
//! with [`super::state::LibraryReadingFilter`]. Structured query parsing and
//! ranking live in [`super::search`]; this module re-exports the matching API
//! and keeps folder/reading helpers.

use pdf_folio_core::{FolderId, LibraryEntry};

use super::state::LibraryReadingFilter;

#[allow(unused_imports)] // Re-exported for domain/tests callers via `library::filters`.
pub use super::search::{
    entry_matches_query, entry_matches_search_query, entry_search_fields_match,
    parse_library_search_query, score_entry_match, search_match_source_label,
    search_match_source_label_for_fields, search_match_source_label_for_query, LibrarySearchQuery,
    SearchField, SearchTerm,
};

/// Map an entry to a [`LibraryReadingFilter`] bucket from saved progress.
pub fn library_entry_reading_state(entry: &LibraryEntry) -> LibraryReadingFilter {
    library_reading_state(entry.last_page, entry.page_count)
}

/// Whether an entry appears under the selected folder (or unfiled when `None`).
pub fn entry_visible_in_folder_scope(
    entry: &LibraryEntry,
    selected_folder: Option<&FolderId>,
) -> bool {
    match selected_folder {
        Some(folder_id) => entry.folders.iter().any(|folder| &folder.id == folder_id),
        None => entry.folders.is_empty(),
    }
}

/// Classify progress from raw page fields without a full [`LibraryEntry`].
///
/// Finished when `last_page + 1 >= page_count`; reading when `last_page > 0`;
/// otherwise unread.
pub fn library_reading_state(last_page: u16, page_count: Option<u16>) -> LibraryReadingFilter {
    if page_count.is_some_and(|count| count > 0 && last_page.saturating_add(1) >= count) {
        LibraryReadingFilter::Finished
    } else if last_page > 0 {
        LibraryReadingFilter::Reading
    } else {
        LibraryReadingFilter::Unread
    }
}
