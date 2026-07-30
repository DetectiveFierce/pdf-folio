//! Viewer-oriented styled widgets: TOC rows.
//!
//! These helpers wrap content with the correct viewer class styles. Page canvas
//! painting uses [`crate::classes::viewer_primitives`] instead of widgets from
//! this module.

use iced::widget::button;
use iced::Length;

use crate::classes::{button_style, Class};
use crate::tokens::ThemeTokens;

/// Creates a table-of-contents entry button from arbitrary row content.
pub fn toc_entry<'a, Message: 'a>(
    content: impl Into<iced::Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::Fill)
        .style(move |_, status| button_style(tokens, Class::TocEntry, status))
}
