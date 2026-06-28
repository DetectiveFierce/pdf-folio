//! Tantivy full-text index setup.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, TantivyDocument, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, Term};

/// A page-level document to add to the search index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDocument {
    /// Library entry identifier.
    pub id: String,
    /// PDF title.
    pub title: String,
    /// PDF author.
    pub author: String,
    /// Page body text.
    pub body: String,
    /// Zero-based page index.
    pub page: u64,
}

/// A search hit returned from Tantivy.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Library entry identifier.
    pub id: String,
    /// Matching zero-based page index.
    pub page: u64,
    /// Tantivy relevance score.
    pub score: f32,
}

#[derive(Debug, Clone, Copy)]
struct SearchFields {
    id: Field,
    title: Field,
    author: Field,
    body: Field,
    page: Field,
}

/// Tantivy search index handle and schema.
#[derive(Debug, Clone)]
pub struct SearchIndex {
    schema: Schema,
    fields: SearchFields,
    index: Index,
    path: PathBuf,
}

impl SearchIndex {
    /// Creates the PDF-Folio Tantivy schema.
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

    /// Returns the active Tantivy schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Returns the index path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Adds or updates a single page document in the index.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying search index cannot write the document.
    pub fn upsert_page(&self, document: IndexDocument) -> Result<()> {
        self.replace_entry_pages([document])
    }

    /// Replaces all indexed pages for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when Tantivy cannot write or commit the replacement documents.
    pub fn replace_entry_pages(
        &self,
        documents: impl IntoIterator<Item = IndexDocument>,
    ) -> Result<()> {
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        let mut documents = documents.into_iter().peekable();
        let Some(first) = documents.peek() else {
            return Ok(());
        };
        let entry_id = first.id.clone();
        writer.delete_term(Term::from_field_text(self.fields.id, &entry_id));

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

    /// Searches the full-text index.
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
        let mut writer: IndexWriter<TantivyDocument> = self.index.writer(50_000_000)?;
        writer.delete_term(Term::from_field_text(self.fields.id, entry_id));
        writer.commit()?;
        Ok(())
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
mod tests {
    use super::*;

    #[test]
    fn indexes_and_searches_page_documents() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("pdf-folio-search-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let index = SearchIndex::open(&root)?;

        index.replace_entry_pages([
            IndexDocument {
                id: String::from("entry-a"),
                title: String::from("Algebra Notes"),
                author: String::from("Ada"),
                body: String::from("rings fields and groups"),
                page: 0,
            },
            IndexDocument {
                id: String::from("entry-a"),
                title: String::from("Algebra Notes"),
                author: String::from("Ada"),
                body: String::from("linear transformations"),
                page: 1,
            },
        ])?;

        let hits = index.search("linear", 10)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "entry-a");
        assert_eq!(hits[0].page, 1);

        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
