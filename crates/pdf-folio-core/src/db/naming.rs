//! Private naming helpers for sort keys and gap-spaced manual order values.
//!
//! Shared by [`super::library`], [`super::organization`], [`super::metadata`],
//! and [`super::raindrop`] so title/author normalization and folder naming stay
//! consistent. Not part of the public crate API (`pub(crate)` only).
//!
//! Manual order columns store integers spaced by [`MANUAL_ORDER_GAP`] so the
//! UI can reorder with simple rewrites of a contiguous visible list without
//! packing every sibling on every drag. Sort keys are lowercased, whitespace-
//! collapsed, control-stripped, and capped at 512 characters.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Spacing between successive manual-order ranks (`1 * gap`, `2 * gap`, …).
///
/// Large enough that occasional mid-list inserts can use intermediate values
/// if a future API needs them; current setters rewrite the whole visible list.
pub(crate) const MANUAL_ORDER_GAP: i64 = 1024;

/// Builds a lowercased sort key from optional display text, or `None` if empty after cleaning.
pub(crate) fn sort_key(value: Option<&str>) -> Option<String> {
    clean_optional_text(value).map(|value| value.to_lowercase())
}

/// Trims control characters, collapses whitespace, caps length, and drops empty results.
pub(crate) fn clean_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(|value| {
            value
                .chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>()
        })
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(512).collect())
}

/// Lowercased title sort key with leading English articles (`the`/`a`/`an`) stripped.
pub(crate) fn clean_title_sort_key(title: &str) -> Option<String> {
    let title = clean_optional_text(Some(title))?;
    let lower = title.to_lowercase();
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return Some(rest.to_owned());
        }
    }
    Some(lower)
}

/// Validates and normalizes a folder display name (non-empty after cleaning).
///
/// # Errors
///
/// Returns an error when the name is empty or only whitespace/control characters.
pub(crate) fn clean_folder_name(name: &str) -> Result<String> {
    clean_optional_text(Some(name)).context("Folder name cannot be empty.")
}

/// Returns a monotonic-ish suffix for new folder IDs (`count + 1` of existing folders).
pub(crate) fn next_folder_suffix(connection: &Connection) -> Result<i64> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))?;
    Ok(count + 1)
}
