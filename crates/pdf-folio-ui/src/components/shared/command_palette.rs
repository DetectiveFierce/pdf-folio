//! # Command palette
//!
//! Overlay capture layer and filtered command list for keyboard-driven actions.

use crate::shell::commands::{command_matches, library_commands, CommandDanger};
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{column, row};

/// Full-window click-catcher that dismisses the palette when clicked.
pub(crate) fn command_palette_capture_layer<'a>() -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseCommandPalette),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Searchable command list overlay for the current command surface.
pub(crate) fn view_command_palette(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let panel_width = app
        .layout()
        .metric("CommandPalette", "width", 520.0)
        .min((app.viewer.viewport_width - Spacing::XL * 2.0).max(320.0));
    let list_height = app
        .layout()
        .metric("CommandPalette", "list_height", 420.0)
        .min((app.viewer.viewport_height - Spacing::XL * 2.0 - 148.0).max(180.0));
    let commands = library_commands(app)
        .into_iter()
        .filter(|command| command.visible && command.enabled)
        .filter(|command| command_matches(command.spec, &app.chrome.command_palette_query))
        .collect::<Vec<_>>();

    let input = text_input("Search commands", &app.chrome.command_palette_query)
        .on_input(Message::CommandPaletteQueryChanged)
        .on_submit(Message::CommandPaletteRunSelected)
        .padding([Spacing::SM, Spacing::MD])
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, Class::LibrarySearchInput, status))
        .width(Length::Fill);

    let mut list = column![].spacing(Spacing::XS).width(Length::Fill);
    for (index, command) in commands.iter().enumerate() {
        let selected = index == app.chrome.command_palette_selected_index;
        let text_color = if selected {
            tokens.text_primary
        } else if command.spec.danger == CommandDanger::Destructive {
            tokens.error
        } else {
            tokens.text_secondary
        };
        let shortcut = command.spec.shortcut.unwrap_or("");
        let target_label = match command.spec.target {
            crate::shell::commands::CommandTargetKind::None => "",
            crate::shell::commands::CommandTargetKind::Library => "Library",
            crate::shell::commands::CommandTargetKind::Folder => "Folder",
            crate::shell::commands::CommandTargetKind::Tag => "Tag",
            crate::shell::commands::CommandTargetKind::SinglePdf => "PDF",
            crate::shell::commands::CommandTargetKind::MultiplePdfs => "Selection",
            crate::shell::commands::CommandTargetKind::SearchResult => "Visible",
            crate::shell::commands::CommandTargetKind::Viewer => "Viewer",
            crate::shell::commands::CommandTargetKind::Document => "Document",
        };
        let icon_slot = if command.spec.icon.is_some() {
            "•"
        } else {
            ""
        };
        let row_content = row![
            text(icon_slot)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary)
                .width(Length::Fixed(app.layout().metric(
                    "CommandPalette",
                    "icon_slot_width",
                    12.0,
                ))),
            column![
                text(command.spec.label)
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(text_color)
                    .wrapping(Wrapping::None),
                text(
                    format!("{} {}", command.spec.category.label(), target_label)
                        .trim()
                        .to_owned()
                )
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
            ]
            .spacing(
                app.layout()
                    .metric("CommandPalette", "metadata_spacing", 1.0,)
            )
            .width(Length::Fill),
            text(shortcut)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center);
        list = list.push(
            button(row_content)
                .padding([Spacing::SM, Spacing::MD])
                .width(Length::Fill)
                .on_press(Message::CommandPaletteRun(command.spec.id))
                .style(move |_, status| {
                    let class = if selected {
                        Class::MenuButton
                    } else {
                        Class::MenuItem
                    };
                    button_style(tokens, class, status)
                }),
        );
    }
    if commands.is_empty() {
        list = list.push(
            container(
                text("No commands found")
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            )
            .padding(Spacing::MD),
        );
    }

    let list_scroll = scrollable(list)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(tokens.primitives.scrollbar_width)
                .scroller_width(tokens.primitives.scrollbar_scroller_width)
                .anchor(Anchor::End),
        ))
        .height(list_height)
        .width(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::MenuPanel, status));

    let panel = column![
        text("Command Palette")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        input,
        list_scroll,
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG)
    .width(panel_width);

    pin(container(
        container(panel)
            .width(panel_width)
            .style(move |_| container_style(tokens, Class::MenuPanel)),
    )
    .center(Length::Fill))
    .into()
}
