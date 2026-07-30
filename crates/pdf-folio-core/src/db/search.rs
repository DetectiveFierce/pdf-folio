//! Tantivy full-text index over extracted PDF page text.
//!
//! [`SearchIndex`] is a separate on-disk store from SQLite (default path under
//! the XDG data directory as `search-index/`). Documents are page-granular:
//! each [`IndexDocument`] is one page of one library entry so hits can jump
//! the viewer to a page. Replacing an entry's pages deletes by `id` term then
//! re-adds in a single commit to keep the index consistent.
//!
//! Queries search title, author, and body fields via Tantivy's query parser;
//! empty queries return no hits. The index does not auto-update when the
//! library changes — callers (UI/import tasks) must upsert or delete after
//! text extraction.
//!
//! # See also
//!
//! - [`crate::pdf::PdfDoc::text_on_page`] for extracting page body text.
//! - [`crate::LibraryOrganizationSnapshot::search_changed_entry_ids`] for
//!   deciding which entries to reindex after organization undo.
//!
//! [`tantivy`]: https://docs.rs/tantivy

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use tantivy::collector::TopDocs;
use tantivy::indexer::NoMergePolicy;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, TantivyDocument, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, Term};

/// One page of library content to index (title/author repeated per page for scoring).
///
/// `id` should match the library [`crate::db::EntryId`] string. Replacing pages
/// for an entry deletes all prior docs with that `id` before inserting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDocument {
    /// Library entry identifier (Tantivy `STRING` field, exact match).
    pub id: String,
    /// PDF title (tokenized; used for ranking and display context).
    pub title: String,
    /// PDF author (tokenized).
    pub author: String,
    /// Extracted page body text (tokenized; primary full-text field).
    pub body: String,
    /// Zero-based page index stored with the hit for navigation.
    pub page: u64,
}

/// A single ranked search hit pointing at an entry page.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Library entry identifier matching [`IndexDocument::id`].
    pub id: String,
    /// Matching zero-based page index within that entry.
    pub page: u64,
    /// Tantivy relevance score (higher is better; not normalized across queries).
    pub score: f32,
}

/// Cached Tantivy field handles for the fixed PDF-Folio search schema.
#[derive(Debug, Clone, Copy)]
struct SearchFields {
    /// Exact-match entry id (`STRING`).
    id: Field,
    /// Tokenized title.
    title: Field,
    /// Tokenized author.
    author: Field,
    /// Tokenized page body.
    body: Field,
    /// Stored zero-based page index.
    page: Field,
}

/// Handle to the on-disk Tantivy index and its fixed PDF-Folio schema.
///
/// Cheap to clone (shares the underlying index). Writers use a 50 MB heap and
/// disable merges for interactive upsert latency; a full optimize is left to
/// maintenance paths outside this type.
#[derive(Debug, Clone)]
pub struct SearchIndex {
    schema: Schema,
    fields: SearchFields,
    index: Index,
    path: PathBuf,
}

impl SearchIndex {
    /// Builds the fixed schema: `id` (string), `title`/`author`/`body` (text), `page` (u64).
    pub fn new_schema() -> Schema {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("id", STRING | STORED);
        schema_builder.add_text_field("title", TEXT | STORED);
        schema_builder.add_text_field("author", TEXT | STORED);
        schema_builder.add_text_field("body", TEXT);
        schema_builder.add_u64_field("page", STORED);
        schema_builder.build()
    }

    /// Opens the default search index under the XDG data directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the index directory cannot be created or opened.
    pub fn open_default() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
            .context("Could not find a data directory for PDF-Folio search.")?;
        Self::open(project_dirs.data_dir().join("search-index"))
    }

    /// Opens or creates a search index at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error when the index directory cannot be created or opened.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Could not create search index: {}.", path.display()))?;
        let schema = Self::new_schema();
        let index = if has_tantivy_meta(&path) {
            Index::open_in_dir(&path)
        } else {
            Index::create_in_dir(&path, schema.clone())
        }
        .with_context(|| format!("Could not open search index: {}.", path.display()))?;

        let fields = SearchFields::from_schema(&schema)?;

        Ok(Self {
            schema,
            fields,
            index,
            path,
        })
    }

    /// Returns the Tantivy schema used when this index was opened/created.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Directory containing the on-disk Tantivy segment files.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Indexes one page, replacing any prior pages for the same entry id.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying search index cannot write the document.
    pub fn upsert_page(&self, document: IndexDocument) -> Result<()> {
        self.replace_entry_pages([document])
    }

    /// Replaces every indexed page for the entry ids present in `documents`.
    ///
    /// # Errors
    ///
    /// Returns an error when Tantivy cannot write or commit the replacement documents.
    pub fn replace_entry_pages(
        &self,
        documents: impl IntoIterator<Item = IndexDocument>,
    ) -> Result<()> {
        self.replace_entries_pages(documents)
    }

    /// Batch variant of [`Self::replace_entry_pages`]: one commit for many entries.
    ///
    /// Each distinct `document.id` is fully deleted from the index before its
    /// new page docs are added. Empty iterators are a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when Tantivy cannot write or commit the replacement documents.
    pub fn replace_entries_pages(
        &self,
        documents: impl IntoIterator<Item = IndexDocument>,
    ) -> Result<()> {
        let documents = documents.into_iter().collect::<Vec<_>>();
        if documents.is_empty() {
            return Ok(());
        }

        let mut writer = self.interactive_writer()?;
        let mut deleted_entry_ids = HashSet::new();
        for document in &documents {
            if deleted_entry_ids.insert(document.id.clone()) {
                writer.delete_term(Term::from_field_text(self.fields.id, &document.id));
            }
        }

        for document in documents {
            writer.add_document(doc!(
                self.fields.id => document.id,
                self.fields.title => document.title,
                self.fields.author => document.author,
                self.fields.body => document.body,
                self.fields.page => document.page,
            ))?;
        }

        writer.commit()?;
        Ok(())
    }

    /// Full-text search over title, author, and body; returns up to `limit` hits by score.
    ///
    /// Empty or whitespace-only queries return an empty vec without touching Tantivy.
    ///
    /// # Errors
    ///
    /// Returns an error when the query cannot be parsed or the searcher fails.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        reader.reload()?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.title, self.fields.author, self.fields.body],
        );
        let query = parser.parse_query(query)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).order_by_score())?;
        let mut hits = Vec::with_capacity(top_docs.len());

        for (score, address) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(address)?;
            let Some(id) = doc
                .get_first(self.fields.id)
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned)
            else {
                continue;
            };
            let page = doc
                .get_first(self.fields.page)
                .and_then(|value| value.as_u64())
                .unwrap_or_default();
            hits.push(SearchHit { id, page, score });
        }

        Ok(hits)
    }

    /// Deletes all pages for an entry from the index.
    ///
    /// # Errors
    ///
    /// Returns an error when Tantivy cannot commit the deletion.
    pub fn delete_entry(&self, entry_id: &str) -> Result<()> {
        self.delete_entries([entry_id])
    }

    /// Deletes all pages for multiple entries from the index.
    ///
    /// # Errors
    ///
    /// Returns an error when Tantivy cannot commit the deletion.
    pub fn delete_entries<'a>(&self, entry_ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let entry_ids = entry_ids.into_iter().collect::<Vec<_>>();
        if entry_ids.is_empty() {
            return Ok(());
        }

        let mut writer = self.interactive_writer()?;
        for entry_id in entry_ids {
            writer.delete_term(Term::from_field_text(self.fields.id, entry_id));
        }
        writer.commit()?;
        Ok(())
    }

    fn interactive_writer(&self) -> Result<IndexWriter<TantivyDocument>> {
        let writer = self.index.writer(50_000_000)?;
        writer.set_merge_policy(Box::new(NoMergePolicy));
        Ok(writer)
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::open_default().expect("Could not open default PDF-Folio search index")
    }
}

impl SearchFields {
    fn from_schema(schema: &Schema) -> Result<Self> {
        Ok(Self {
            id: schema.get_field("id")?,
            title: schema.get_field("title")?,
            author: schema.get_field("author")?,
            body: schema.get_field("body")?,
            page: schema.get_field("page")?,
        })
    }
}

/// True when `path` looks like an existing Tantivy index (meta file or lock present).
fn has_tantivy_meta(path: &Path) -> bool {
    path.join("meta.json").exists()
        || path.join(".tantivy-meta.lock").exists()
        || std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_string_lossy().contains("meta"))
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests;
