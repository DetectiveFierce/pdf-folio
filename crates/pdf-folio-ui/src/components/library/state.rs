//! # Library presentation enums
//!
//! Shared enums under `components::library::state` for metadata density and
//! reading-progress filters. Used by pure helpers ([`super::metadata`],
//! [`super::filters`]), toolbar pickers ([`super::view`]), and domain library
//! state that persists user preferences.
//!
//! Kept free of iced and `Db` dependencies so the same types can be
//! re-exported into the library domain without pulling presentation crates.
//! Density maps to concrete field lists for settings serialization; reading
//! filters pair with progress classification in [`super::filters`].

/// How much secondary metadata is shown under library card/row titles.
///
/// Controls the strings built by [`super::metadata::library_card_metadata_label`]
/// and [`super::metadata::library_row_metadata_label`], and the toolbar density
/// picker in [`super::view`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMetadataDensity {
    /// Title plus author only (no page count or size line on cards).
    Minimal,
    /// Author and page count (standard library density).
    Standard,
    /// Author, page count, and file size for denser browsing.
    Detailed,
}

impl LibraryMetadataDensity {
    /// Map a persisted settings field list to the matching density preset.
    ///
    /// Presence of `"file_size"` → [`Self::Detailed`]; `"page_count"` without
    /// size → [`Self::Standard`]; otherwise [`Self::Minimal`].
    pub fn from_visible_fields(fields: &[String]) -> Self {
        let has_file_size = fields.iter().any(|field| field == "file_size");
        let has_page_count = fields.iter().any(|field| field == "page_count");
        if has_file_size {
            Self::Detailed
        } else if has_page_count {
            Self::Standard
        } else {
            Self::Minimal
        }
    }

    /// Field keys this density shows, for settings write-back and UI toggles.
    ///
    /// Always includes `"author"`; standard adds `"page_count"`, detailed also
    /// adds `"file_size"`.
    pub fn visible_fields(self) -> Vec<String> {
        match self {
            Self::Minimal => vec![String::from("author")],
            Self::Standard => vec![String::from("author"), String::from("page_count")],
            Self::Detailed => vec![
                String::from("author"),
                String::from("page_count"),
                String::from("file_size"),
            ],
        }
    }

    /// Short label for the density pick list (`"Minimal"`, `"Standard"`, …).
    pub fn label(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Standard => "Standard",
            Self::Detailed => "Detailed",
        }
    }
}

impl std::fmt::Display for LibraryMetadataDensity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Reading-progress bucket used to filter the library entry list.
///
/// Classification of a concrete entry is performed by
/// [`super::filters::library_entry_reading_state`] using saved `last_page` / page count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryReadingFilter {
    /// Entries with no saved progress.
    Unread,
    /// Entries with saved progress before the final known page.
    Reading,
    /// Entries whose saved progress reaches the final known page.
    Finished,
}

impl LibraryReadingFilter {
    /// Short label for filter chips and menu items.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unread => "Unread",
            Self::Reading => "Reading",
            Self::Finished => "Finished",
        }
    }
}

impl std::fmt::Display for LibraryReadingFilter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}
