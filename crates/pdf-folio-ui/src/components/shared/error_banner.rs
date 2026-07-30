//! # Error banners
//!
//! Dismissible error presentation used by library and shell surfaces.

use crate::*;
use iced::widget::row;

/// Inline error message with a dismiss control.
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
