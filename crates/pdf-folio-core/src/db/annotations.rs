//! Text-annotation CRUD for library PDF entries.
//!
//! Annotations are text-anchored comments stored as library metadata (not in the
//! PDF). Each row ties an [`Annotation`] to an [`EntryId`] with character-range
//! anchors from the document text layer. Cascading foreign keys remove
//! annotations when their entry is hard-deleted.
//!
//! # See also
//!
//! - [`crate::Annotation`] / [`crate::AnnotationId`] for row shapes.
//! - Viewer UI loads annotations when a library document opens.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;

use super::{Annotation, AnnotationId, Db, EntryId};

impl Db {
    /// Returns all annotations for `entry_id`, ordered by document position.
    ///
    /// Sort order is `(start_page, start_char, created_at)` so the viewer sidebar
    /// can present a stable reading-order thread list without re-sorting.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot query annotations.
    pub fn list_annotations(&self, entry_id: &EntryId) -> Result<Vec<Annotation>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, entry_id, start_page, start_char, end_page, end_char,
                    quote, body, created_at, updated_at
             FROM annotations
             WHERE entry_id = ?1
             ORDER BY start_page ASC, start_char ASC, created_at ASC",
        )?;
        let rows = statement.query_map(params![entry_id.as_str()], row_to_annotation)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Could not load annotations.")
    }

    /// Inserts a new annotation row.
    ///
    /// Callers supply a fully-formed [`Annotation`] including id and timestamps.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot write the annotation (for example if
    /// `entry_id` does not exist or the id collides).
    pub fn insert_annotation(&self, annotation: &Annotation) -> Result<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO annotations
                    (id, entry_id, start_page, start_char, end_page, end_char,
                     quote, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    annotation.id.as_str(),
                    annotation.entry_id.as_str(),
                    i64::from(annotation.start_page),
                    annotation.start_char as i64,
                    i64::from(annotation.end_page),
                    annotation.end_char as i64,
                    annotation.quote,
                    annotation.body,
                    annotation.created_at.timestamp(),
                    annotation.updated_at.timestamp(),
                ],
            )
            .with_context(|| {
                format!(
                    "Could not insert annotation {} for entry {}.",
                    annotation.id.as_str(),
                    annotation.entry_id.as_str()
                )
            })?;
        Ok(())
    }

    /// Updates the body (and `updated_at`) of an existing annotation.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot update the row.
    pub fn update_annotation_body(
        &self,
        id: &AnnotationId,
        body: &str,
        updated_at: DateTime<Utc>,
    ) -> Result<()> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE annotations SET body = ?1, updated_at = ?2 WHERE id = ?3",
                params![body, updated_at.timestamp(), id.as_str()],
            )
            .with_context(|| format!("Could not update annotation {}.", id.as_str()))?;
        if changed == 0 {
            anyhow::bail!("Annotation {} does not exist.", id.as_str());
        }
        Ok(())
    }

    /// Deletes an annotation by id.
    ///
    /// # Errors
    ///
    /// Returns an error when SQLite cannot delete the row.
    pub fn delete_annotation(&self, id: &AnnotationId) -> Result<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM annotations WHERE id = ?1",
                params![id.as_str()],
            )
            .with_context(|| format!("Could not delete annotation {}.", id.as_str()))?;
        Ok(())
    }

    /// Allocates a unique annotation id string.
    ///
    /// Uses nanosecond timestamps plus a local counter suffix, matching the
    /// folder id generation style used elsewhere in this crate.
    pub fn new_annotation_id(&self) -> Result<AnnotationId> {
        let connection = self.connection()?;
        let suffix = next_annotation_suffix(&connection)?;
        Ok(AnnotationId::new(format!(
            "annotation-{}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            suffix
        )))
    }
}

fn next_annotation_suffix(connection: &rusqlite::Connection) -> Result<u64> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM annotations", [], |row| row.get(0))
        .unwrap_or(0);
    Ok((count as u64).saturating_add(1))
}

fn row_to_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    let created_at: i64 = row.get(8)?;
    let updated_at: i64 = row.get(9)?;
    Ok(Annotation {
        id: AnnotationId::new(row.get::<_, String>(0)?),
        entry_id: EntryId::new(row.get::<_, String>(1)?),
        start_page: row.get::<_, i64>(2)? as u16,
        start_char: row.get::<_, i64>(3)? as usize,
        end_page: row.get::<_, i64>(4)? as u16,
        end_char: row.get::<_, i64>(5)? as usize,
        quote: row.get(6)?,
        body: row.get(7)?,
        created_at: DateTime::from_timestamp(created_at, 0)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
        updated_at: DateTime::from_timestamp(updated_at, 0)
            .unwrap_or(DateTime::<Utc>::UNIX_EPOCH),
    })
}
