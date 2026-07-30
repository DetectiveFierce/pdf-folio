//! # Loading overlays
//!
//! Full-surface blocking overlays under `components::shared::loading` for
//! long-running work that should freeze interaction: library history restore,
//! PDF document open, and startup library preparation.
//!
//! ## Ownership
//!
//! Reads timing and status fields from `PDFolioApp` and paints spinner /
//! progress chrome only. Spinners reuse [`HistoryRestoreSpinner`] from
//! `components::viewer::canvas`. Composition into the view tree is done by
//! [`super::root_surface`].
//!
//! Related: non-blocking progress banners in `components::library::import_status`.

use crate::components::viewer::canvas::HistoryRestoreSpinner;
use crate::*;
use iced::widget::{canvas, column};

/// Dimmed full-window spinner while organization history is being restored.
pub(crate) fn history_restore_spinner_layer(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let Some(started_at) = app.library.library_history_restore_started_at else {
        return container("").into();
    };
    let spinner_size = app.layout().metric("HistoryRestoreSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));
    let mut background = tokens.background;
    background.a = 0.54;

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(background)),
                ..iced::widget::container::Style::default()
            }),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

/// Overlay shown while a PDF document is opening in the viewer.
pub(crate) fn document_loading_layer(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let started_at = app
        .viewer
        .document_open_started_at
        .unwrap_or(app.library.animation_now);
    let spinner_size = app.layout().metric("DocumentLoadingSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

/// Overlay shown while the initial library load is in progress.
pub(crate) fn startup_library_loading_layer(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let status = app
        .library
        .raindrop_rollback_recovery_status
        .as_deref()
        .unwrap_or("Preparing library...");
    mouse_area(
        container(
            container(
                column![
                    text("Restoring library")
                        .size(FontSize::HEADING)
                        .font(display_font(FontWeight::MEDIUM))
                        .color(tokens.text_primary),
                    text(status).size(FontSize::MD).color(tokens.text_secondary),
                    container(progress_bar(0.42, tokens)).width(Length::Fill),
                ]
                .spacing(Spacing::MD)
                .padding(Spacing::LG),
            )
            .width(app.layout().metric("StartupLoadingDialog", "width", 460.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}
