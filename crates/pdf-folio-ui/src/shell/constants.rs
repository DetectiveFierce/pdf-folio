//! Stable widget ids, animation timings, and option lists shared by app views.

use pdf_folio_db::LibrarySortMode;

use crate::library::state::LibraryMetadataDensity;

pub(crate) const PAGE_INPUT_ID: &str = "viewer-toolbar-page-input";
pub(crate) const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
pub(crate) const VIEWER_SCROLLABLE_ID: &str = "viewer-scrollable";
pub(crate) const LIBRARY_SEARCH_INPUT_ID: &str = "library-search-input";
pub(crate) const LIBRARY_TAG_RENAME_INPUT_ID: &str = "library-tag-rename-input";
pub(crate) const LIBRARY_NAME_DIALOG_INPUT_ID: &str = "library-name-dialog-input";
pub(crate) const LIBRARY_CREATE_FOLDER_INPUT_ID: &str = "library-create-folder-input";
pub(crate) const VIEWER_FIND_INPUT_ID: &str = "viewer-find-input";
pub(crate) const LIBRARY_FOLDER_RENAME_INPUT_ID: &str = "library-folder-rename-input";
pub(crate) const LIBRARY_DETAILS_TITLE_INPUT_ID: &str = "library-details-title-input";

pub(crate) const LIBRARY_CARD_HOVER_TICK_MS: u64 = 16;
pub(crate) const LIBRARY_CARD_HOVER_DURATION_MS: u64 = 180;
pub(crate) const VIEWER_ANIMATION_TICK_MS: u64 = 16;

pub(crate) const LIBRARY_SORT_OPTIONS: [LibrarySortMode; 10] = [
    LibrarySortMode::Manual,
    LibrarySortMode::TitleAsc,
    LibrarySortMode::TitleDesc,
    LibrarySortMode::AuthorAsc,
    LibrarySortMode::AuthorDesc,
    LibrarySortMode::RecentlyAdded,
    LibrarySortMode::RecentlyOpened,
    LibrarySortMode::ReadingProgress,
    LibrarySortMode::PageCount,
    LibrarySortMode::MissingFiles,
];

pub(crate) const LIBRARY_METADATA_DENSITY_OPTIONS: [LibraryMetadataDensity; 3] = [
    LibraryMetadataDensity::Minimal,
    LibraryMetadataDensity::Standard,
    LibraryMetadataDensity::Detailed,
];
