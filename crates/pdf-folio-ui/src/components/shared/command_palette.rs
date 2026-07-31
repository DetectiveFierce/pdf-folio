//! # Command palette
//!
//! Keyboard-driven command overlay under `components::shared::command_palette`.
//! Provides a full-window capture layer that dismisses on outside click and a
//! searchable list filtered by the current command surface (library vs viewer).
//!
//! ## Ownership
//!
//! Presentation only: reads `app.chrome.command_palette_*` query and selection
//! index, and visibility from `crate::shell::commands`. Running a selected
//! command emits `Message`s handled by shell/domain update. Stacked by
//! [`super::root_surface`] above domain content when the palette is open.
//!
//! Related: [`super::menus`] for library-switcher chrome, [`super::context_menu`]
//! for pointer-driven actions on the same surfaces.

use crate::shell::commands::{command_matches, library_commands, CommandDanger};
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{column, row};

/// Full-window transparent click-catcher that closes the palette on press.
pub(crate) fn command_palette_capture_layer<'a>() -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseCommandPalette),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Centered searchable command list for the active command surface.
///
/// Filters visible/enabled commands by `command_palette_query`, highlights the
/// selected index, and sizes the panel from layout metrics clamped to the
/// current viewport.
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

    // Slightly brighter than pure secondary so category/shortcut metadata stays
    // readable on the raised menu panel without competing with the title.
    let meta_color = mix_color(tokens.text_secondary, tokens.text_primary, 0.28);
    let muted_meta = mix_color(tokens.text_secondary, tokens.surface_raised, 0.15);

    let input = text_input("Search commands…", &app.chrome.command_palette_query)
        .on_input(Message::CommandPaletteQueryChanged)
        .on_submit(Message::CommandPaletteRunSelected)
        .padding([Spacing::SM + 1.0, Spacing::MD])
        .size(FontSize::CONTROL)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
        .width(Length::Fill);

    let mut list = column![].spacing(Spacing::XS).width(Length::Fill);
    for (index, command) in commands.iter().enumerate() {
        let selected = index == app.chrome.command_palette_selected_index;
        let label_color = if selected {
            tokens.text_primary
        } else if command.spec.danger == CommandDanger::Destructive {
            tokens.error
        } else {
            tokens.text_primary
        };
        let detail_color = if selected {
            mix_color(tokens.text_secondary, tokens.text_primary, 0.45)
        } else {
            meta_color
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
        let category_line = {
            let category = command.spec.category.label();
            if target_label.is_empty() {
                category.to_owned()
            } else {
                format!("{category} · {target_label}")
            }
        };
        let row_content = row![
            column![
                text(command.spec.label)
                    .size(FontSize::CONTROL)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(label_color)
                    .wrapping(Wrapping::None),
                text(category_line)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(detail_color)
                    .wrapping(Wrapping::None),
            ]
            .spacing(
                app.layout()
                    .metric("CommandPalette", "metadata_spacing", 2.0)
            )
            .width(Length::Fill),
            text(shortcut)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(if selected {
                    mix_color(tokens.accent, tokens.text_primary, 0.25)
                } else {
                    muted_meta
                })
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center);
        list = list.push(
            button(row_content)
                .padding([Spacing::SM, Spacing::MD])
                .width(Length::Fill)
                .on_press(Message::CommandPaletteRun(command.spec.id))
                .style(move |_, status| {
                    // Selected rows use MenuItem + Active so they share the
                    // panel’s selected fill instead of MenuButton chrome.
                    let status = if selected && matches!(status, iced::widget::button::Status::Active)
                    {
                        iced::widget::button::Status::Hovered
                    } else {
                        status
                    };
                    button_style(tokens, Class::MenuItem, status)
                }),
        );
    }
    if commands.is_empty() {
        list = list.push(
            container(
                column![
                    text("No matching commands")
                        .size(FontSize::CONTROL)
                        .font(ui_font(FontWeight::MEDIUM))
                        .color(tokens.text_primary),
                    text("Try a different search, or press Esc to close")
                        .size(FontSize::SM)
                        .font(ui_font(FontWeight::REGULAR))
                        .color(meta_color),
                ]
                .spacing(Spacing::XS),
            )
            .padding([Spacing::LG, Spacing::MD])
            .width(Length::Fill)
            .center_x(Length::Fill),
        );
    }

    let list_scroll = scrollable(list)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(tokens.primitives.scrollbar_width)
                .scroller_width(tokens.primitives.scrollbar_scroller_width)
                .anchor(Anchor::Start),
        ))
        .height(list_height)
        .width(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::MenuPanel, status));

    let hint = text("↑↓ navigate  ·  Enter run  ·  Esc close")
        .size(FontSize::SM)
        .font(ui_font(FontWeight::REGULAR))
        .color(muted_meta);

    let panel = column![
        text("Command Palette")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary),
        input,
        list_scroll,
        hint,
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
