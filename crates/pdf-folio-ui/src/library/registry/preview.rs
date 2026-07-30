//! # Library switcher previews
//!
//! Loads a small set of cover thumbnails and entry counts for each vault so
//! the library switcher can show visual cards without opening full UI state.
//!
//! Opens each profile's SQLite read-only for listing, prefers cached small
//! thumbnails, and falls back to rendering page 0 when needed.

use crate::library::registry::state::{
    LibraryPreview, LibraryPreviewThumbnail, LIBRARY_SWITCHER_PREVIEW_LIMIT,
};
use crate::*;
use pdf_folio_core::thumbnail_path;

/// Build entry count + up to `LIBRARY_SWITCHER_PREVIEW_LIMIT` covers for one profile.
pub(crate) fn load_library_preview(profile: &LibraryProfile) -> LibraryPreview {
    Db::open(profile.db_path.clone())
        .and_then(|db| db.get_entries_sorted(LibrarySortMode::RecentlyAdded))
        .map(|entries| LibraryPreview {
            total_entries: entries.len(),
            thumbnails: entries
                .iter()
                .take(LIBRARY_SWITCHER_PREVIEW_LIMIT)
                .filter_map(library_preview_thumbnail)
                .collect(),
        })
        .unwrap_or_default()
}

/// Load previews for every profile into a map keyed by library id.
pub(super) fn load_library_previews(
    profiles: &[LibraryProfile],
) -> HashMap<String, LibraryPreview> {
    profiles
        .iter()
        .map(|profile| (profile.id.clone(), load_library_preview(profile)))
        .collect()
}

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

/// Load or render a small cover handle for one entry in a switcher preview strip.
pub(super) fn library_preview_thumbnail(entry: &LibraryEntry) -> Option<LibraryPreviewThumbnail> {
    let width = ThumbnailSize::Small.width_px();
    let path = small_thumbnail_path(&entry.id).ok()?;
    let (rgba, height) = if path.exists() {
        let rgba = std::fs::read(&path).ok()?;
        let height = thumbnail_height_from_rgba_len(rgba.len(), width)?;
        (rgba, height)
    } else {
        let doc = PdfDoc::open(&entry.path).ok()?;
        let page = doc.render_page(0, width).ok()?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &page.rgba);
        (page.rgba, page.height)
    };

    Some(LibraryPreviewThumbnail {
        title: library_preview_title(entry),
        width,
        height,
        handle: image::Handle::from_rgba(u32::from(width), u32::from(height), rgba),
    })
}

fn small_thumbnail_path(entry_id: &EntryId) -> anyhow::Result<PathBuf> {
    Ok(thumbnail_path(entry_id)?.with_file_name(format!("{}.small.rgba", entry_id.as_str())))
}

fn thumbnail_height_from_rgba_len(len: usize, width: u16) -> Option<u16> {
    let stride = usize::from(width) * 4;
    if stride == 0 || len < stride {
        return None;
    }
    Some((len / stride).clamp(1, usize::from(u16::MAX)) as u16)
}
