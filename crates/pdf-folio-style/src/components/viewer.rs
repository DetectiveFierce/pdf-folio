use iced::widget::{button, container};
use iced::{Element, Length};

use crate::classes::{button_style, container_style, Class};
use crate::tokens::{Spacing, ThemeTokens};

use super::core::with_normal_side_border;

/// Creates a table-of-contents entry button from arbitrary row content.
pub fn toc_entry<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::Fill)
        .style(move |_, status| button_style(tokens, Class::TocEntry, status))
}

/// Creates an annotation toolbar surface from arbitrary content.
pub fn annotation_toolbar<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let content = container(content)
        .padding(Spacing::MD)
        .style(move |_| container_style(tokens, Class::AnnotationToolbar));
    with_normal_side_border(content, tokens, Class::AnnotationToolbar)
}

/// Creates an annotation popover surface from arbitrary content.
pub fn annotation_popover<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let content = container(content)
        .padding(Spacing::MD)
        .style(move |_| container_style(tokens, Class::AnnotationPopover));
    with_normal_side_border(content, tokens, Class::AnnotationPopover)
}
