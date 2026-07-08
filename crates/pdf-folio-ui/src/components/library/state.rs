//! Library view state types.

/// Density of metadata shown in library cards and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryMetadataDensity {
    /// Show title and author with minimal supporting metadata.
    Minimal,
    /// Show common reading metadata.
    Standard,
    /// Show reading metadata plus file details.
    Detailed,
}

impl LibraryMetadataDensity {
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

/// Reading-progress filter applied to the library.
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
