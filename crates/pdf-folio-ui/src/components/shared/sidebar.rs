//! # Shared sidebar chrome
//!
//! Colors, scroll direction, chevrons, and action buttons reused by library
//! and viewer sidebars.

use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};

/// Vertical scrollable direction style for sidebar panes.
pub(crate) fn sidebar_scroll_direction(tokens: ThemeTokens) -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(tokens.primitives.sidebar_scrollbar_width)
            .scroller_width(tokens.primitives.sidebar_scrollbar_scroller_width)
            .anchor(Anchor::End),
    )
}

/// Primary text color for sidebar detail rows.
pub(crate) fn sidebar_detail_primary_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarDetailRow.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_primary)
}

/// Secondary/muted text color for sidebar detail rows.
pub(crate) fn sidebar_detail_secondary_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarSection.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_secondary)
}

/// Title color for folder cards shown in sidebars.
pub(crate) fn sidebar_folder_card_title_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarFolderCardTitle.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or_else(|| sidebar_detail_secondary_color(tokens))
}

/// Text input style for inline folder rename fields.
pub(crate) fn folder_sidebar_text_input_style(
    tokens: ThemeTokens,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    text_input_style(tokens, Class::SidebarFolderTextInput, status)
}

/// Icon button used to expand/collapse sidebar sections.
pub(crate) fn sidebar_chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    chevron_button(icon, tooltip_label, message, tokens, false)
}

/// Generic chevron icon button with tooltip and press message.
pub(crate) fn chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
    transparent: bool,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(tokens.primitives.sidebar_chevron_icon_size)
        .height(tokens.primitives.sidebar_chevron_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(tokens.primitives.sidebar_chevron_button_size)
    .height(tokens.primitives.sidebar_chevron_button_size)
    .padding(tokens.primitives.sidebar_chevron_button_padding)
    .style(move |_, status| {
        let _ = transparent;
        crate::style::button_style(tokens, Class::SidebarToggleButton, status)
    })
    .on_press(message);

    tooltip(
        button,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

/// Compact text action button for sidebar toolbars.
pub(crate) fn sidebar_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::SidebarActionButton, status))
}

/// Folder-specific action button with enabled styling.
pub(crate) fn sidebar_folder_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_folder_action_text_color(tokens, true)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SidebarFolderActionButton, status)
    })
}

/// Folder action button that may be omitted or disabled based on flags.
pub(crate) fn maybe_sidebar_folder_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    enabled: bool,
    message: Message,
) -> iced::widget::Button<'a, Message> {
    let button = button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_folder_action_text_color(tokens, enabled)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SidebarFolderActionButton, status)
    });

    if enabled {
        button.on_press(message)
    } else {
        button
    }
}

/// Text color for folder actions reflecting enabled/disabled state.
pub(crate) fn sidebar_folder_action_text_color(tokens: ThemeTokens, enabled: bool) -> Color {
    if enabled {
        tokens.class_styles[Class::SidebarFolderActionButton.index()]
            .resolve(ComponentState::Normal)
            .text_color
            .unwrap_or_else(|| sidebar_detail_primary_color(tokens))
    } else {
        tokens.class_styles[Class::SidebarFolderActionButton.index()]
            .resolve(ComponentState::Disabled)
            .text_color
            .unwrap_or(tokens.text_secondary)
    }
}
