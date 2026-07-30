//! # Library switcher previews
//!
//! Loads a small set of cover thumbnails and entry counts for each vault so
//! the library switcher can show visual cards without opening full UI state.
//!
//! Opens each profile's SQLite database for listing and loads cached small
//! thumbnails. Missing covers are left for the normal, asynchronous thumbnail
//! pipeline; preview refreshes must never render PDFs on a startup/sync worker.

use crate::library::registry::state::{
    LibraryPreview, LibraryPreviewThumbnail, LIBRARY_SWITCHER_PREVIEW_LIMIT,
};
use crate::*;
use pdf_folio_core::thumbnail_path;

/// Build entry count + up to `LIBRARY_SWITCHER_PREVIEW_LIMIT` covers for one profile.
pub(crate) fn load_library_preview(profile: &LibraryProfile) -> LibraryPreview {
    Db::open(profile.db_path.clone())
        .and_then(|db| db.library_preview_entries(LIBRARY_SWITCHER_PREVIEW_LIMIT))
        .map(|(total_entries, entries)| LibraryPreview {
            total_entries,
            thumbnails: entries
                .iter()
                .take(LIBRARY_SWITCHER_PREVIEW_LIMIT)
                .filter_map(library_preview_thumbnail)
                .collect(),
        })
        .unwrap_or_default()
}

/// Display title for a switcher preview tile (display title → title → path stem → `"PDF"`).
fn library_preview_title(entry: &LibraryEntry) -> String {
    entry
        .display_title
        .as_deref()
        .or(entry.title.as_deref())
        .or_else(|| entry.path.file_stem().and_then(|stem| stem.to_str()))
        .or_else(|| entry.path.file_name().and_then(|name| name.to_str()))
        .unwrap_or("PDF")
        .to_owned()
}

/// Load a cached small cover handle for one entry in a switcher preview strip.
pub(super) fn library_preview_thumbnail(entry: &LibraryEntry) -> Option<LibraryPreviewThumbnail> {
    let default_path = thumbnail_path(&entry.id).ok()?;
    let variants = [
        (
            default_path.with_file_name(format!("{}.small.rgba", entry.id.as_str())),
            ThumbnailSize::Small.width_px(),
        ),
        (default_path.clone(), ThumbnailSize::Default.width_px()),
        (
            default_path.with_file_name(format!("{}.large.rgba", entry.id.as_str())),
            ThumbnailSize::Large.width_px(),
        ),
    ];
    let (rgba, width, height) = variants.into_iter().find_map(|(path, width)| {
        let rgba = std::fs::read(path).ok()?;
        let height = thumbnail_height_from_rgba_len(rgba.len(), width)?;
        Some((rgba, width, height))
    })?;

    Some(LibraryPreviewThumbnail {
        title: library_preview_title(entry),
        width,
        height,
        handle: image::Handle::from_rgba(u32::from(width), u32::from(height), rgba),
    })
}

/// Infer pixel height from raw RGBA byte length given `width` (4 bytes/pixel).
fn thumbnail_height_from_rgba_len(len: usize, width: u16) -> Option<u16> {
    let stride = usize::from(width) * 4;
    if stride == 0 || len < stride {
        return None;
    }
    Some((len / stride).clamp(1, usize::from(u16::MAX)) as u16)
}
