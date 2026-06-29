//! Styled widget constructors for common UI pieces.

use iced::widget::{button, container, progress_bar as iced_progress_bar, text, text_input};
use iced::{Element, Length};

use super::classes::{button_style, container_style, progress_bar_style, text_input_style, Class};
use super::tokens::{
    ui_font, ContentAlignment, FontSize, FontWeight, Spacing, TextAlignment, ThemeTokens,
};

/// Creates text with semantic alignment control.
pub fn aligned_text<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    size: u32,
    alignment: TextAlignment,
) -> iced::widget::Text<'a> {
    weighted_text(label, tokens, size, alignment, FontWeight::REGULAR)
}

/// Creates text with semantic alignment and font weight control.
pub fn weighted_text<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    size: u32,
    alignment: TextAlignment,
    weight: iced::font::Weight,
) -> iced::widget::Text<'a> {
    text(label.into())
        .size(size)
        .font(ui_font(weight))
        .color(tokens.text_primary)
        .align_x(alignment.horizontal())
}

/// Applies horizontal semantic content alignment to a container.
pub fn align_content_x<'a, Message: 'a>(
    container: iced::widget::Container<'a, Message>,
    alignment: ContentAlignment,
) -> iced::widget::Container<'a, Message> {
    container.align_x(alignment.horizontal())
}

/// Applies vertical semantic content alignment to a container.
pub fn align_content_y<'a, Message: 'a>(
    container: iced::widget::Container<'a, Message>,
    alignment: ContentAlignment,
) -> iced::widget::Container<'a, Message> {
    container.align_y(alignment.vertical())
}

/// Creates a toolbar button.
pub fn toolbar_button<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(weighted_text(
        label,
        tokens,
        FontSize::MD,
        TextAlignment::Start,
        FontWeight::MEDIUM,
    ))
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

/// Creates a compact icon-like toolbar button.
pub fn icon_button<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(weighted_text(
        label,
        tokens,
        FontSize::MD,
        TextAlignment::Center,
        FontWeight::MEDIUM,
    ))
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

/// Creates a sidebar button.
pub fn sidebar_button<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(weighted_text(
        label,
        tokens,
        FontSize::MD,
        TextAlignment::Start,
        FontWeight::MEDIUM,
    ))
    .padding([Spacing::SM, Spacing::MD])
    .width(Length::Fill)
    .style(move |_, status| button_style(tokens, Class::SidebarRow, status))
}

/// Creates a table-of-contents entry button from arbitrary row content.
pub fn toc_entry<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::Fill)
        .style(move |_, status| button_style(tokens, Class::TocEntry, status))
}

/// Creates a library card button from arbitrary content.
pub fn library_card<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::FillPortion(1))
        .style(move |_, status| button_style(tokens, Class::LibraryCard, status))
}

/// Creates a library list-row button from arbitrary content.
pub fn library_row<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .width(Length::Fill)
        .style(move |_, status| button_style(tokens, Class::LibraryRow, status))
}

/// Creates a reusable tag pill button.
pub fn tag_pill<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(weighted_text(
        label,
        tokens,
        FontSize::SM,
        TextAlignment::Center,
        FontWeight::MEDIUM,
    ))
    .padding([Spacing::XS, Spacing::MD])
    .style(move |_, status| button_style(tokens, Class::TagPill, status))
}

/// Creates a section heading.
pub fn section_heading<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Text<'a> {
    text(label.into())
        .size(FontSize::SM)
        .font(ui_font(FontWeight::MEDIUM))
        .color(tokens.text_secondary)
        .align_x(TextAlignment::Start.horizontal())
}

/// Creates an empty-state panel.
pub fn empty_state<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text(label.into())
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::MEDIUM)),
    )
    .center(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::EmptyState))
    .into()
}

/// Creates a search input.
pub fn search_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    tokens: ThemeTokens,
    on_input: impl Fn(String) -> Message + 'a,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([super::tokens::Spacing::SM, super::tokens::Spacing::MD])
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
}

/// Creates a progress bar.
pub fn progress_bar(value: f32, tokens: ThemeTokens) -> iced::widget::ProgressBar<'static> {
    iced_progress_bar(0.0..=1.0, value.clamp(0.0, 1.0))
        .girth(tokens.primitives.progress_girth)
        .style(move |_| progress_bar_style(tokens, Class::ProgressBar))
}

/// Creates an error banner.
pub fn error_banner<'a, Message: 'a>(
    message: impl Into<String>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text(message.into())
            .size(FontSize::MD)
            .color(tokens.text_primary),
    )
    .padding(Spacing::MD)
    .width(Length::Fill)
    .style(move |_| container_style(tokens, Class::ErrorBanner))
    .into()
}

/// Creates an annotation toolbar surface from arbitrary content.
pub fn annotation_toolbar<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(content)
        .padding(Spacing::MD)
        .style(move |_| container_style(tokens, Class::AnnotationToolbar))
        .into()
}

/// Creates an annotation popover surface from arbitrary content.
pub fn annotation_popover<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(content)
        .padding(Spacing::MD)
        .style(move |_| container_style(tokens, Class::AnnotationPopover))
        .into()
}
