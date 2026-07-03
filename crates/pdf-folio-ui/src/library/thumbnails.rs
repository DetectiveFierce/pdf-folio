//! Thumbnail cache types and async thumbnail tasks for the library UI.

use std::path::PathBuf;

use iced::widget::image;
use iced::Task;
use pdf_folio_core::{PdfDoc, RenderedPage};
use pdf_folio_db::{thumbnail_path, EntryId, LibraryEntry};

use crate::library::metadata::entry_title;
use crate::messages::Message;

/// A rendered cover thumbnail prepared for display by iced.
#[derive(Debug, Clone)]
pub struct ThumbnailView {
    /// Thumbnail width in pixels.
    pub width: u16,
    /// Thumbnail height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

/// Rendered cover thumbnail cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailCacheKey {
    /// Library entry id.
    pub entry_id: EntryId,
    /// Thumbnail resolution tier.
    pub size: ThumbnailSize,
}

/// Thumbnail resolution tiers stored on disk and selected by grid zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbnailSize {
    /// Dense grid thumbnail.
    Small,
    /// Normal grid and list thumbnail.
    Default,
    /// Large single/few-column grid thumbnail.
    Large,
}

impl ThumbnailSize {
    pub(crate) fn width_px(self) -> u16 {
        match self {
            Self::Small => 96,
            Self::Default => 200,
            Self::Large => 640,
        }
    }

    fn cache_suffix(self) -> Option<&'static str> {
        match self {
            Self::Small => Some("small"),
            Self::Default => None,
            Self::Large => Some("large"),
        }
    }
}

pub(crate) async fn load_or_render_thumbnail(
    entry: LibraryEntry,
    size: ThumbnailSize,
) -> anyhow::Result<(EntryId, ThumbnailSize, RenderedPage)> {
    tokio::task::spawn_blocking(move || {
        let path = thumbnail_variant_path(&entry.id, size)?;
        if path.exists() {
            let data = std::fs::read(&path)?;
            let width = size.width_px();
            let height = (data.len() / (usize::from(width) * 4)).clamp(1, usize::from(u16::MAX));
            return Ok((
                entry.id,
                size,
                RenderedPage {
                    width,
                    height: height as u16,
                    rgba: data,
                },
            ));
        }

        let doc = PdfDoc::open(&entry.path)?;
        let page = doc.render_page(0, size.width_px())?;
        std::fs::write(path, &page.rgba)?;
        Ok((entry.id, size, page))
    })
    .await?
}

pub(crate) fn bulk_thumbnail_task(entries: Vec<LibraryEntry>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let mut updated = 0;
                let mut errors = Vec::new();
                for entry in entries {
                    match rebuild_entry_thumbnail(&entry) {
                        Ok(()) => updated += 1,
                        Err(error) => errors.push(format!("{}: {error}", entry_title(&entry))),
                    }
                }
                (String::from("Rebuilt thumbnails for"), updated, errors)
            })
            .await
        },
        |result| match result {
            Ok((label, updated, errors)) => Message::BulkOperationFinished {
                label,
                updated,
                errors,
            },
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn rebuild_entry_thumbnail(entry: &LibraryEntry) -> anyhow::Result<()> {
    let doc = PdfDoc::open(&entry.path)?;
    for size in [
        ThumbnailSize::Small,
        ThumbnailSize::Default,
        ThumbnailSize::Large,
    ] {
        let path = thumbnail_variant_path(&entry.id, size)?;
        let page = doc.render_page(0, size.width_px())?;
        std::fs::write(path, &page.rgba)?;
    }
    Ok(())
}

fn thumbnail_variant_path(entry_id: &EntryId, size: ThumbnailSize) -> anyhow::Result<PathBuf> {
    let default_path = thumbnail_path(entry_id)?;
    let Some(suffix) = size.cache_suffix() else {
        return Ok(default_path);
    };
    Ok(default_path.with_file_name(format!("{}.{}.rgba", entry_id.as_str(), suffix)))
}
