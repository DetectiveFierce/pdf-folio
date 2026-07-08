//! Library entry metadata display and formatting helpers.

use std::path::Path;

use pdf_folio_db::LibraryEntry;

use super::state::LibraryMetadataDensity;

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

pub fn entry_author(entry: &LibraryEntry) -> String {
    entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"))
}

pub fn clean_metadata_input(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

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

pub fn last_opened_label(entry: &LibraryEntry) -> String {
    entry.opened_at.map_or_else(
        || String::from("Never opened"),
        |opened_at| format!("Last opened {}", opened_at.format("%b %-d, %Y")),
    )
}

pub fn file_size_label(entry: &LibraryEntry) -> String {
    entry
        .file_size
        .map_or_else(|| String::from("Unknown size"), format_file_size)
}

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

pub fn file_size(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|metadata| metadata.len())
}

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
