//! Async task constructors for viewer document loading and rendering.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::Task;
use pdf_folio_core::{PdfDoc, RenderedPage, TileKey};
use pdf_folio_db::EntryId;

use crate::messages::Message;
use crate::PDFolioApp;

pub(crate) async fn render_page(
    doc: Arc<PdfDoc>,
    key: TileKey,
) -> anyhow::Result<(TileKey, RenderedPage)> {
    let page =
        tokio::task::spawn_blocking(move || doc.render_page(key.page, key.width_px)).await??;
    Ok((key, page))
}

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

pub(crate) fn schedule_zoom_render(generation: u64) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(140)).await;
            generation
        },
        Message::ZoomRenderSettled,
    )
}
