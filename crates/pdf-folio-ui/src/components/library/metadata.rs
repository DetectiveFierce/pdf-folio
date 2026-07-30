//! # Entry metadata formatting
//!
//! Pure display helpers under `components::library::metadata` for turning
//! [`LibraryEntry`] fields into user-facing strings. Covers title/author
//! resolution (display override → extracted → path/unknown fallbacks), file
//! size labels, last-opened dates, reading progress, and density-aware card
//! and list-row meta strings driven by [`LibraryMetadataDensity`].
//!
//! ## Ownership
//!
//! No database access and no iced widgets—callers in the library domain and
//! presentation helpers (cards, inspector, dialogs) format for display only.
//! Related modules: [`super::state`] for density enums, [`super::filters`]
//! for search field matching that reuses [`entry_title`] / [`entry_author`].

use std::path::Path;

use pdf_folio_core::LibraryEntry;

use super::state::LibraryMetadataDensity;

/// Resolve the display title: `display_title`, then extracted `title`, then
/// the file stem, else `"Untitled PDF"`.
pub fn entry_title(entry: &LibraryEntry) -> String {
    entry
        .display_title
        .clone()
        .or_else(|| entry.title.clone())
        .unwrap_or_else(|| {
            entry
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Untitled PDF")
                .to_owned()
        })
}

/// Resolve the display author: `display_author`, then extracted `author`,
/// else `"Unknown author"`.
pub fn entry_author(entry: &LibraryEntry) -> String {
    entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"))
}

/// Trim user-edited metadata input; empty strings become `None` so callers can
/// clear optional fields without storing whitespace-only values.
pub fn clean_metadata_input(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Human-readable page count for inspector and detail panes (`"N Pages"` or
/// `"Unknown pages"` when count is missing).
pub fn page_count_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || String::from("Unknown pages"),
        |pages| {
            if pages == 1 {
                String::from("1 Page")
            } else {
                format!("{pages} Pages")
            }
        },
    )
}

/// Last-opened date line for detail panes (`"Last opened Mon D, YYYY"` or
/// `"Never opened"`).
pub fn last_opened_label(entry: &LibraryEntry) -> String {
    entry.opened_at.map_or_else(
        || String::from("Never opened"),
        |opened_at| format!("Last opened {}", opened_at.format("%b %-d, %Y")),
    )
}

/// Single-entry file size string for cards, rows, and inspector fields.
pub fn file_size_label(entry: &LibraryEntry) -> String {
    entry
        .file_size
        .map_or_else(|| String::from("Unknown size"), format_file_size)
}

/// Sum file sizes across a selection for bulk-op summaries; appends
/// `"+ unknown"` when some entries lack size metadata.
pub fn total_file_size_label(entries: &[LibraryEntry]) -> String {
    let mut total = 0_u64;
    let mut unknown = 0_usize;

    for entry in entries {
        match entry.file_size {
            Some(file_size) => total = total.saturating_add(file_size),
            None => unknown += 1,
        }
    }

    match (total, unknown) {
        (0, 0) | (0, _) => String::from("Unknown size"),
        (_, 0) => format_file_size(total),
        _ => format!("{} + unknown", format_file_size(total)),
    }
}

/// Secondary metadata line under a grid card title, scaled by density.
///
/// Returns `None` for [`LibraryMetadataDensity::Minimal`] so the card omits
/// the line entirely; standard shows page count, detailed adds file size.
pub fn library_card_metadata_label(
    density: LibraryMetadataDensity,
    entry: &LibraryEntry,
) -> Option<String> {
    match density {
        LibraryMetadataDensity::Minimal => None,
        LibraryMetadataDensity::Standard => Some(library_card_page_count_label(entry)),
        LibraryMetadataDensity::Detailed => Some(format!(
            "{}   •   {}",
            library_card_page_count_label(entry),
            file_size_label(entry)
        )),
    }
}

/// Compact secondary line for list-row layouts (author, optional pages, size).
pub fn library_row_metadata_label(density: LibraryMetadataDensity, entry: &LibraryEntry) -> String {
    match density {
        LibraryMetadataDensity::Minimal => entry_author(entry),
        LibraryMetadataDensity::Standard => format!(
            "{}{}",
            entry_author(entry),
            entry
                .page_count
                .map_or(String::new(), |pages| format!(" . {pages} pages"))
        ),
        LibraryMetadataDensity::Detailed => format!(
            "{}{} . {}",
            entry_author(entry),
            entry
                .page_count
                .map_or(String::new(), |pages| format!(" . {pages} pages")),
            file_size_label(entry)
        ),
    }
}

/// Lowercase page-count fragment used on grid cards (`"n pages"`).
pub fn library_card_page_count_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || String::from("Unknown pages"),
        |pages| {
            if pages == 1 {
                String::from("1 page")
            } else {
                format!("{pages} pages")
            }
        },
    )
}

/// Format a byte count with binary units (`B` … `TiB`), preferring whole
/// numbers once the value is ≥ 10 of the current unit.
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Read on-disk byte length for `path`, used when importing or refreshing an
/// entry that does not yet carry a stored `file_size`.
pub fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

/// Reading progress in `0.0..=1.0` from `last_page` and known page count.
///
/// Missing files and unknown page counts yield `0.0`. Used by progress bars
/// and reading-filter classification in [`super::filters`].
pub fn progress_fraction(entry: &LibraryEntry) -> f32 {
    if entry.missing {
        return 0.0;
    }

    entry.page_count.map_or(0.0, |pages| {
        if pages == 0 {
            0.0
        } else {
            (f32::from(entry.last_page) + 1.0) / f32::from(pages)
        }
    })
}
