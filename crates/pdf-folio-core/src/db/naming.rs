use anyhow::{Context, Result};
use rusqlite::Connection;

pub(crate) const MANUAL_ORDER_GAP: i64 = 1024;

pub(crate) fn sort_key(value: Option<&str>) -> Option<String> {
    clean_optional_text(value).map(|value| value.to_lowercase())
}

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

pub(crate) fn clean_folder_name(name: &str) -> Result<String> {
    clean_optional_text(Some(name)).context("Folder name cannot be empty.")
}

pub(crate) fn next_folder_suffix(connection: &Connection) -> Result<i64> {
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM folders", [], |row| row.get(0))?;
    Ok(count + 1)
}
