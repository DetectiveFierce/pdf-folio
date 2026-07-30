//! # Library cover thumbnails
//!
//! Types and I/O for on-disk RGBA cover caches at multiple resolution tiers
//! ([`ThumbnailSize`]), plus iced-ready [`ThumbnailView`] handles.
//!
//! ## Ownership
//!
//! Mostly pure filesystem + PDF render helpers. The in-memory map of
//! [`ThumbnailCacheKey`] → view lives on `app.library`; this module does
//! not own that map. Domain code (`data::request_visible_thumbnails`)
//! decides *what* to load; here we load/render bytes.
//!
//! Bulk rebuild goes through [`bulk_thumbnail_task`] for menu actions;
//! import paths call [`cache_thumbnail_variants`] after opening a PDF.

use std::path::PathBuf;

use iced::widget::image;
use iced::Task;
use pdf_folio_core::{thumbnail_path, EntryId, LibraryEntry};
use pdf_folio_core::{PdfDoc, RenderedPage};

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
    /// Target render width in pixels for this tier.
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

/// Load a cached RGBA thumbnail or render page 0 and write the cache file.
pub(crate) async fn load_or_render_thumbnail(
    entry: LibraryEntry,
    size: ThumbnailSize,
) -> anyhow::Result<(EntryId, ThumbnailSize, RenderedPage)> {
    tokio::task::spawn_blocking(move || {
        let path = thumbnail_variant_path(&entry.id, size)?;
        if path.exists() {
            let data = std::fs::read(&path)?;
            let width = size.width_px();
            if let Some(height) = thumbnail_height_from_rgba_len(data.len(), width) {
                return Ok((
                    entry.id,
                    size,
                    RenderedPage {
                        width,
                        height,
                        rgba: data,
                    },
                ));
            }
        }

        let doc = PdfDoc::open(&entry.path)?;
        let page = doc.render_page(0, size.width_px())?;
        std::fs::write(path, &page.rgba)?;
        Ok((entry.id, size, page))
    })
    .await?
}

/// Synchronously load a cached thumbnail into an iced image handle, if present.
pub(crate) fn load_cached_thumbnail(
    entry_id: &EntryId,
    size: ThumbnailSize,
) -> anyhow::Result<Option<ThumbnailView>> {
    let path = thumbnail_variant_path(entry_id, size)?;
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(&path)?;
    let width = size.width_px();
    let height = thumbnail_height_from_rgba_len(data.len(), width).unwrap_or(1);
    let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
    Ok(Some(ThumbnailView {
        width,
        height,
        handle,
    }))
}

/// Rebuild Small/Default/Large cover variants for many entries (bulk operation UI).
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
    cache_thumbnail_variants(&entry.id, &doc)
}

/// Render and write all thumbnail size tiers for one entry from an open `PdfDoc`.
pub(crate) fn cache_thumbnail_variants(entry_id: &EntryId, doc: &PdfDoc) -> anyhow::Result<()> {
    for size in [
        ThumbnailSize::Small,
        ThumbnailSize::Default,
        ThumbnailSize::Large,
    ] {
        let path = thumbnail_variant_path(entry_id, size)?;
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

fn thumbnail_height_from_rgba_len(len: usize, width: u16) -> Option<u16> {
    let stride = usize::from(width) * 4;
    if stride == 0 || len < stride || !len.is_multiple_of(stride) {
        return None;
    }
    let height = len / stride;
    let max_height = usize::from(width) * 3;
    if height == 0 || height > max_height {
        return None;
    }
    Some(height as u16)
}
