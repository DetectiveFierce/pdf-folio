//! Styled widget constructors for common UI pieces.

use iced::widget::{button, container, progress_bar as iced_progress_bar, text, text_input};
use iced::{Element, Length};

use super::classes::{
    button_style, container_style, progress_bar_style, side_border_for_class, text_input_style,
    Class, ComponentState,
};
use super::side_border::side_border;
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
    let content = container(
        text(label.into())
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::MEDIUM)),
    )
    .center(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::EmptyState));
    with_normal_side_border(content, tokens, Class::EmptyState)
}

/// Creates a search input.
pub fn search_input<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    tokens: ThemeTokens,
    on_input: impl Fn(String) -> Message + 'a,
) -> iced::widget::TextInput<'a, Message> {
    search_input_with_class(placeholder, value, tokens, Class::SearchInput, on_input)
}

/// Creates a search input for a specific semantic class.
pub fn search_input_with_class<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    tokens: ThemeTokens,
    class: Class,
    on_input: impl Fn(String) -> Message + 'a,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([super::tokens::Spacing::SM, super::tokens::Spacing::MD])
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, class, status))
}

/// Creates a progress bar.
pub fn progress_bar(value: f32, tokens: ThemeTokens) -> iced::widget::ProgressBar<'static> {
    iced_progress_bar(0.0..=1.0, value.clamp(0.0, 1.0))
        .girth(tokens.primitives.progress_girth)
        .style(move |_| progress_bar_style(tokens, Class::ProgressBar))
}

/// Selection state represented by the library master checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterCheckboxState {
    /// No visible entries are selected.
    None,
    /// Some, but not all, visible entries are selected.
    Partial,
    /// Every visible entry is selected.
    All,
}

/// Creates a library entry selection checkbox.
pub fn selection_checkbox<'a, Message: Clone + 'a>(
    checked: bool,
    tokens: ThemeTokens,
    on_toggle: Message,
) -> iced::widget::Button<'a, Message> {
    checkbox_button(
        if checked { "✓" } else { "" },
        tokens,
        Class::SelectionCheckbox,
    )
    .on_press(on_toggle)
}

/// Creates a master selection checkbox for all visible library entries.
pub fn master_checkbox<'a, Message: Clone + 'a>(
    state: MasterCheckboxState,
    tokens: ThemeTokens,
    on_click: Message,
) -> iced::widget::Button<'a, Message> {
    let label = match state {
        MasterCheckboxState::None => "",
        MasterCheckboxState::Partial => "−",
        MasterCheckboxState::All => "✓",
    };
    checkbox_button(label, tokens, Class::MasterCheckbox).on_press(on_click)
}

fn checkbox_button<'a, Message: Clone + 'a>(
    label: &'static str,
    tokens: ThemeTokens,
    class: Class,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_primary)
            .align_x(iced::alignment::Horizontal::Center),
    )
    .width(Length::Fixed(24.0))
    .height(Length::Fixed(24.0))
    .padding(0)
    .style(move |_, status| button_style(tokens, class, status))
}

/// Creates an error banner.
pub fn error_banner<'a, Message: 'a>(
    message: impl Into<String>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let content = container(
        text(message.into())
            .size(FontSize::MD)
            .color(tokens.text_primary),
    )
    .padding(Spacing::MD)
    .width(Length::Fill)
    .style(move |_| container_style(tokens, Class::ErrorBanner));
    with_normal_side_border(content, tokens, Class::ErrorBanner)
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

fn with_normal_side_border<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    tokens: ThemeTokens,
    class: Class,
) -> Element<'a, Message> {
    if let Some(border) = side_border_for_class(tokens, class, ComponentState::Normal) {
        side_border(content, border)
    } else {
        content.into()
    }
}
