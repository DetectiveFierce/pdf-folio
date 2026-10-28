//! # Library search query parsing and matching
//!
//! Pure helpers that turn free-text library queries into structured terms,
//! score metadata matches, and label hit sources. Full-text page hits come
//! from Tantivy in [`crate::library::tasks`]; this module covers title, author,
//! tag, folder, path, and filename matching plus field operators.
//!
//! ## Query syntax
//!
//! | Form | Meaning |
//! | --- | --- |
//! | `words` | All tokens must match somewhere (AND) |
//! | `"exact phrase"` | Contiguous substring match |
//! | `title:`, `author:`, `tag:`, `folder:`/`in:`, `path:`/`file:` | Field-scoped term |
//! | `text:`/`body:`/`content:` | Reserved for full-text body (metadata pass ignores) |
//! | `-term` / `-field:value` | Exclusion (must not match) |
//!
//! Field aliases are case-insensitive. Values may be quoted.

use pdf_folio_core::LibraryEntry;

use super::metadata::{entry_author, entry_title};

/// Which library field a search term targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchField {
    /// Match across title, author, tags, folders, path, and filename.
    Any,
    /// Document title / display title.
    Title,
    /// Document author / display author.
    Author,
    /// User tags.
    Tag,
    /// Folder names the entry belongs to.
    Folder,
    /// Full filesystem path (including filename).
    Path,
    /// Filename stem only.
    File,
    /// Full-text body (handled by Tantivy; metadata matcher skips positives).
    Text,
}

impl SearchField {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "title" | "t" | "name" => Some(Self::Title),
            "author" | "a" | "by" => Some(Self::Author),
            "tag" | "tags" | "#" => Some(Self::Tag),
            "folder" | "folders" | "in" | "dir" | "directory" => Some(Self::Folder),
            "path" | "p" => Some(Self::Path),
            "file" | "filename" | "fn" => Some(Self::File),
            "text" | "body" | "content" | "fulltext" | "ft" => Some(Self::Text),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Any => "library",
            Self::Title => "title",
            Self::Author => "author",
            Self::Tag => "tag",
            Self::Folder => "folder",
            Self::Path => "path",
            Self::File => "filename",
            Self::Text => "text",
        }
    }
}

/// One positive or negative clause in a parsed library query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchTerm {
    /// Field scope for this term.
    pub field: SearchField,
    /// Lowercased match text (phrase or token).
    pub value: String,
    /// When true, require a contiguous substring rather than loose token match.
    pub phrase: bool,
}

/// Structured library search query produced by [`parse_library_search_query`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySearchQuery {
    /// Original trimmed input (preserved for display).
    pub raw: String,
    /// Terms that must all match (AND).
    pub required: Vec<SearchTerm>,
    /// Terms that must not match.
    pub excluded: Vec<SearchTerm>,
}

impl LibrarySearchQuery {
    /// Whether the query has no matchable content.
    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.excluded.is_empty()
    }

    /// True when every required term is full-text-only (no metadata fields).
    pub fn is_text_only(&self) -> bool {
        !self.required.is_empty()
            && self
                .required
                .iter()
                .all(|term| term.field == SearchField::Text)
    }

    /// Values that should be sent to the full-text index.
    ///
    /// Includes unscoped terms and explicit `text:` terms. Field-scoped
    /// metadata operators are omitted so Tantivy is not polluted with tag names.
    pub fn full_text_query(&self) -> String {
        let mut parts = Vec::new();
        for term in &self.required {
            match term.field {
                SearchField::Any | SearchField::Text | SearchField::Title | SearchField::Author => {
                    if term.phrase || term.value.contains(char::is_whitespace) {
                        parts.push(format!("\"{}\"", term.value.replace('"', "")));
                    } else {
                        parts.push(term.value.clone());
                    }
                }
                SearchField::Tag
                | SearchField::Folder
                | SearchField::Path
                | SearchField::File => {}
            }
        }
        parts.join(" ")
    }

    /// Whether Tantivy should run for this query.
    pub fn wants_full_text(&self) -> bool {
        self.required.iter().any(|term| {
            matches!(
                term.field,
                SearchField::Any | SearchField::Text | SearchField::Title | SearchField::Author
            )
        }) || self.required.is_empty() && !self.excluded.is_empty()
    }
}

/// Parse a raw search box string into structured terms.
pub fn parse_library_search_query(raw: &str) -> LibrarySearchQuery {
    let raw_trimmed = raw.trim();
    let tokens = tokenize_search_input(raw_trimmed);
    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for token in tokens {
        let (negative, body) = if let Some(rest) = token.strip_prefix('-') {
            if rest.is_empty() {
                continue;
            }
            (true, rest)
        } else {
            (false, token.as_str())
        };

        let (field, value_raw, phrase) = split_field_term(body);
        let value = value_raw.trim().to_lowercase();
        if value.is_empty() {
            continue;
        }

        let term = SearchTerm {
            field,
            value,
            phrase,
        };
        if negative {
            excluded.push(term);
        } else {
            required.push(term);
        }
    }

    LibrarySearchQuery {
        raw: raw_trimmed.to_owned(),
        required,
        excluded,
    }
}

/// Whether an entry matches the structured query (metadata fields only).
///
/// Full-text-only required terms are treated as vacuously true here so the
/// caller can union Tantivy hits. Exclusions against `Text` are ignored in the
/// metadata pass.
pub fn entry_matches_search_query(entry: &LibraryEntry, query: &LibrarySearchQuery) -> bool {
    if query.is_empty() {
        return true;
    }

    let fields = EntrySearchFields::from_entry(entry);
    entry_fields_match_query(&fields, query)
}

/// Whether an entry matches a free-text library query (parses then matches).
pub fn entry_matches_query(entry: &LibraryEntry, raw_query: &str) -> bool {
    let query = parse_library_search_query(raw_query);
    if query.is_empty() {
        return true;
    }
    // Text-only queries cannot be confirmed from metadata alone.
    if query.is_text_only() {
        return false;
    }
    entry_matches_search_query(entry, &query)
}

/// Relevance score for ranking metadata hits (higher is better). `None` if no match.
pub fn score_entry_match(entry: &LibraryEntry, query: &LibrarySearchQuery) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    if query.is_text_only() {
        return None;
    }

    let fields = EntrySearchFields::from_entry(entry);
    if !entry_fields_match_query(&fields, query) {
        return None;
    }

    let mut score = 0i32;
    for term in &query.required {
        if term.field == SearchField::Text {
            continue;
        }
        score += score_term_against_fields(&fields, term);
    }

    // Contiguous full-query bonus when the raw string appears in the title.
    let raw_lower = query.raw.to_lowercase();
    if !raw_lower.is_empty() && fields.title.contains(&raw_lower) {
        score += 40;
    }
    if !raw_lower.is_empty() && fields.title == raw_lower {
        score += 80;
    }

    Some(score)
}

/// Human-readable label describing why an entry matched.
pub fn search_match_source_label(entry: &LibraryEntry, raw_query: &str) -> Option<String> {
    let query = parse_library_search_query(raw_query);
    search_match_source_label_for_query(entry, &query)
}

/// Label describing which field matched a structured query.
pub fn search_match_source_label_for_query(
    entry: &LibraryEntry,
    query: &LibrarySearchQuery,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    let fields = EntrySearchFields::from_entry(entry);
    let mut labels = Vec::new();

    for term in &query.required {
        if term.field == SearchField::Text {
            continue;
        }
        if let Some(label) = match_label_for_term(&fields, term) {
            if !labels.iter().any(|existing| existing == &label) {
                labels.push(label);
            }
        }
    }

    if labels.is_empty() {
        // Fallback: describe first matching any-field for legacy single-substring queries.
        if let Some(term) = query.required.first() {
            return match_label_for_term(&fields, term);
        }
        return None;
    }

    if labels.len() == 1 {
        Some(labels.remove(0))
    } else {
        Some(format!("Match in {}", labels.join(" · ")))
    }
}

/// Label describing which field matched, using explicit field strings (tests / callers).
pub fn search_match_source_label_for_fields<'a>(
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

    let fields = EntrySearchFields {
        title: title.to_lowercase(),
        author: author.to_lowercase(),
        path: path.to_lowercase(),
        file_name: file_name_from_path(path).to_lowercase(),
        tags: tags.into_iter().map(|tag| tag.to_lowercase()).collect(),
        folder_names: folder_names
            .into_iter()
            .map(|folder| folder.to_lowercase())
            .collect(),
    };

    let query = parse_library_search_query(normalized_query);
    if query.is_empty() {
        return None;
    }

    for term in &query.required {
        if let Some(label) = match_label_for_term(&fields, term) {
            return Some(format_single_match_label(&label));
        }
    }

    // Legacy single-substring priority order when parse yields one Any term.
    let needle = normalized_query.to_lowercase();
    if fields.title.contains(&needle) {
        Some(String::from("Match in title"))
    } else if fields.author.contains(&needle) {
        Some(String::from("Match in author"))
    } else if fields.tags.iter().any(|tag| tag.contains(&needle)) {
        Some(String::from("Match in tag"))
    } else if fields
        .folder_names
        .iter()
        .any(|folder| folder.contains(&needle))
    {
        Some(String::from("Match in folder"))
    } else if fields.path.contains(&needle) {
        Some(String::from("Match in path"))
    } else {
        None
    }
}

/// Whether any of the provided fields contains the normalized query substring.
///
/// Prefer [`entry_matches_query`] / [`entry_matches_search_query`] for new code.
pub fn entry_search_fields_match<'a>(
    title: &str,
    author: &str,
    path: &str,
    tags: impl IntoIterator<Item = &'a str>,
    folder_names: impl IntoIterator<Item = &'a str>,
    normalized_query: &str,
) -> bool {
    if normalized_query.is_empty() {
        return true;
    }

    let fields = EntrySearchFields {
        title: title.to_lowercase(),
        author: author.to_lowercase(),
        path: path.to_lowercase(),
        file_name: file_name_from_path(path).to_lowercase(),
        tags: tags.into_iter().map(|tag| tag.to_lowercase()).collect(),
        folder_names: folder_names
            .into_iter()
            .map(|folder| folder.to_lowercase())
            .collect(),
    };
    let query = parse_library_search_query(normalized_query);
    entry_fields_match_query(&fields, &query)
}

#[derive(Debug, Clone)]
struct EntrySearchFields {
    title: String,
    author: String,
    path: String,
    file_name: String,
    tags: Vec<String>,
    folder_names: Vec<String>,
}

impl EntrySearchFields {
    fn from_entry(entry: &LibraryEntry) -> Self {
        let path = entry.path.to_string_lossy();
        Self {
            title: entry_title(entry).to_lowercase(),
            author: entry_author(entry).to_lowercase(),
            path: path.to_lowercase(),
            file_name: file_name_from_path(&path).to_lowercase(),
            tags: entry.tags.iter().map(|tag| tag.to_lowercase()).collect(),
            folder_names: entry
                .folders
                .iter()
                .map(|folder| folder.name.to_lowercase())
                .collect(),
        }
    }
}

fn entry_fields_match_query(fields: &EntrySearchFields, query: &LibrarySearchQuery) -> bool {
    if query.is_empty() {
        return true;
    }

    for term in &query.excluded {
        if term.field == SearchField::Text {
            continue;
        }
        if term_matches_fields(fields, term) {
            return false;
        }
    }

    let metadata_required = query
        .required
        .iter()
        .filter(|term| term.field != SearchField::Text)
        .collect::<Vec<_>>();

    if metadata_required.is_empty() {
        // Only text terms (or only exclusions that passed): metadata pass accepts;
        // caller unions with FTS for positive text confirmation.
        return !query.is_text_only();
    }

    metadata_required
        .into_iter()
        .all(|term| term_matches_fields(fields, term))
}

fn term_matches_fields(fields: &EntrySearchFields, term: &SearchTerm) -> bool {
    match term.field {
        SearchField::Any => {
            value_matches(&fields.title, &term.value, term.phrase)
                || value_matches(&fields.author, &term.value, term.phrase)
                || fields
                    .tags
                    .iter()
                    .any(|tag| value_matches(tag, &term.value, term.phrase))
                || fields
                    .folder_names
                    .iter()
                    .any(|folder| value_matches(folder, &term.value, term.phrase))
                || value_matches(&fields.path, &term.value, term.phrase)
                || value_matches(&fields.file_name, &term.value, term.phrase)
        }
        SearchField::Title => value_matches(&fields.title, &term.value, term.phrase),
        SearchField::Author => value_matches(&fields.author, &term.value, term.phrase),
        SearchField::Tag => fields
            .tags
            .iter()
            .any(|tag| value_matches(tag, &term.value, term.phrase)),
        SearchField::Folder => fields
            .folder_names
            .iter()
            .any(|folder| value_matches(folder, &term.value, term.phrase)),
        SearchField::Path => value_matches(&fields.path, &term.value, term.phrase),
        SearchField::File => value_matches(&fields.file_name, &term.value, term.phrase),
        SearchField::Text => false,
    }
}

fn value_matches(haystack: &str, needle: &str, phrase: bool) -> bool {
    if needle.is_empty() {
        return true;
    }
    if phrase || needle.contains(char::is_whitespace) {
        return haystack.contains(needle);
    }
    // Token match: substring OR any whitespace-split token prefix.
    if haystack.contains(needle) {
        return true;
    }
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .any(|part| part.starts_with(needle))
}

fn score_term_against_fields(fields: &EntrySearchFields, term: &SearchTerm) -> i32 {
    let mut best = 0i32;

    let consider = |current: &mut i32, haystack: &str, base: i32| {
        if !value_matches(haystack, &term.value, term.phrase) {
            return;
        }
        let mut score = base;
        if haystack == term.value {
            score += 50;
        } else if haystack.starts_with(&term.value) {
            score += 25;
        } else if haystack
            .split(|c: char| !c.is_alphanumeric())
            .any(|part| part == term.value)
        {
            score += 15;
        }
        *current = (*current).max(score);
    };

    match term.field {
        SearchField::Any => {
            consider(&mut best, &fields.title, 100);
            consider(&mut best, &fields.author, 80);
            for tag in &fields.tags {
                consider(&mut best, tag, 70);
            }
            for folder in &fields.folder_names {
                consider(&mut best, folder, 55);
            }
            consider(&mut best, &fields.file_name, 50);
            consider(&mut best, &fields.path, 30);
        }
        SearchField::Title => consider(&mut best, &fields.title, 100),
        SearchField::Author => consider(&mut best, &fields.author, 80),
        SearchField::Tag => {
            for tag in &fields.tags {
                consider(&mut best, tag, 70);
            }
        }
        SearchField::Folder => {
            for folder in &fields.folder_names {
                consider(&mut best, folder, 55);
            }
        }
        SearchField::Path => consider(&mut best, &fields.path, 30),
        SearchField::File => consider(&mut best, &fields.file_name, 50),
        SearchField::Text => {}
    }

    best
}

fn match_label_for_term(fields: &EntrySearchFields, term: &SearchTerm) -> Option<String> {
    match term.field {
        SearchField::Any => {
            if value_matches(&fields.title, &term.value, term.phrase) {
                Some(String::from("title"))
            } else if value_matches(&fields.author, &term.value, term.phrase) {
                Some(String::from("author"))
            } else if fields
                .tags
                .iter()
                .any(|tag| value_matches(tag, &term.value, term.phrase))
            {
                Some(String::from("tag"))
            } else if fields
                .folder_names
                .iter()
                .any(|folder| value_matches(folder, &term.value, term.phrase))
            {
                Some(String::from("folder"))
            } else if value_matches(&fields.file_name, &term.value, term.phrase) {
                Some(String::from("filename"))
            } else if value_matches(&fields.path, &term.value, term.phrase) {
                Some(String::from("path"))
            } else {
                None
            }
        }
        SearchField::Text => None,
        other => {
            if term_matches_fields(fields, term) {
                Some(other.label().to_owned())
            } else {
                None
            }
        }
    }
}

fn format_single_match_label(field: &str) -> String {
    match field {
        "title" => String::from("Match in title"),
        "author" => String::from("Match in author"),
        "tag" => String::from("Match in tag"),
        "folder" => String::from("Match in folder"),
        "filename" => String::from("Match in filename"),
        "path" => String::from("Match in path"),
        other => format!("Match in {other}"),
    }
}

fn file_name_from_path(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// Split on whitespace; keep `"quoted phrases"` intact (quotes stripped).
fn tokenize_search_input(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                if !in_quotes && !current.is_empty() {
                    // End of quote — flush the accumulated field:value or phrase token.
                    tokens.push(std::mem::take(&mut current));
                }
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    // Re-attach field: prefix when user wrote field:"phrase" — the quote handler
    // may have split oddly. Also detect phrase tokens that came from quotes by
    // checking the original input positions is hard; instead, treat multi-word
    // tokens (only possible via quotes) as phrases in split_field_term.
    tokens
}

fn split_field_term(token: &str) -> (SearchField, String, bool) {
    // Support field:value and field:"already unquoted value"
    if let Some((field_raw, rest)) = token.split_once(':') {
        if let Some(field) = SearchField::parse(field_raw) {
            let value = rest.trim().trim_matches('"').to_owned();
            let phrase = value.contains(char::is_whitespace)
                || rest.starts_with('"')
                || (rest.starts_with('"') && rest.ends_with('"'));
            // Multi-word values from quote tokenization are phrases.
            let phrase = phrase || value.contains(' ');
            return (field, value, phrase);
        }
    }

    let value = token.trim().trim_matches('"').to_owned();
    let phrase = value.contains(char::is_whitespace);
    (SearchField::Any, value, phrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_field_operators_and_exclusions() {
        let query = parse_library_search_query(r#"title:"linear algebra" tag:math -author:bob"#);
        assert_eq!(query.required.len(), 2);
        assert_eq!(query.required[0].field, SearchField::Title);
        assert_eq!(query.required[0].value, "linear algebra");
        assert!(query.required[0].phrase);
        assert_eq!(query.required[1].field, SearchField::Tag);
        assert_eq!(query.required[1].value, "math");
        assert_eq!(query.excluded.len(), 1);
        assert_eq!(query.excluded[0].field, SearchField::Author);
        assert_eq!(query.excluded[0].value, "bob");
    }

    #[test]
    fn and_matches_require_all_tokens() {
        let fields_ok = entry_search_fields_match(
            "Linear Algebra Notes",
            "Strang",
            "/tmp/algebra.pdf",
            ["math"].into_iter(),
            ["Courses"].into_iter(),
            "linear strang",
        );
        assert!(fields_ok);

        let fields_bad = entry_search_fields_match(
            "Linear Algebra Notes",
            "Strang",
            "/tmp/algebra.pdf",
            ["math"].into_iter(),
            ["Courses"].into_iter(),
            "linear calculus",
        );
        assert!(!fields_bad);
    }

    #[test]
    fn tag_operator_matches_only_tags() {
        assert!(entry_search_fields_match(
            "Quarterly Report",
            "Analyst",
            "/tmp/report.pdf",
            ["finance"].into_iter(),
            ["Archive"].into_iter(),
            "tag:finance",
        ));
        assert!(!entry_search_fields_match(
            "finance primer",
            "Analyst",
            "/tmp/report.pdf",
            ["other"].into_iter(),
            ["Archive"].into_iter(),
            "tag:finance",
        ));
    }

    #[test]
    fn full_text_query_skips_metadata_operators() {
        let query = parse_library_search_query("rings tag:math author:ada");
        assert_eq!(query.full_text_query(), "rings ada");
    }
}
