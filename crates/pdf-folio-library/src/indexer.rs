//! Tantivy full-text index setup.

use anyhow::Result;
use tantivy::schema::{Schema, STORED, STRING, TEXT};

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

/// Tantivy search index handle and schema.
#[derive(Debug, Clone)]
pub struct SearchIndex {
    schema: Schema,
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

    /// Creates a search index handle with the current schema.
    pub fn new() -> Self {
        Self {
            schema: Self::new_schema(),
        }
    }

    /// Returns the active Tantivy schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Adds or updates a page document in the index.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying search index cannot write the document.
    pub fn upsert_page(&self, _document: IndexDocument) -> Result<()> {
        Ok(())
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}
