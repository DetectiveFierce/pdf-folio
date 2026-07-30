//! # Import and bulk-operation progress UI
//!
//! Banners and modal progress UI under `components::library::import_status`
//! for long-running library work: bulk tag/move/delete operations and
//! streaming Raindrop imports. Shows indeterminate progress and phase labels
//! without owning the tasks themselves.
//!
//! Tasks and progress mutation live in `crate::library::{tasks, update}`;
//! richer import/export configuration dialogs live in [`super::dialogs`].

use crate::library::view::format_count;
use crate::*;
use iced::widget::{column, row};
use pdf_folio_cloud::raindrop::RaindropImportPhase;

/// Inline banner for an in-flight bulk operation with indeterminate progress.
pub(crate) fn bulk_operation_progress_banner<'a>(
    app: &'a PDFolioApp,
    progress: &'a BulkOperationProgress,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let elapsed = app
        .library
        .animation_now
        .saturating_duration_since(progress.started_at)
        .as_secs_f32();
    let value = indeterminate_progress_value(elapsed);
    let label = format!("{} {} PDFs...", progress.label, progress.total);

    container(
        column![
            row![
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text("Working in background")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            ]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center),
            progress_bar(value, tokens),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

/// Oscillating 0..1 value for indeterminate progress bars from elapsed time.
pub(crate) fn indeterminate_progress_value(elapsed_secs: f32) -> f32 {
    let sweep = (elapsed_secs * 0.72).fract();
    (0.18 + 0.64 * (0.5 - (sweep - 0.5).abs()) * 2.0).clamp(0.0, 1.0)
}

/// Modal progress UI while a Raindrop import stream is running.
pub(crate) fn view_raindrop_import_progress_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let Some(progress) = app.library.raindrop_import_progress.as_ref() else {
        return container("").into();
    };
    let total = progress.total.max(1);
    let value = progress.progress_basis_points.map_or_else(
        || progress.completed as f32 / total as f32,
        |basis_points| f32::from(basis_points) / 10_000.0,
    );
    let status = match progress.phase {
        RaindropImportPhase::PreparingImports => String::from("Preparing Imports"),
        RaindropImportPhase::DownloadingImportFiles => String::from("Downloading Import Files"),
        RaindropImportPhase::ImportingDownloadedFiles if progress.failed => {
            format!(
                "Importing downloaded Files: skipped {} of {}",
                progress.completed,
                format_count(progress.total, "PDF")
            )
        }
        RaindropImportPhase::ImportingDownloadedFiles => {
            format!(
                "Importing downloaded Files: imported {} of {}",
                progress.completed,
                format_count(progress.total, "PDF")
            )
        }
    };

    let content = column![
        text("Importing from Raindrop.io")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text(status).size(FontSize::MD).color(tokens.text_secondary),
        container(progress_bar(value, tokens)).width(Length::Fill),
        text(truncate_for_width_with_font(
            &progress.current_title,
            app.layout()
                .metric("RaindropImportProgressDialog", "title_width", 400.0),
            0.0,
            FontSize::SM
        ))
        .size(FontSize::SM)
        .font(ui_font(FontWeight::MEDIUM))
        .color(if progress.failed {
            tokens.error
        } else {
            tokens.text_secondary
        })
        .wrapping(Wrapping::None),
        row![
            text("Cancel rolls back PDFs imported so far.")
                .size(FontSize::SM)
                .color(tokens.text_secondary)
                .width(Length::Fill),
            toolbar_button("Cancel import", tokens).on_press(Message::CancelRaindropImport),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(content)
            .width(
                app.layout()
                    .metric("RaindropImportProgressDialog", "width", 460.0),
            )
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}
