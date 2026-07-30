//! Stable widget ids, animation timings, and option lists shared by app views.
//!
//! iced focuses and scrolls by string widget ids; this module is the single
//! place those ids and a few shared picker option arrays live so library and
//! viewer chrome stay in sync.
//!
//! # Contents
//!
//! - `*_ID` string constants for focus/scroll targets (search, rename, zoom,
//!   find, library/viewer scrollables).
//! - Animation tick/duration constants for card hover and viewer fades.
//! - Ordered option arrays for sort mode and metadata density pickers.
//!
//! Related: viewer zoom input id lives in
//! [`crate::viewer::rendering::ZOOM_INPUT_ID`] next to zoom math.

use pdf_folio_core::LibrarySortMode;

use crate::library::state::LibraryMetadataDensity;

/// iced widget id for the viewer toolbar page-number input.
pub(crate) const PAGE_INPUT_ID: &str = "viewer-toolbar-page-input";
/// iced widget id for the library main scrollable.
pub(crate) const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";
/// iced widget id for the viewer document scrollable.
pub(crate) const VIEWER_SCROLLABLE_ID: &str = "viewer-scrollable";
/// iced widget id for the library search field.
pub(crate) const LIBRARY_SEARCH_INPUT_ID: &str = "library-search-input";
/// iced widget id for inline sidebar tag rename.
pub(crate) const LIBRARY_TAG_RENAME_INPUT_ID: &str = "library-tag-rename-input";
/// iced widget id for create/rename library dialog input.
pub(crate) const LIBRARY_NAME_DIALOG_INPUT_ID: &str = "library-name-dialog-input";
/// iced widget id for the new-folder dialog name field.
pub(crate) const LIBRARY_CREATE_FOLDER_INPUT_ID: &str = "library-create-folder-input";
/// iced widget id for the viewer find-in-document query field.
pub(crate) const VIEWER_FIND_INPUT_ID: &str = "viewer-find-input";
/// iced widget id for selected-folder rename.
pub(crate) const LIBRARY_FOLDER_RENAME_INPUT_ID: &str = "library-folder-rename-input";
/// iced widget id for the details panel title override.
pub(crate) const LIBRARY_DETAILS_TITLE_INPUT_ID: &str = "library-details-title-input";

/// Animation frame interval for library card hover tweens (milliseconds).
pub(crate) const LIBRARY_CARD_HOVER_TICK_MS: u64 = 16;
/// Hover tween duration for library cards (milliseconds).
pub(crate) const LIBRARY_CARD_HOVER_DURATION_MS: u64 = 180;
/// Animation frame interval for viewer page-fade and related tweens (ms).
pub(crate) const VIEWER_ANIMATION_TICK_MS: u64 = 16;

/// Ordered library sort modes shown in the sort picker menu.
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

/// Ordered metadata density options for library cards and rows.
pub(crate) const LIBRARY_METADATA_DENSITY_OPTIONS: [LibraryMetadataDensity; 3] = [
    LibraryMetadataDensity::Minimal,
    LibraryMetadataDensity::Standard,
    LibraryMetadataDensity::Detailed,
];
