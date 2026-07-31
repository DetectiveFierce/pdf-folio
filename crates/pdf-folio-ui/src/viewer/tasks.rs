//! Async task constructors for viewer document loading and rendering.
//!
//! Keeps blocking PDF open/render work off the iced UI thread. Completions
//! return through the crate [`Message`] bus (`DocumentOpened`,
//! `LibraryDocumentOpened`, `DocumentTitleLoaded`, `AnnotationsLoaded`,
//! `PageRendered`, `ZoomRenderSettled`, etc.).
//!
//! Title and annotation loads follow the same pattern: bump a generation,
//! `spawn_blocking` on a worker, emit a message that the update arm discards
//! when the generation no longer matches the open document.
//!
//! Related: [`super::update`] handles those messages, [`super::navigation`]
//! schedules zoom debounce, library update calls open tasks when a card is
//! activated.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::Task;
use pdf_folio_core::{Annotation, AnnotationId, EntryId};
use pdf_folio_core::{PdfDoc, RenderedPage, TileKey};

use crate::messages::Message;
use crate::PDFolioApp;

/// Renders one page at `key.width_px` on a blocking thread pool worker.
pub(crate) async fn render_page(
    doc: Arc<PdfDoc>,
    key: TileKey,
) -> anyhow::Result<(TileKey, RenderedPage)> {
    let page =
        tokio::task::spawn_blocking(move || doc.render_page(key.page, key.width_px)).await??;
    Ok((key, page))
}

/// Opens a PDF from an arbitrary filesystem path (file dialog / CLI).
///
/// Emits [`Message::DocumentOpened`] or [`Message::DocumentError`].
pub(crate) fn open_document_task(path: PathBuf) -> Task<Message> {
    Task::perform(
        async move {
            let doc_path = path.clone();
            let doc = tokio::task::spawn_blocking(move || PdfDoc::open(&path)).await??;
            Ok::<_, anyhow::Error>((doc_path, doc))
        },
        |result| match result {
            Ok((path, doc)) => Message::DocumentOpened {
                path,
                doc: Arc::new(doc),
            },
            Err(error) => Message::DocumentError(error.to_string()),
        },
    )
}

/// Opens a library entry's PDF, tagging the result with `entry_id`.
///
/// Emits [`Message::LibraryDocumentOpened`] or [`Message::DocumentError`].
pub(crate) fn open_library_document_task(entry_id: EntryId, path: PathBuf) -> Task<Message> {
    Task::perform(
        async move { tokio::task::spawn_blocking(move || PdfDoc::open(&path)).await? },
        move |result| match result {
            Ok(doc) => Message::LibraryDocumentOpened {
                entry_id: entry_id.clone(),
                doc: Arc::new(doc),
            },
            Err(error) => Message::DocumentError(error.to_string()),
        },
    )
}

/// Records “last opened” for the current library entry after a successful open.
///
/// No-op when the document was not opened from a library entry. Emits
/// [`Message::ProgressSaved`] or [`Message::LibraryError`].
pub(crate) fn mark_entry_opened_task(app: &PDFolioApp) -> Task<Message> {
    let Some(entry_id) = app.viewer.current_entry_id.clone() else {
        return Task::none();
    };
    let db = Arc::clone(&app.db);
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.mark_entry_opened(&entry_id)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::ProgressSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

/// Waits 140 ms then emits [`Message::ZoomRenderSettled`] for `generation`.
///
/// Stale generations are ignored when a newer zoom superseded the gesture.
pub(crate) fn schedule_zoom_render(generation: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(140)).await;
            generation
        },
        Message::ZoomRenderSettled,
    )
}

/// Loads the PDF document title metadata on a blocking worker.
///
/// Emits [`Message::DocumentTitleLoaded`] with the trimmed title (or `None`).
/// Stale generations are ignored when a newer document supersedes the load.
pub(crate) fn load_document_title_task(app: &mut PDFolioApp) -> Task<Message> {
    let Some(doc) = app.viewer.doc.clone() else {
        return Task::none();
    };
    let generation = app.viewer.document_title_load_generation.wrapping_add(1);
    app.viewer.document_title_load_generation = generation;
    Task::perform(
        async move {
            match tokio::task::spawn_blocking(move || doc.metadata_title()).await {
                Ok(Ok(title)) => title,
                Ok(Err(_)) | Err(_) => None,
            }
        },
        move |title| Message::DocumentTitleLoaded { title, generation },
    )
}

/// Loads annotations for the open library entry.
///
/// Emits [`Message::AnnotationsLoaded`] (possibly empty) or a library error.
/// No-op when the document was not opened from the library.
pub(crate) fn load_annotations_task(app: &mut PDFolioApp) -> Task<Message> {
    let Some(entry_id) = app.viewer.current_entry_id.clone() else {
        return Task::none();
    };
    let generation = app.viewer.annotations_load_generation.wrapping_add(1);
    app.viewer.annotations_load_generation = generation;
    let db = Arc::clone(&app.db);
    let load_entry_id = entry_id.clone();
    Task::perform(
        async move {
            let annotations = tokio::task::spawn_blocking(move || {
                db.list_annotations(&load_entry_id)
            })
            .await??;
            Ok::<_, anyhow::Error>((entry_id, annotations, generation))
        },
        |result| match result {
            Ok((entry_id, annotations, generation)) => Message::AnnotationsLoaded {
                entry_id,
                annotations,
                generation,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

/// Inserts a new annotation on a blocking worker.
pub(crate) fn insert_annotation_task(app: &PDFolioApp, annotation: Annotation) -> Task<Message> {
    let db = Arc::clone(&app.db);
    Task::perform(
        async move {
            let db = db;
            let annotation = annotation;
            tokio::task::spawn_blocking(move || {
                db.insert_annotation(&annotation)?;
                Ok::<_, anyhow::Error>(annotation)
            })
            .await?
        },
        |result| match result {
            Ok(annotation) => Message::AnnotationCreateFinished(Ok(annotation)),
            Err(error) => Message::AnnotationCreateFinished(Err(error.to_string())),
        },
    )
}

/// Updates an annotation body on a blocking worker.
pub(crate) fn update_annotation_body_task(
    app: &PDFolioApp,
    id: AnnotationId,
    body: String,
) -> Task<Message> {
    let db = Arc::clone(&app.db);
    let updated_at = chrono::Utc::now();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.update_annotation_body(&id, &body, updated_at)?;
                Ok::<_, anyhow::Error>((id, body, updated_at))
            })
            .await?
        },
        |result| match result {
            Ok((id, body, updated_at)) => Message::AnnotationEditFinished(Ok((id, body, updated_at))),
            Err(error) => Message::AnnotationEditFinished(Err(error.to_string())),
        },
    )
}

/// Deletes an annotation on a blocking worker.
pub(crate) fn delete_annotation_task(app: &PDFolioApp, id: AnnotationId) -> Task<Message> {
    let db = Arc::clone(&app.db);
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                db.delete_annotation(&id)?;
                Ok::<_, anyhow::Error>(id)
            })
            .await?
        },
        |result| match result {
            Ok(id) => Message::AnnotationDeleteFinished { id: Some(id), error: None },
            Err(error) => Message::AnnotationDeleteFinished {
                id: None,
                error: Some(error.to_string()),
            },
        },
    )
}

/// Builds a new [`Annotation`] with a fresh id for the open library entry.
pub(crate) fn build_annotation_from_compose(
    app: &PDFolioApp,
    compose: &crate::viewer::document::AnnotationComposeState,
    body: String,
) -> Option<Annotation> {
    let entry_id = app.viewer.current_entry_id.clone()?;
    let now = chrono::Utc::now();
    let id = AnnotationId::new(format!(
        "annotation-{}-{}",
        now.timestamp_nanos_opt().unwrap_or_default(),
        app.viewer.annotations.len().saturating_add(1)
    ));
    Some(Annotation {
        id,
        entry_id,
        start_page: compose.start_page,
        start_char: compose.start_char,
        end_page: compose.end_page,
        end_char: compose.end_char,
        quote: compose.quote.clone(),
        body,
        created_at: now,
        updated_at: now,
    })
}
