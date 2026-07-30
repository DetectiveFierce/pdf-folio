//! # Error banners
//!
//! Dismissible inline error chrome under `components::shared::error_banner`.
//! Used by the shell root surface and library mode to show transient failures
//! (document open errors, library load errors) without blocking the main UI.
//!
//! ## Ownership
//!
//! Pure presentation: callers supply the message text, theme tokens, layout
//! metrics for the dismiss control width, and the `Message` emitted on
//! dismiss. No app mutation occurs inside this module.
//!
//! Related: [`super::loading`] for blocking overlays; modal confirmations live
//! in `components::library::dialogs`.

use crate::*;
use iced::widget::row;

/// Full-width error strip with message text and a trailing dismiss button.
///
/// `dismiss_message` is typically a domain/shell variant that clears the
/// corresponding error field on `PDFolioApp`.
pub(crate) fn dismissible_error_banner<'a>(
    message: &'a str,
    tokens: ThemeTokens,
    layout: &crate::style::AppLayoutTokens,
    dismiss_message: Message,
) -> Element<'a, Message> {
    container(
        row![
            text(message)
                .size(FontSize::MD)
                .color(tokens.text_primary)
                .width(Length::Fill),
            icon_button("x", tokens)
                .on_press(dismiss_message)
                .width(Length::Fixed(layout.metric(
                    "ErrorBannerAction",
                    "action_width",
                    32.0
                ))),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center),
    )
    .padding(Spacing::MD)
    .width(Length::Fill)
    .style(move |_| container_style(tokens, Class::ErrorBanner))
    .into()
}
