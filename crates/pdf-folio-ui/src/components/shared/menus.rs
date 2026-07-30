//! # Application menus
//!
//! Menu bar / library switcher structures and item builders shared by the shell.

use crate::*;
use iced::widget::image;
use iced::widget::{button, column, row, stack, Svg};
use iced::ContentFit;

const OVERFLOW_HORIZONTAL_SVG: &[u8] =
    include_bytes!("../../../assets/icons/overflow-horizontal.svg");
const OVERFLOW_VERTICAL_SVG: &[u8] = include_bytes!("../../../assets/icons/overflow-vertical.svg");

/// Library switcher panel listing vault profiles with previews and actions.
pub(crate) fn view_library_switcher(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let card_width = app.layout().metric("LibrarySwitcher", "card_width", 230.0);
    let card_height = app.layout().metric("LibrarySwitcher", "card_height", 362.0);
    let mut cards = Vec::new();
    for profile in &app.libraries.profiles {
        cards.push(library_profile_card(
            app,
            profile,
            tokens,
            card_width,
            card_height,
        ));
    }
    cards.push(new_library_card(
        app.layout(),
        tokens,
        card_width,
        card_height,
    ));

    let mut grid = column![]
        .spacing(Spacing::MD)
        .align_x(iced::Alignment::Center);
    let mut current_row = row![].spacing(Spacing::MD).align_y(iced::Alignment::Center);
    for (index, card) in cards.into_iter().enumerate() {
        if index > 0 && index % 3 == 0 {
            grid = grid.push(current_row);
            current_row = row![].spacing(Spacing::MD).align_y(iced::Alignment::Center);
        }
        current_row = current_row.push(card);
    }
    if !app.libraries.profiles.is_empty() {
        grid = grid.push(current_row);
    }

    let content = column![
        text("Choose a Library")
            .size(app.layout().metric("LibrarySwitcher", "heading_size", 34.0) as u32)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text("Keep separate PDF collections, reading state, folders, and imports.")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
        grid,
        toolbar_button("Back to Library", tokens).on_press(Message::CloseLibrarySwitcher),
    ]
    .spacing(Spacing::LG)
    .align_x(iced::Alignment::Center);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .padding(Spacing::XL)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into()
}

fn library_profile_card<'a>(
    app: &'a PDFolioApp,
    profile: &'a LibraryProfile,
    tokens: ThemeTokens,
    width: f32,
    height: f32,
) -> Element<'a, Message> {
    let active = profile.id == app.libraries.active_library_id;
    let open_message = if active {
        Message::CloseLibrarySwitcher
    } else {
        Message::SelectLibrary(profile.id.clone())
    };
    let preview = app.libraries.previews.get(&profile.id);
    let total_entries = preview.map_or(0, |preview| preview.total_entries);
    let content_width = width - Spacing::MD * 2.0;
    let title_size = app
        .layout()
        .metric("LibrarySwitcher", "card_title_size", 18.0) as u32;

    let body = column![
        container("").height(
            app.layout()
                .metric("LibrarySwitcher", "card_top_spacer", 12.0)
        ),
        library_preview_panel(app.layout(), preview, tokens),
        container("").height(Spacing::XS),
        container(
            column![
                text(truncate_for_width_with_font(
                    &profile.name,
                    content_width,
                    0.0,
                    title_size,
                ))
                .size(title_size)
                .font(display_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None)
                .width(Length::Fill),
                text(format_count(total_entries, "PDF"))
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(if active {
                        tokens.accent
                    } else {
                        tokens.text_secondary
                    })
                    .width(Length::Fill),
            ]
            .spacing(
                app.layout()
                    .metric("LibrarySwitcher", "card_title_spacing", 2.0)
            )
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(
            app.layout()
                .metric("LibrarySwitcher", "card_title_height", 38.0),
        )
        .align_y(iced::alignment::Vertical::Center),
    ]
    .spacing(0)
    .align_x(iced::Alignment::Start);

    let card = mouse_area(
        container(body)
            .width(width)
            .height(height)
            .padding(Spacing::MD)
            .style(move |_| {
                let mut style = container_style(tokens, Class::LibraryCard);
                if active {
                    let selected_style = tokens.class_styles[Class::LibraryCard.index()]
                        .resolve(ComponentState::Selected);
                    style = style.with_visual_override(selected_style);
                }
                style
            }),
    )
    .on_press(open_message);

    let overlay_gutter = app
        .layout()
        .metric("LibrarySwitcher", "card_overlay_gutter", 72.0);
    let menu_x = app.layout().metric("LibrarySwitcherMenu", "x", 7.0);
    let menu_y = app.layout().metric("LibrarySwitcherMenu", "y", 2.0);
    let menu_offset = app.layout().metric("LibrarySwitcherMenu", "offset", 6.0);
    let menu_down_shift = app
        .layout()
        .metric("LibrarySwitcherMenu", "down_shift", 4.0);

    let mut layered = stack![pin(card).y(overlay_gutter)]
        .width(width)
        .height(height + overlay_gutter);

    layered = layered.push(
        pin(library_card_menu_button(app.layout(), profile, tokens))
            .x(menu_x)
            .y(overlay_gutter + menu_y),
    );
    if app.libraries.open_menu_library_id.as_ref() == Some(&profile.id) {
        let menu_height = library_card_overflow_menu_height(app);
        layered = layered.push(
            pin(library_card_overflow_menu(app, profile, tokens))
                .x(menu_x - menu_offset)
                .y(overlay_gutter + menu_y - menu_height - menu_offset + menu_down_shift),
        );
    }

    layered.into()
}

fn new_library_card(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
    width: f32,
    height: f32,
) -> Element<'static, Message> {
    let create_action = column![
        text("+")
            .size(layout.metric("LibrarySwitcher", "create_icon_size", 48.0) as u32)
            .font(ui_font(FontWeight::REGULAR))
            .wrapping(Wrapping::None),
        text("Create New Library")
            .size(FontSize::CONTROL)
            .font(ui_font(FontWeight::SEMIBOLD))
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::SM)
    .align_x(iced::Alignment::Center);

    let body = column![
        container("").height(
            layout.metric("LibrarySwitcher", "card_top_spacer", 12.0)
                + layout.metric("LibrarySwitcher", "create_top_extra", 12.0),
        ),
        container(create_action)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .align_y(iced::alignment::Vertical::Center),
        container("").height(Spacing::LG),
    ]
    .spacing(0)
    .align_x(iced::Alignment::Center);

    let card = button(body)
        .width(width)
        .height(height)
        .padding(Spacing::MD)
        .style(move |_, status| {
            let mut style = button_style(tokens, Class::LibraryCard, status);
            match status {
                button::Status::Active => {
                    let inactive_style = tokens.class_styles[Class::LibraryCard.index()]
                        .resolve(ComponentState::Disabled);
                    style = style.with_visual_override(inactive_style);
                }
                button::Status::Hovered => {
                    let hovered_style = tokens.class_styles[Class::LibraryCard.index()]
                        .resolve(ComponentState::Hovered);
                    style = style.with_visual_override(hovered_style);
                }
                button::Status::Pressed => {
                    let pressed_style = tokens.class_styles[Class::LibraryCard.index()]
                        .resolve(ComponentState::Pressed);
                    style = style.with_visual_override(pressed_style);
                }
                button::Status::Disabled => {}
            }
            style
        })
        .on_press(Message::OpenCreateLibraryDialog);

    let overlay_gutter = layout.metric("LibrarySwitcher", "card_overlay_gutter", 72.0);
    stack![pin(card).y(overlay_gutter)]
        .width(width)
        .height(height + overlay_gutter)
        .into()
}

fn library_preview_panel<'a>(
    layout: &crate::style::AppLayoutTokens,
    preview: Option<&'a crate::library::registry::LibraryPreview>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let Some(preview) = preview else {
        return library_empty_preview_panel(layout, tokens);
    };
    if preview.thumbnails.is_empty() {
        return library_empty_preview_panel(layout, tokens);
    }

    let columns = layout.count("LibrarySwitcherPreview", "columns", 4);
    let rows = layout.count("LibrarySwitcherPreview", "rows", 3);
    let tile_width = layout.metric("LibrarySwitcherPreview", "tile_width", 48.0);
    let tile_height = layout.metric("LibrarySwitcherPreview", "tile_height", 77.0);
    let row_height = layout.metric("LibrarySwitcherPreview", "row_height", tile_height);
    let row_offset = layout.metric("LibrarySwitcherPreview", "row_offset", 5.0);
    let column_gap = layout.metric("LibrarySwitcherPreview", "column_gap", 5.0);
    let grid_width = tile_width * columns as f32 + column_gap * (columns as f32 - 1.0);
    let ellipsis_row_height = layout.metric("LibrarySwitcherPreview", "ellipsis_row_height", 25.0);

    let mut grid = column![].spacing(0).align_x(iced::Alignment::Center);
    let mut rendered_rows = 0;
    for (row_index, chunk) in preview.thumbnails.chunks(columns).take(rows).enumerate() {
        if row_index > 0 {
            grid = grid.push(container("").height(row_offset));
        }
        let mut row = row![].spacing(column_gap).align_y(iced::Alignment::Center);
        for thumbnail in chunk {
            row = row.push(library_preview_pdf_tile(layout, thumbnail, tokens));
        }
        for _ in chunk.len()..columns {
            row = row.push(container("").width(tile_width).height(tile_height));
        }
        grid = grid.push(
            container(row.width(Length::Fixed(grid_width)))
                .width(Length::Fill)
                .height(row_height)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        );
        rendered_rows += 1;
    }
    for _ in rendered_rows..rows {
        grid = grid.push(container("").width(Length::Fill).height(row_height));
    }
    if preview.total_entries > preview.thumbnails.len() {
        let mut row = row![].spacing(column_gap).align_y(iced::Alignment::Center);
        for _ in 0..columns {
            row = row.push(library_preview_column_ellipsis(layout, tokens));
        }
        grid = grid.push(
            container(row.width(Length::Fixed(grid_width)))
                .width(Length::Fill)
                .height(ellipsis_row_height)
                .center_x(Length::Fill)
                .align_y(iced::alignment::Vertical::Top),
        );
    }

    container(grid)
        .width(Length::Fill)
        .height(layout.metric("LibrarySwitcherPreview", "height", 280.0))
        .padding(layout.metric("LibrarySwitcherPreview", "panel_padding", 4.0))
        .center_x(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .style(move |_| container_style(tokens, Class::SidebarDetailRow))
        .into()
}

fn library_empty_preview_panel(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
) -> Element<'static, Message> {
    container(
        text("No PDFs")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
    )
    .width(Length::Fill)
    .height(layout.metric("LibrarySwitcherPreview", "height", 280.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

fn library_preview_pdf_tile<'a>(
    layout: &crate::style::AppLayoutTokens,
    thumbnail: &'a crate::library::registry::LibraryPreviewThumbnail,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let tile_width = layout.metric("LibrarySwitcherPreview", "tile_width", 48.0);
    let tile_height = layout.metric("LibrarySwitcherPreview", "tile_height", 77.0);
    let image_width = layout.metric("LibrarySwitcherPreview", "image_width", 38.0);
    let image_slot_height = layout.metric("LibrarySwitcherPreview", "image_slot_height", 49.0);
    let image_min_height = layout.metric("LibrarySwitcherPreview", "image_min_height", 28.0);
    let title_font_size = layout.metric("LibrarySwitcherPreview", "title_font_size", 8.0) as u32;
    let title_height = layout.metric("LibrarySwitcherPreview", "title_height", 22.0);
    let title_lines = layout.count("LibrarySwitcherPreview", "title_lines", 3);
    let title_width = tile_width - layout.metric("LibrarySwitcherPreview", "title_inset", 4.0);
    let image_height = (image_width * f32::from(thumbnail.height)
        / f32::from(thumbnail.width.max(1)))
    .clamp(image_min_height, image_slot_height);
    container(
        column![
            container(
                image(thumbnail.handle.clone())
                    .width(image_width)
                    .height(image_height)
                    .content_fit(ContentFit::Contain),
            )
            .width(tile_width)
            .height(image_slot_height)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .clip(true),
            text(wrap_preview_title(
                &thumbnail.title,
                title_width,
                title_font_size,
                title_lines,
            ))
            .size(title_font_size)
            .line_height(1.04)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::WordOrGlyph)
            .width(title_width)
            .height(title_height),
        ]
        .spacing(layout.metric("LibrarySwitcherPreview", "tile_spacing", 2.0)),
    )
    .width(tile_width)
    .height(tile_height)
    .padding(layout.metric("LibrarySwitcherPreview", "tile_padding", 2.0))
    .into()
}

fn wrap_preview_title(label: &str, width: f32, font_size: u32, max_lines: usize) -> String {
    const ELLIPSIS: &str = "...";

    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.is_empty() || max_lines == 0 {
        return String::new();
    }

    let approx_char_width = (font_size as f32 * 0.48).max(1.0);
    let max_chars = (width / approx_char_width)
        .floor()
        .max(ELLIPSIS.len() as f32) as usize;
    let mut remaining = label.as_str();
    let mut lines = Vec::new();

    for line_index in 0..max_lines {
        let remaining_chars = remaining.chars().count();
        if remaining_chars <= max_chars {
            lines.push(remaining.to_owned());
            break;
        }

        let last_line = line_index + 1 == max_lines;
        if last_line {
            let keep = max_chars.saturating_sub(ELLIPSIS.len()).max(1);
            let mut line: String = remaining.chars().take(keep).collect();
            line.push_str(ELLIPSIS);
            lines.push(line);
            break;
        }

        let candidate: String = remaining.chars().take(max_chars).collect();
        let split_at = candidate
            .char_indices()
            .rev()
            .find_map(|(index, character)| character.is_whitespace().then_some(index))
            .filter(|index| *index >= max_chars / 2)
            .unwrap_or_else(|| candidate.len());
        let (line, rest) = remaining.split_at(split_at);
        lines.push(line.trim().to_owned());
        remaining = rest.trim_start();
    }

    lines.join("\n")
}

fn library_preview_column_ellipsis(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
) -> Element<'static, Message> {
    let tile_width = layout.metric("LibrarySwitcherPreview", "tile_width", 48.0);
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(
        OVERFLOW_VERTICAL_SVG,
    ))
    .width(layout.metric("LibrarySwitcherPreview", "ellipsis_icon_width", 6.0))
    .height(layout.metric("LibrarySwitcherPreview", "ellipsis_icon_height", 34.0))
    .style(move |_, _| iced::widget::svg::Style {
        color: Some(with_alpha(tokens.text_secondary, 0.92)),
    });

    container(icon)
        .width(tile_width)
        .height(layout.metric("LibrarySwitcherPreview", "ellipsis_row_height", 25.0))
        .center_x(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

fn library_card_menu_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    profile: &'a LibraryProfile,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(
        OVERFLOW_HORIZONTAL_SVG,
    ))
    .width(layout.metric("LibrarySwitcherMenu", "icon_width", 18.0))
    .height(layout.metric("LibrarySwitcherMenu", "icon_height", 6.0))
    .style(move |_, _| iced::widget::svg::Style {
        color: Some(tokens.text_secondary),
    });

    button(
        container(icon)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill),
    )
    .width(layout.metric("LibrarySwitcherMenu", "button_width", 28.0))
    .height(layout.metric("LibrarySwitcherMenu", "button_height", 22.0))
    .padding(layout.metric("LibrarySwitcherMenu", "button_padding", 0.0))
    .style(move |_, status| button_style(tokens, Class::SidebarToggleButton, status))
    .on_press(Message::ToggleLibraryCardMenu(profile.id.clone()))
    .into()
}

fn library_card_overflow_menu<'a>(
    app: &'a PDFolioApp,
    profile: &'a LibraryProfile,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let delete_enabled = app.libraries.profiles.len() > 1;
    let item_height = app.layout().app_menu_item_height;
    let menu = column![
        library_card_menu_row(
            "Rename",
            true,
            Message::OpenRenameLibraryDialog(profile.id.clone()),
            tokens,
            item_height,
        ),
        library_card_menu_row(
            "Delete",
            delete_enabled,
            Message::RequestDeleteLibrary(profile.id.clone()),
            tokens,
            item_height,
        ),
    ]
    .spacing(0);

    container(menu)
        .width(app.layout().metric("LibrarySwitcherMenu", "width", 118.0))
        .padding([
            tokens.class_styles[Class::MenuPanel.index()]
                .layout
                .padding_y(Spacing::XS),
            tokens.class_styles[Class::MenuPanel.index()]
                .layout
                .padding_x(Spacing::XS),
        ])
        .style(move |_| container_style(tokens, Class::MenuPanel))
        .into()
}

fn library_card_overflow_menu_height(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let item_height = tokens.class_styles[Class::MenuItem.index()]
        .layout
        .height
        .unwrap_or(app.layout().app_menu_item_height);
    let panel_layout = tokens.class_styles[Class::MenuPanel.index()].layout;
    item_height * 2.0 + panel_layout.padding_y(Spacing::XS) * 2.0
}

fn library_card_menu_row<'a>(
    label: &'a str,
    enabled: bool,
    message: Message,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let item_layout = tokens.class_styles[Class::MenuItem.index()].layout;
    let item_text = tokens.class_styles[Class::MenuItem.index()].text;
    let state = if enabled {
        ComponentState::Normal
    } else {
        ComponentState::Disabled
    };
    let label_color = class_text_color(tokens, Class::MenuItem, state, tokens.text_primary);
    let content = row![text(label)
        .size(item_text.size.unwrap_or(FontSize::MD))
        .font(ui_font(item_text.weight.unwrap_or(FontWeight::REGULAR)))
        .color(label_color)
        .wrapping(Wrapping::None)
        .width(Length::Fill),]
    .align_y(iced::Alignment::Center);

    if enabled {
        button(content)
            .width(Length::Fill)
            .height(item_layout.height.unwrap_or(item_height))
            .padding([
                item_layout.padding_y(Spacing::XS),
                item_layout.padding_x(Spacing::MD),
            ])
            .style(move |_, status| button_style(tokens, Class::MenuItem, status))
            .on_press(message)
            .into()
    } else {
        container(content)
            .width(Length::Fill)
            .height(item_layout.height.unwrap_or(item_height))
            .padding([
                item_layout.padding_y(Spacing::XS),
                item_layout.padding_x(Spacing::MD),
            ])
            .style(move |_| {
                let disabled_style =
                    tokens.class_styles[Class::MenuItem.index()].resolve(ComponentState::Disabled);
                container_style(tokens, Class::MenuItem).with_visual_override(disabled_style)
            })
            .into()
    }
}

fn class_text_color(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
    fallback: Color,
) -> Color {
    tokens.class_styles[class.index()]
        .resolve(state)
        .text_color
        .unwrap_or(fallback)
}
