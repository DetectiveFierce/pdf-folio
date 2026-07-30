//! Reusable shell chrome widgets: buttons, inputs, tags, empty states, banners.
//!
//! Builders apply the matching [`Class`](crate::Class) stylesheet and layout
//! tokens (padding, text size/weight) so View menu / toolbar / sidebar chrome
//! stays consistent across library and viewer modes.

use iced::widget::{button, container, progress_bar as iced_progress_bar, text, text_input};
use iced::{Element, Length};

use crate::borders::side_border;
use crate::classes::{
    button_style, container_style, progress_bar_style, side_border_for_class, text_input_style,
    Class, ComponentState,
};
use crate::tokens::{
    display_font, ui_font, FontSize, FontWeight, Spacing, TextAlignment, ThemeTokens,
};

/// Primary-color UI text with horizontal alignment (regular weight).
///
/// Use for generic labels; prefer [`weighted_text`] when controls need medium/semibold.
pub fn aligned_text<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    size: u32,
    alignment: TextAlignment,
) -> iced::widget::Text<'a> {
    weighted_text(label, tokens, size, alignment, FontWeight::REGULAR)
}

/// Primary-color UI text with alignment and weight (IBM Plex Sans).
///
/// Shared by toolbar/tag factories so chrome labels stay on the UI face.
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

/// Labeled toolbar control using [`Class::ToolbarButton`] layout and paint.
///
/// Typical call sites: library control bar, viewer toolbar, selection toolbar.
/// Padding/size/weight come from component KDL with [`Spacing`] / [`FontSize`] fallbacks.
pub fn toolbar_button<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let layout = tokens.class_styles[Class::ToolbarButton.index()].layout;
    let text_style = tokens.class_styles[Class::ToolbarButton.index()].text;
    button(weighted_text(
        label,
        tokens,
        text_style.size.unwrap_or(FontSize::MD),
        TextAlignment::Start,
        text_style.weight.unwrap_or(FontWeight::MEDIUM),
    ))
    .padding([layout.padding_y(Spacing::SM), layout.padding_x(Spacing::LG)])
    .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

/// Compact square-ish toolbar control (icons, chevrons, overflow glyphs).
///
/// Same class paint as [`toolbar_button`] but tighter horizontal padding and centered label.
pub fn icon_button<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let layout = tokens.class_styles[Class::ToolbarButton.index()].layout;
    let text_style = tokens.class_styles[Class::ToolbarButton.index()].text;
    button(weighted_text(
        label,
        tokens,
        text_style.size.unwrap_or(FontSize::MD),
        TextAlignment::Center,
        text_style.weight.unwrap_or(FontWeight::MEDIUM),
    ))
    .padding([layout.padding_y(Spacing::SM), layout.padding_x(Spacing::MD)])
    .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

/// Small rounded tag chip using [`Class::TagPill`] (inspector, filters, bulk tag UI).
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

/// Secondary display-font section label for sidebars, inspectors, and dialogs.
pub fn section_heading<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Text<'a> {
    text(label.into())
        .size(FontSize::CONTROL)
        .font(display_font(FontWeight::SEMIBOLD))
        .color(tokens.text_secondary)
        .align_x(TextAlignment::Start.horizontal())
}

/// Full-pane empty-state message using [`Class::EmptyState`] (no results, empty library, …).
pub fn empty_state<'a, Message: 'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let content = container(
        text(label.into())
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM)),
    )
    .center(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::EmptyState));
    with_normal_side_border(content, tokens, Class::EmptyState)
}

/// Search field styled for a caller-chosen semantic [`Class`].
///
/// Pass [`Class::LibrarySearchInput`] for the library control bar, or other
/// input classes when reusing the same padding/type scale under different paint.
pub fn search_input_with_class<'a, Message: Clone + 'a>(
    placeholder: &str,
    value: &str,
    tokens: ThemeTokens,
    class: Class,
    on_input: impl Fn(String) -> Message + 'a,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(on_input)
        .padding([crate::tokens::Spacing::SM, crate::tokens::Spacing::MD])
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, class, status))
}

/// Thin progress rail using [`Class::ProgressBar`] and `primitives.progress_girth`.
///
/// `value` is clamped to `0.0..=1.0` (library reading progress, import bars, …).
pub fn progress_bar(value: f32, tokens: ThemeTokens) -> iced::widget::ProgressBar<'static> {
    iced_progress_bar(0.0..=1.0, value.clamp(0.0, 1.0))
        .girth(tokens.primitives.progress_girth)
        .style(move |_| progress_bar_style(tokens, Class::ProgressBar))
}

/// Wraps `content` with a side-border when `class` defines one in the Normal state.
pub(crate) fn with_normal_side_border<'a, Message: 'a>(
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
