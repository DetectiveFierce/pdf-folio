//! App shell and viewer-surface rendering.

use crate::app_context_menu::{context_menu_capture_layer, view_context_menu_dropdown};
use crate::library::view::{
    chevron_button, floating_folder_drag_preview, floating_library_drag_preview,
    view_confirmation_dialog, view_create_folder_dialog, view_library,
    view_library_move_picker_dialog, view_raindrop_connect_dialog, view_raindrop_import_dialog,
    view_raindrop_import_progress_dialog,
};
use crate::menu::{
    app_menu_bar_height, app_menu_capture_layer, selection_menu_capture_layer, view_app_menu_bar,
    view_app_menu_dropdown, view_selection_menu_dropdown,
};
use crate::viewer::canvas::{HistoryRestoreSpinner, ViewerCanvas, ViewerSelectionOverlay};
use crate::viewer::outline::{view_jump_dialog, view_sidebar};
use crate::viewer::zoom::{zoom_control, zoom_menu};
use crate::*;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{canvas, column, row, stack};
use std::time::Duration;

const OVERFLOW_HORIZONTAL_SVG: &[u8] = include_bytes!("../../assets/icons/overflow-horizontal.svg");
const OVERFLOW_VERTICAL_SVG: &[u8] = include_bytes!("../../assets/icons/overflow-vertical.svg");

pub(crate) fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let base_content: Element<'_, Message> = if app.mode == AppMode::LibrarySwitcher {
        view_library_switcher(app, tokens)
    } else if app.mode == AppMode::Viewer && app.viewer.doc.is_some() {
        let sidebar: Element<'_, Message> = if app.viewer.toc_open {
            view_sidebar(app).into()
        } else {
            container("").width(Length::Shrink).into()
        };

        let content_size = app.viewer_content_size(app.viewer.viewer_viewport_width);
        let viewer = canvas(ViewerCanvas { app })
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let selection_overlay = canvas(ViewerSelectionOverlay { app })
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let viewer_content = stack![viewer, selection_overlay]
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let viewer_scroll = scrollable(viewer_content)
            .id(Id::new(VIEWER_SCROLLABLE_ID))
            .direction(Direction::Both {
                vertical: Scrollbar::default(),
                horizontal: Scrollbar::default(),
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_, status| scrollable_style(tokens, Class::ViewerCanvas, status))
            .on_scroll(|viewport| {
                let offset = viewport.absolute_offset();
                let bounds = viewport.bounds();
                Message::ViewportChanged {
                    horizontal_offset: offset.x,
                    scroll_offset: offset.y,
                    width: bounds.width,
                    height: bounds.height,
                }
            });
        let mut viewer_stack = stack![viewer_scroll]
            .width(Length::Fill)
            .height(Length::Fill);
        if !app.viewer.toc_open {
            viewer_stack = viewer_stack.push(
                pin(viewer_floating_sidebar_toggle(tokens))
                    .x(Spacing::SM)
                    .y(Spacing::SM),
            );
        }
        if app.viewer.viewer_find.open {
            let find_width = app
                .layout()
                .viewer_find_bar_width
                .min((app.viewer.viewer_viewport_width - Spacing::MD * 2.0).max(320.0));
            viewer_stack = viewer_stack.push(viewer_find_anchor(app, tokens, find_width));
        }
        let mut main = column![].spacing(0);
        if let Some(error) = app.viewer.document_error.as_deref() {
            main = main.push(dismissible_error_banner(
                error,
                tokens,
                app.layout(),
                Message::DismissDocumentError,
            ));
        }
        if app.viewer.jump_dialog_open {
            main = main.push(view_jump_dialog(app));
        }
        main = main.push(viewer_stack);

        column![
            view_app_menu_bar(app),
            view_viewer_toolbar(app),
            row![sidebar, main.width(Length::Fill)].height(Length::Fill)
        ]
        .into()
    } else {
        let mut library_shell = column![view_app_menu_bar(app)];
        if let Some(error) = app.viewer.document_error.as_deref() {
            library_shell = library_shell.push(dismissible_error_banner(
                error,
                tokens,
                app.layout(),
                Message::DismissDocumentError,
            ));
        }
        library_shell.push(view_library(app)).into()
    };

    let menu_content = if app.chrome.open_app_menu.is_some() {
        stack![
            base_content,
            app_menu_capture_layer(app),
            view_app_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.chrome.open_selection_menu.is_some() {
        stack![
            base_content,
            selection_menu_capture_layer(app),
            view_selection_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.viewer.zoom_menu_open {
        stack![
            base_content,
            zoom_menu_capture_layer(app),
            view_zoom_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.chrome.open_context_menu.is_some() {
        stack![
            base_content,
            context_menu_capture_layer(app),
            view_context_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        base_content
    };

    let content = if app.libraries.name_dialog.is_some() {
        stack![menu_content, view_library_name_dialog(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.chrome.pending_confirmation.is_some() {
        stack![menu_content, view_confirmation_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.create_folder_dialog_open {
        stack![menu_content, view_create_folder_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.move_picker.is_some() {
        stack![menu_content, view_library_move_picker_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_connect_dialog_open {
        stack![menu_content, view_raindrop_connect_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_import_dialog_open {
        stack![menu_content, view_raindrop_import_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_import_progress.is_some() {
        stack![menu_content, view_raindrop_import_progress_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_folder_drag_preview(app, tokens) {
        stack![menu_content, floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_library_drag_preview(app, tokens) {
        stack![menu_content, floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        menu_content
    };

    let shell: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into();

    let shell = if app.library.library_startup_loading {
        stack![shell, startup_library_loading_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.viewer.pending_document_open {
        stack![shell, document_loading_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    };

    if app.library.library_history_restore_started_at.is_some() {
        stack![shell, history_restore_spinner_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    }
}

fn view_library_switcher(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
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
    preview: Option<&'a crate::app_libraries::LibraryPreview>,
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
    thumbnail: &'a crate::app_libraries::LibraryPreviewThumbnail,
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

fn view_library_name_dialog(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(dialog) = app.libraries.name_dialog.as_ref() else {
        return container("").into();
    };
    let (title, confirm_label) = match dialog {
        LibraryNameDialog::Create => ("Create Library", "Create"),
        LibraryNameDialog::Rename(_) => ("Rename Library", "Rename"),
    };
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text_input("Library name", &app.libraries.new_library_name)
            .id(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID))
            .on_input(Message::NewLibraryNameChanged)
            .on_submit(Message::ConfirmLibraryNameDialog)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelLibraryNameDialog),
            container("").width(Length::Fill),
            toolbar_button(confirm_label, tokens).on_press(Message::ConfirmLibraryNameDialog),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("LibraryNameDialog", "width", 360.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

fn viewer_find_anchor(app: &PDFolioApp, tokens: ThemeTokens, width: f32) -> Element<'_, Message> {
    container(view_viewer_find_bar(app, tokens, width))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

fn view_viewer_find_bar(app: &PDFolioApp, tokens: ThemeTokens, width: f32) -> Element<'_, Message> {
    let current = app.viewer.viewer_find.selected.map_or(0, |index| index + 1);
    let total = app.viewer.viewer_find.matches.len();
    let fraction = format!("{current}/{total}");

    let content = row![
        search_input_with_class(
            "Find in Text",
            &app.viewer.viewer_find.query,
            tokens,
            Class::ViewerFindInput,
            Message::ViewerFindQueryChanged,
        )
        .id(Id::new(VIEWER_FIND_INPUT_ID))
        .on_submit(Message::ViewerFindNext)
        .width(Length::Fixed(app.layout().metric(
            "ViewerFindBar",
            "input_width",
            140.0
        ),)),
        text(fraction)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "counter_width",
                44.0
            ),)),
        viewer_find_icon_button(app.layout(), CHEVRON_UP_SVG, "Previous match", tokens)
            .on_press(Message::ViewerFindPrevious),
        viewer_find_icon_button(app.layout(), CHEVRON_DOWN_SVG, "Next match", tokens)
            .on_press(Message::ViewerFindNext),
        checkbox(app.viewer.viewer_find.highlight_all)
            .label("Highlight All")
            .on_toggle(Message::ViewerFindHighlightAllToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_case)
            .label("Match Case")
            .on_toggle(Message::ViewerFindMatchCaseToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_diacritics)
            .label("Match Diacritics")
            .on_toggle(Message::ViewerFindMatchDiacriticsToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        icon_button("x", tokens)
            .on_press(Message::CloseViewerFind)
            .width(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "button_size",
                30.0
            ),))
            .height(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "button_size",
                30.0
            ),)),
    ]
    .spacing(Spacing::XS)
    .padding([Spacing::XS, Spacing::SM])
    .height(app.layout().viewer_find_bar_height)
    .align_y(iced::Alignment::Center);

    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(app.layout().viewer_find_bar_height))
        .style(move |_| container_style(tokens, Class::ViewerFindBar))
        .into()
}

fn viewer_find_icon_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    icon: &'static [u8],
    label: &'static str,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        tooltip(
            container(
                Svg::new(iced::widget::svg::Handle::from_memory(icon))
                    .width(layout.metric("ViewerFindBar", "icon_size", 16.0))
                    .height(layout.metric("ViewerFindBar", "icon_size", 16.0))
                    .style(move |_, _| iced::widget::svg::Style {
                        color: Some(tokens.text_primary),
                    }),
            )
            .center(Length::Fill),
            label,
            tooltip::Position::Top,
        )
        .style(move |_| container_style(tokens, Class::Tooltip)),
    )
    .width(Length::Fixed(layout.metric(
        "ViewerFindBar",
        "button_size",
        30.0,
    )))
    .height(Length::Fixed(layout.metric(
        "ViewerFindBar",
        "button_size",
        30.0,
    )))
    .padding(layout.metric("ViewerFindBar", "button_padding", 0.0))
    .style(move |_, status| crate::style::button_style(tokens, Class::ViewerFindButton, status))
}

fn history_restore_spinner_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(started_at) = app.library.library_history_restore_started_at else {
        return container("").into();
    };
    let spinner_size = app.layout().metric("HistoryRestoreSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));
    let mut background = tokens.background;
    background.a = 0.54;

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(background)),
                ..iced::widget::container::Style::default()
            }),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

fn document_loading_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let started_at = app
        .viewer
        .document_open_started_at
        .unwrap_or(app.library.animation_now);
    let spinner_size = app.layout().metric("DocumentLoadingSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

fn startup_library_loading_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let status = app
        .library
        .raindrop_rollback_recovery_status
        .as_deref()
        .unwrap_or("Preparing library...");
    mouse_area(
        container(
            container(
                column![
                    text("Restoring library")
                        .size(FontSize::HEADING)
                        .font(display_font(FontWeight::MEDIUM))
                        .color(tokens.text_primary),
                    text(status).size(FontSize::MD).color(tokens.text_secondary),
                    container(progress_bar(0.42, tokens)).width(Length::Fill),
                ]
                .spacing(Spacing::MD)
                .padding(Spacing::LG),
            )
            .width(app.layout().metric("StartupLoadingDialog", "width", 460.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

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

fn view_viewer_toolbar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let page_count = app.viewer.doc.as_ref().map_or(0, |doc| doc.page_count());
    let current_page = if page_count == 0 {
        0
    } else {
        app.current_page().saturating_add(1).min(page_count)
    };
    let document_title = app
        .viewer
        .doc
        .as_ref()
        .and_then(|doc| doc.path().file_name())
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("Open PDF");
    let theme_label = match app.appearance.theme {
        AppTheme::Light => "Dark",
        AppTheme::Dark => "Light",
    };
    let title_width = viewer_toolbar_title_width(app);

    let mut toolbar = row![
        viewer_library_back_button(app.layout(), tokens).on_press(Message::BackToLibrary),
        toolbar_button("Open PDF", tokens).on_press(Message::OpenFileDialog),
        viewer_toolbar_title(document_title, title_width, tokens),
        viewer_page_control(app, current_page, page_count, tokens),
        icon_button("-", tokens).on_press(Message::ZoomOut),
        zoom_control(app, tokens),
        icon_button("+", tokens).on_press(Message::ZoomIn),
    ];

    if let Some(selection) = app.viewer.viewer_text_selection {
        let (start, end) = selection.ordered();
        let label = if start.page == end.page {
            let count = end.char_index.saturating_sub(start.char_index) + 1;
            format!("{count} char{} selected", if count == 1 { "" } else { "s" })
        } else {
            format!("{} pages selected", end.page.saturating_sub(start.page) + 1)
        };
        toolbar = toolbar
            .push(viewer_toolbar_status_label(
                label,
                app.layout().viewer_toolbar_selection_width,
                tokens,
            ))
            .push(toolbar_button("Copy", tokens).on_press(Message::CopyViewerTextSelection))
            .push(toolbar_button("Clear", tokens).on_press(Message::ClearViewerTextSelection));
    }

    let toolbar = toolbar
        .push(toolbar_button(theme_label, tokens).on_press(Message::ThemeToggled))
        .spacing(toolbar_layout.spacing.unwrap_or(Spacing::SM))
        .padding([
            toolbar_layout.padding_y(Spacing::SM),
            toolbar_layout.padding_x(Spacing::MD),
        ])
        .height(toolbar_layout.height.unwrap_or(app.layout().toolbar_height))
        .align_y(iced::Alignment::Center);

    container(toolbar)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::ViewerToolbar))
        .into()
}

fn viewer_library_back_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let button_layout = tokens.class_styles[Class::ViewerToolbarButton.index()].layout;
    let button_text = tokens.class_styles[Class::ViewerToolbarButton.index()].text;
    let text_color = class_text_color(
        tokens,
        Class::ViewerToolbarButton,
        ComponentState::Normal,
        tokens.text_secondary,
    );
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(CHEVRON_LEFT_SVG))
        .width(layout.metric("ViewerToolbarChrome", "icon_size", 16.0))
        .height(layout.metric("ViewerToolbarChrome", "icon_size", 16.0))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(text_color),
        });
    let label = text("Library")
        .size(button_text.size.unwrap_or(FontSize::MD))
        .font(ui_font(button_text.weight.unwrap_or(FontWeight::MEDIUM)))
        .color(text_color)
        .wrapping(Wrapping::None);

    button(
        row![icon, label]
            .spacing(button_layout.spacing.unwrap_or(Spacing::XS))
            .align_y(iced::Alignment::Center),
    )
    .padding([
        button_layout.padding_y(Spacing::SM),
        button_layout.padding_x(Spacing::LG),
    ])
    .style(move |_, status| crate::style::button_style(tokens, Class::ViewerToolbarButton, status))
}

fn viewer_page_control<'a>(
    app: &'a PDFolioApp,
    current_page: u16,
    page_count: u16,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let control_layout = tokens.class_styles[Class::ViewerPageControl.index()].layout;
    let control_text = tokens.class_styles[Class::ViewerPageControl.index()].text;
    let control_color = class_text_color(
        tokens,
        Class::ViewerPageControl,
        ComponentState::Normal,
        tokens.text_secondary,
    );
    let numerator: Element<'a, Message> = if app.viewer.page_input_editing {
        text_input("", &app.viewer.jump_input)
            .id(iced::widget::Id::new(PAGE_INPUT_ID))
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .padding([
                control_layout.padding_y(Spacing::XS),
                control_layout.padding_x(Spacing::SM),
            ])
            .size(control_text.size.unwrap_or(FontSize::MD))
            .font(ui_font(control_text.weight.unwrap_or(FontWeight::MEDIUM)))
            .width(Length::Fixed(app.layout().viewer_page_number_width))
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
            .into()
    } else {
        mouse_area(
            container(
                text(current_page.to_string())
                    .size(control_text.size.unwrap_or(FontSize::MD))
                    .font(ui_font(control_text.weight.unwrap_or(FontWeight::MEDIUM)))
                    .color(control_color)
                    .wrapping(Wrapping::None),
            )
            .width(Length::Fixed(app.layout().viewer_page_number_width))
            .height(Length::Fixed(app.layout().viewer_page_chevron_size))
            .center(Length::Fill),
        )
        .on_double_click(Message::StartPageInputEdit)
        .into()
    };

    row![
        viewer_page_chevron_button(app.layout(), CHEVRON_LEFT_SVG, tokens)
            .on_press(Message::PreviousPage)
            .width(Length::Fixed(app.layout().viewer_page_chevron_size))
            .height(Length::Fixed(app.layout().viewer_page_chevron_size)),
        numerator,
        text(format!("/ {page_count}"))
            .size(control_text.size.unwrap_or(FontSize::MD))
            .font(ui_font(control_text.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(control_color)
            .wrapping(Wrapping::None),
        viewer_page_chevron_button(app.layout(), CHEVRON_RIGHT_SVG, tokens)
            .on_press(Message::NextPage)
            .width(Length::Fixed(app.layout().viewer_page_chevron_size))
            .height(Length::Fixed(app.layout().viewer_page_chevron_size)),
    ]
    .spacing(control_layout.spacing.unwrap_or(Spacing::XS))
    .align_y(iced::Alignment::Center)
    .into()
}

fn viewer_page_chevron_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    icon: &'static [u8],
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let button_layout = tokens.class_styles[Class::ViewerToolbarButton.index()].layout;
    let icon_color = class_text_color(
        tokens,
        Class::ViewerToolbarButton,
        ComponentState::Normal,
        tokens.text_secondary,
    );
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(layout.metric("ViewerToolbarChrome", "icon_size", 16.0))
        .height(layout.metric("ViewerToolbarChrome", "icon_size", 16.0))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });

    button(container(icon).center(Length::Fill))
        .padding(
            button_layout
                .padding_x(0.0)
                .min(button_layout.padding_y(0.0)),
        )
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::ViewerToolbarButton, status)
        })
}

fn zoom_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseZoomMenu),
    )
    .y(app_menu_bar_height(app) + app.layout().toolbar_height)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_zoom_menu_dropdown(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    pin(zoom_menu(app, tokens))
        .x(viewer_zoom_menu_x(app))
        .y(app_menu_bar_height(app) + app.layout().toolbar_height)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn viewer_toolbar_title<'a>(
    title: &'a str,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let title_text = tokens.class_styles[Class::ViewerToolbarTitle.index()].text;
    let title_size = title_text.size.unwrap_or(FontSize::MD);
    let title_color = class_text_color(
        tokens,
        Class::ViewerToolbarTitle,
        ComponentState::Normal,
        tokens.text_primary,
    );
    let visible = truncate_for_width_with_font(title, width, 0.0, title_size);
    let is_truncated = visible != title;
    let label = text(visible)
        .size(title_size)
        .font(ui_font(title_text.weight.unwrap_or(FontWeight::MEDIUM)))
        .color(title_color)
        .wrapping(Wrapping::None)
        .width(Length::Fill);

    let content = container(label)
        .width(Length::Fixed(width))
        .center_y(Length::Shrink)
        .clip(true);

    if !is_truncated {
        return content.into();
    }

    tooltip(
        content,
        container(
            text(title)
                .size(title_size)
                .font(ui_font(title_text.weight.unwrap_or(FontWeight::MEDIUM)))
                .color(title_color)
                .wrapping(Wrapping::None),
        )
        .padding(
            tokens.class_styles[Class::Tooltip.index()]
                .layout
                .padding_x(Spacing::SM),
        )
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn viewer_toolbar_status_label<'a>(
    label: String,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let status_text = tokens.class_styles[Class::ViewerToolbarTitle.index()].text;
    let status_size = status_text.size.unwrap_or(FontSize::SM);
    let status_color = class_text_color(
        tokens,
        Class::ViewerToolbarTitle,
        ComponentState::Normal,
        tokens.text_secondary,
    );
    text(truncate_for_width_with_font(
        &label,
        width,
        0.0,
        status_size,
    ))
    .size(status_size)
    .font(ui_font(status_text.weight.unwrap_or(FontWeight::MEDIUM)))
    .color(status_color)
    .wrapping(Wrapping::None)
    .width(Length::Fill)
    .into()
}

fn viewer_toolbar_title_width(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let toolbar_spacing = toolbar_layout.spacing.unwrap_or(Spacing::SM);
    let selection_reserve = if app.viewer.viewer_text_selection.is_some() {
        app.layout().viewer_toolbar_selection_width
            + 2.0
                * (app
                    .layout()
                    .metric("ViewerToolbarChrome", "selection_button_width", 76.0)
                    + toolbar_spacing)
    } else {
        0.0
    };
    let chrome_reserve = app
        .layout()
        .metric("ViewerToolbarChrome", "fixed_width", 470.0)
        + selection_reserve;
    (app.viewer.viewport_width - chrome_reserve).clamp(
        app.layout().viewer_toolbar_title_min_width,
        app.layout().viewer_toolbar_title_max_width,
    )
}

fn viewer_zoom_menu_x(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let toolbar_spacing = toolbar_layout.spacing.unwrap_or(Spacing::SM);
    let zoom_control_right = toolbar_layout.padding_left(Spacing::MD)
        + app
            .layout()
            .metric("ViewerToolbarChrome", "library_button_width", 70.0)
        + toolbar_spacing
        + app
            .layout()
            .metric("ViewerToolbarChrome", "open_button_width", 87.0)
        + toolbar_spacing
        + viewer_toolbar_title_width(app)
        + toolbar_spacing
        + app.layout().viewer_page_control_width
        + toolbar_spacing
        + app
            .layout()
            .metric("ViewerToolbarChrome", "zoom_step_button_width", 30.0)
        + toolbar_spacing
        + app.layout().viewer_zoom_control_width;

    (zoom_control_right - app.layout().viewer_zoom_menu_width)
        .max(toolbar_layout.padding_left(Spacing::MD))
}

fn class_text_color(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
    fallback: iced::Color,
) -> iced::Color {
    tokens.class_styles[class.index()]
        .resolve(state)
        .text_color
        .unwrap_or(fallback)
}

fn viewer_floating_sidebar_toggle<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    chevron_button(
        CHEVRON_RIGHT_SVG,
        "Show Contents",
        Message::ToggleSidebar,
        tokens,
        true,
    )
}
