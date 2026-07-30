//! # Sync status indicators
//!
//! Compact toolbar chrome under `components::shared::sync_status` that
//! reflects cloud sync state for the active library: in-progress spinner,
//! queued ellipsis, or last-synced checkmark with a tooltip timestamp.
//!
//! ## Ownership
//!
//! Reads `app.sync_*` and auth state; emits no messages (pure indicator).
//! Hidden when the user is signed out. Spinner geometry reuses
//! [`HistoryRestoreSpinner`] from `components::viewer::canvas`; muted colors
//! use `with_alpha` from `components::library::view`.
//!
//! Related: blocking restore overlays in [`super::loading`]; auth flows live
//! in the shell, not this module.

use crate::components::library::view::with_alpha;
use crate::components::viewer::canvas::HistoryRestoreSpinner;
use crate::*;
use chrono::{DateTime, Local};
use iced::widget::canvas;
use std::time::{Duration, SystemTime};

/// Checkmark glyph shown when sync is idle and the last run completed.
const SYNC_CHECK_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"##;

/// Library toolbar glyph for cloud sync: spinner, check, or queued ellipsis.
///
/// Tooltip text describes the current phase (“Syncing changes”, queue wait,
/// or “Last synced at …”). Returns an empty shrink element when signed out.
pub(crate) fn library_sync_indicator(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    if !app.sync_auth.is_signed_in() {
        return container("").width(Length::Shrink).into();
    }

    let size = app.layout().metric("LibrarySyncIndicator", "size", 30.0);
    let tooltip_label = if app.sync_in_progress.is_some() {
        String::from("Syncing changes")
    } else if !app.sync_queued_libraries.is_empty() {
        String::from("Waiting to begin syncing changes")
    } else {
        last_sync_tooltip_label(app.last_sync_completed_at)
    };

    let content: Element<'_, Message> = if app.sync_in_progress.is_some() {
        let spinner_size = app
            .layout()
            .metric("LibrarySyncIndicator", "spinner_size", 14.0);
        canvas(HistoryRestoreSpinner {
            started_at: app
                .last_sync_started_at
                .unwrap_or(app.library.animation_now),
            now: app.library.animation_now,
            color: with_alpha(tokens.accent, 0.82),
        })
        .width(Length::Fixed(spinner_size))
        .height(Length::Fixed(spinner_size))
        .into()
    } else if app.sync_queued_libraries.is_empty() {
        let icon_size = app
            .layout()
            .metric("LibrarySyncIndicator", "icon_size", 14.0);
        Svg::new(iced::widget::svg::Handle::from_memory(SYNC_CHECK_SVG))
            .width(Length::Fixed(icon_size))
            .height(Length::Fixed(icon_size))
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(with_alpha(tokens.text_secondary, 0.78)),
            })
            .into()
    } else {
        text("...")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(with_alpha(tokens.text_secondary, 0.82))
            .wrapping(Wrapping::None)
            .into()
    };

    let indicator = container(content)
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);

    tooltip(
        indicator,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(400))
    .into()
}

/// Tooltip string for idle sync: “Last synced at …” or “never”.
fn last_sync_tooltip_label(last_synced_at: Option<SystemTime>) -> String {
    match last_synced_at {
        Some(time) => format!("Last synced at {}", format_local_time(time)),
        None => String::from("Last synced at never"),
    }
}

/// Local wall-clock time for sync tooltips (`%-I:%M:%S %p`).
fn format_local_time(time: SystemTime) -> String {
    let local: DateTime<Local> = time.into();
    local.format("%-I:%M:%S %p").to_string()
}
