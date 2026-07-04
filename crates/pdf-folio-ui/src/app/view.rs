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
use crate::viewer::canvas::{ViewerCanvas, ViewerSelectionOverlay};
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

    if app.library.library_startup_loading {
        stack![shell, startup_library_loading_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.viewer.pending_document_open {
        stack![shell, loading_cursor_layer()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    }
}

const LIBRARY_SWITCHER_CARD_WIDTH: f32 = 230.0;
const LIBRARY_SWITCHER_CARD_HEIGHT: f32 = 362.0;
const LIBRARY_CARD_OVERLAY_GUTTER: f32 = 72.0;
const LIBRARY_CARD_TOP_SPACER: f32 = 12.0;
const LIBRARY_CARD_MENU_X: f32 = 7.0;
const LIBRARY_CARD_MENU_Y: f32 = 2.0;
const LIBRARY_CARD_MENU_OFFSET: f32 = 6.0;
const LIBRARY_CARD_MENU_DOWN_SHIFT: f32 = 4.0;
const LIBRARY_CARD_TITLE_HEIGHT: f32 = 38.0;
const LIBRARY_PREVIEW_COLUMNS: usize = 4;
const LIBRARY_PREVIEW_ROWS: usize = 3;
const LIBRARY_PREVIEW_HEIGHT: f32 = 280.0;
const LIBRARY_PREVIEW_TILE_WIDTH: f32 = 48.0;
const LIBRARY_PREVIEW_TILE_HEIGHT: f32 = 77.0;
const LIBRARY_PREVIEW_ROW_HEIGHT: f32 = LIBRARY_PREVIEW_TILE_HEIGHT;
const LIBRARY_PREVIEW_ROW_OFFSET: f32 = 5.0;
const LIBRARY_PREVIEW_ELLIPSIS_ROW_HEIGHT: f32 = 25.0;
const LIBRARY_PREVIEW_COLUMN_GAP: f32 = 5.0;
const LIBRARY_PREVIEW_GRID_WIDTH: f32 = LIBRARY_PREVIEW_TILE_WIDTH * 4.0
    + LIBRARY_PREVIEW_COLUMN_GAP * (LIBRARY_PREVIEW_COLUMNS as f32 - 1.0);
const LIBRARY_PREVIEW_PANEL_PADDING: f32 = 4.0;
const LIBRARY_PREVIEW_IMAGE_WIDTH: f32 = 38.0;
const LIBRARY_PREVIEW_IMAGE_SLOT_HEIGHT: f32 = 49.0;
const LIBRARY_PREVIEW_IMAGE_MIN_HEIGHT: f32 = 28.0;
const LIBRARY_PREVIEW_TITLE_FONT_SIZE: u32 = 8;
const LIBRARY_PREVIEW_TITLE_HEIGHT: f32 = 22.0;
const LIBRARY_PREVIEW_TITLE_LINES: usize = 3;

fn view_library_switcher(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let mut cards = Vec::new();
    for profile in &app.libraries.profiles {
        cards.push(library_profile_card(
            app,
            profile,
            tokens,
            LIBRARY_SWITCHER_CARD_WIDTH,
            LIBRARY_SWITCHER_CARD_HEIGHT,
        ));
    }
    cards.push(new_library_card(
        tokens,
        LIBRARY_SWITCHER_CARD_WIDTH,
        LIBRARY_SWITCHER_CARD_HEIGHT,
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
            .size(34)
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
    let title_size = 18;

    let body = column![
        container("").height(LIBRARY_CARD_TOP_SPACER),
        library_preview_panel(preview, tokens),
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
            .spacing(2.0)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(LIBRARY_CARD_TITLE_HEIGHT)
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
                    style.border.color = tokens.accent;
                    style.border.width = 1.5;
                }
                style
            }),
    )
    .on_press(open_message);

    let mut layered = stack![pin(card).y(LIBRARY_CARD_OVERLAY_GUTTER)]
        .width(width)
        .height(height + LIBRARY_CARD_OVERLAY_GUTTER);

    layered = layered.push(
        pin(library_card_menu_button(profile, tokens))
            .x(LIBRARY_CARD_MENU_X)
            .y(LIBRARY_CARD_OVERLAY_GUTTER + LIBRARY_CARD_MENU_Y),
    );
    if app.libraries.open_menu_library_id.as_ref() == Some(&profile.id) {
        let menu_height = library_card_overflow_menu_height(app);
        layered = layered.push(
            pin(library_card_overflow_menu(app, profile, tokens))
                .x(LIBRARY_CARD_MENU_X - LIBRARY_CARD_MENU_OFFSET)
                .y(LIBRARY_CARD_OVERLAY_GUTTER + LIBRARY_CARD_MENU_Y
                    - menu_height
                    - LIBRARY_CARD_MENU_OFFSET
                    + LIBRARY_CARD_MENU_DOWN_SHIFT),
        );
    }

    layered.into()
}

fn new_library_card(tokens: ThemeTokens, width: f32, height: f32) -> Element<'static, Message> {
    let create_action = column![
        text("+")
            .size(48)
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
        container("").height(LIBRARY_CARD_TOP_SPACER + 12.0),
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
                    style.background = Some(iced::Background::Color(with_alpha(
                        tokens.surface_raised,
                        0.42,
                    )));
                    style.text_color = with_alpha(tokens.text_primary, 0.58);
                    style.border.color = with_alpha(tokens.text_secondary, 0.34);
                    style.border.width = 1.0;
                }
                button::Status::Hovered => {
                    style.background = Some(iced::Background::Color(tokens.surface_raised));
                    style.text_color = tokens.text_primary;
                    style.border.color = tokens.accent;
                    style.border.width = 1.5;
                }
                button::Status::Pressed => {
                    style.background = Some(iced::Background::Color(mix_color(
                        tokens.surface_raised,
                        tokens.accent,
                        0.16,
                    )));
                    style.text_color = tokens.text_primary;
                    style.border.color = tokens.accent;
                    style.border.width = 1.5;
                }
                button::Status::Disabled => {}
            }
            style
        })
        .on_press(Message::OpenCreateLibraryDialog);

    stack![pin(card).y(LIBRARY_CARD_OVERLAY_GUTTER)]
        .width(width)
        .height(height + LIBRARY_CARD_OVERLAY_GUTTER)
        .into()
}

fn library_preview_panel<'a>(
    preview: Option<&'a crate::app_libraries::LibraryPreview>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let Some(preview) = preview else {
        return library_empty_preview_panel(tokens);
    };
    if preview.thumbnails.is_empty() {
        return library_empty_preview_panel(tokens);
    }

    let mut grid = column![].spacing(0).align_x(iced::Alignment::Center);
    let mut rendered_rows = 0;
    for (row_index, chunk) in preview
        .thumbnails
        .chunks(LIBRARY_PREVIEW_COLUMNS)
        .take(LIBRARY_PREVIEW_ROWS)
        .enumerate()
    {
        if row_index > 0 {
            grid = grid.push(container("").height(LIBRARY_PREVIEW_ROW_OFFSET));
        }
        let mut row = row![]
            .spacing(LIBRARY_PREVIEW_COLUMN_GAP)
            .align_y(iced::Alignment::Center);
        for thumbnail in chunk {
            row = row.push(library_preview_pdf_tile(thumbnail, tokens));
        }
        for _ in chunk.len()..LIBRARY_PREVIEW_COLUMNS {
            row = row.push(
                container("")
                    .width(LIBRARY_PREVIEW_TILE_WIDTH)
                    .height(LIBRARY_PREVIEW_TILE_HEIGHT),
            );
        }
        grid = grid.push(
            container(row.width(Length::Fixed(LIBRARY_PREVIEW_GRID_WIDTH)))
                .width(Length::Fill)
                .height(LIBRARY_PREVIEW_ROW_HEIGHT)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        );
        rendered_rows += 1;
    }
    for _ in rendered_rows..LIBRARY_PREVIEW_ROWS {
        grid = grid.push(
            container("")
                .width(Length::Fill)
                .height(LIBRARY_PREVIEW_ROW_HEIGHT),
        );
    }
    if preview.total_entries > preview.thumbnails.len() {
        let mut row = row![]
            .spacing(LIBRARY_PREVIEW_COLUMN_GAP)
            .align_y(iced::Alignment::Center);
        for _ in 0..LIBRARY_PREVIEW_COLUMNS {
            row = row.push(library_preview_column_ellipsis(tokens));
        }
        grid = grid.push(
            container(row.width(Length::Fixed(LIBRARY_PREVIEW_GRID_WIDTH)))
                .width(Length::Fill)
                .height(LIBRARY_PREVIEW_ELLIPSIS_ROW_HEIGHT)
                .center_x(Length::Fill)
                .align_y(iced::alignment::Vertical::Top),
        );
    }

    container(grid)
        .width(Length::Fill)
        .height(LIBRARY_PREVIEW_HEIGHT)
        .padding(LIBRARY_PREVIEW_PANEL_PADDING)
        .center_x(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .style(move |_| container_style(tokens, Class::SidebarDetailRow))
        .into()
}

fn library_empty_preview_panel(tokens: ThemeTokens) -> Element<'static, Message> {
    container(
        text("No PDFs")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
    )
    .width(Length::Fill)
    .height(LIBRARY_PREVIEW_HEIGHT)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

fn library_preview_pdf_tile<'a>(
    thumbnail: &'a crate::app_libraries::LibraryPreviewThumbnail,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let image_width = LIBRARY_PREVIEW_IMAGE_WIDTH;
    let image_height =
        (image_width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1))).clamp(
            LIBRARY_PREVIEW_IMAGE_MIN_HEIGHT,
            LIBRARY_PREVIEW_IMAGE_SLOT_HEIGHT,
        );
    container(
        column![
            container(
                image(thumbnail.handle.clone())
                    .width(image_width)
                    .height(image_height)
                    .content_fit(ContentFit::Contain),
            )
            .width(LIBRARY_PREVIEW_TILE_WIDTH)
            .height(LIBRARY_PREVIEW_IMAGE_SLOT_HEIGHT)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .clip(true),
            text(wrap_preview_title(
                &thumbnail.title,
                LIBRARY_PREVIEW_TILE_WIDTH - 4.0,
                LIBRARY_PREVIEW_TITLE_FONT_SIZE,
                LIBRARY_PREVIEW_TITLE_LINES,
            ))
            .size(LIBRARY_PREVIEW_TITLE_FONT_SIZE)
            .line_height(1.04)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::WordOrGlyph)
            .width(LIBRARY_PREVIEW_TILE_WIDTH - 4.0)
            .height(LIBRARY_PREVIEW_TITLE_HEIGHT),
        ]
        .spacing(2.0),
    )
    .width(LIBRARY_PREVIEW_TILE_WIDTH)
    .height(LIBRARY_PREVIEW_TILE_HEIGHT)
    .padding(2.0)
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

fn library_preview_column_ellipsis(tokens: ThemeTokens) -> Element<'static, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(
        OVERFLOW_VERTICAL_SVG,
    ))
    .width(6.0)
    .height(34.0)
    .style(move |_, _| iced::widget::svg::Style {
        color: Some(with_alpha(tokens.text_secondary, 0.92)),
    });

    container(icon)
        .width(LIBRARY_PREVIEW_TILE_WIDTH)
        .height(LIBRARY_PREVIEW_ELLIPSIS_ROW_HEIGHT)
        .center_x(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .into()
}

fn library_card_menu_button<'a>(
    profile: &'a LibraryProfile,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(
        OVERFLOW_HORIZONTAL_SVG,
    ))
    .width(18.0)
    .height(6.0)
    .style(move |_, _| iced::widget::svg::Style {
        color: Some(tokens.text_secondary),
    });

    button(
        container(icon)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill),
    )
    .width(28.0)
    .height(22.0)
    .padding(0)
    .style(move |_, status| {
        let mut style = button_style(tokens, Class::SidebarToggleButton, status);
        if matches!(status, button::Status::Active) {
            style.background = None;
            style.border.width = 0.0;
        } else {
            style.border.width = 0.0;
            style.background = Some(iced::Background::Color(with_alpha(
                tokens.surface_raised,
                0.72,
            )));
        }
        style.shadow = iced::Shadow::default();
        style
    })
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
        .width(118.0)
        .padding(Spacing::XS)
        .style(move |_| container_style(tokens, Class::MenuPanel))
        .into()
}

fn library_card_overflow_menu_height(app: &PDFolioApp) -> f32 {
    app.layout().app_menu_item_height * 2.0 + Spacing::XS * 2.0
}

fn library_card_menu_row<'a>(
    label: &'a str,
    enabled: bool,
    message: Message,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let label_color = if enabled {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let content = row![text(label)
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .color(label_color)
        .wrapping(Wrapping::None)
        .width(Length::Fill),]
    .align_y(iced::Alignment::Center);

    if enabled {
        button(content)
            .width(Length::Fill)
            .height(item_height)
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_, status| button_style(tokens, Class::MenuItem, status))
            .on_press(message)
            .into()
    } else {
        container(content)
            .width(Length::Fill)
            .height(item_height)
            .padding([Spacing::XS, Spacing::MD])
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
            .width(360.0)
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
        .width(Length::Fixed(140.0)),
        text(fraction)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(44.0)),
        viewer_find_icon_button(CHEVRON_UP_SVG, "Previous match", tokens)
            .on_press(Message::ViewerFindPrevious),
        viewer_find_icon_button(CHEVRON_DOWN_SVG, "Next match", tokens)
            .on_press(Message::ViewerFindNext),
        checkbox(app.viewer.viewer_find.highlight_all)
            .label("Highlight All")
            .on_toggle(Message::ViewerFindHighlightAllToggled)
            .size(16.0)
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_case)
            .label("Match Case")
            .on_toggle(Message::ViewerFindMatchCaseToggled)
            .size(16.0)
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_diacritics)
            .label("Match Diacritics")
            .on_toggle(Message::ViewerFindMatchDiacriticsToggled)
            .size(16.0)
            .text_size(FontSize::SM),
        icon_button("x", tokens)
            .on_press(Message::CloseViewerFind)
            .width(Length::Fixed(30.0))
            .height(Length::Fixed(30.0)),
    ]
    .spacing(Spacing::XS)
    .padding([Spacing::XS, Spacing::SM])
    .height(app.layout().viewer_find_bar_height)
    .align_y(iced::Alignment::Center);

    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(app.layout().viewer_find_bar_height))
        .style(move |_| {
            let mut style = container_style(tokens, Class::ViewerFindBar);
            let top_left = style.border.radius.top_left;
            style.border.radius = iced::border::Radius {
                top_left,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            };
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 18.0,
            };
            style
        })
        .into()
}

fn viewer_find_icon_button<'a>(
    icon: &'static [u8],
    label: &'static str,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        tooltip(
            container(
                Svg::new(iced::widget::svg::Handle::from_memory(icon))
                    .width(16.0)
                    .height(16.0)
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
    .width(Length::Fixed(30.0))
    .height(Length::Fixed(30.0))
    .padding(0)
    .style(move |_, status| crate::style::button_style(tokens, Class::ViewerFindButton, status))
}

fn loading_cursor_layer() -> Element<'static, Message> {
    mouse_area(container("").width(Length::Fill).height(Length::Fill))
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
            .width(460.0)
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
                .width(Length::Fixed(32.0)),
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
        viewer_library_back_button().on_press(Message::BackToLibrary),
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
        .spacing(Spacing::SM)
        .padding([Spacing::SM, Spacing::MD])
        .height(app.layout().toolbar_height)
        .align_y(iced::Alignment::Center);

    container(toolbar)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::ViewerToolbar))
        .into()
}

fn viewer_library_back_button<'a>() -> iced::widget::Button<'a, Message> {
    let brown = Color::from_rgb8(185, 156, 120);
    let bright_brown = Color::from_rgb8(212, 168, 83);
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(CHEVRON_LEFT_SVG))
        .width(16.0)
        .height(16.0)
        .style(move |_, status| iced::widget::svg::Style {
            color: Some(match status {
                iced::widget::svg::Status::Hovered => bright_brown,
                _ => brown,
            }),
        });
    let label = text("Library")
        .size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .wrapping(Wrapping::None);

    button(
        row![icon, label]
            .spacing(Spacing::XS)
            .align_y(iced::Alignment::Center),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| transparent_brown_toolbar_button_style(brown, bright_brown, status))
}

fn transparent_brown_toolbar_button_style(
    brown: Color,
    bright_brown: Color,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let text_color = match status {
        iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed => {
            bright_brown
        }
        _ => brown,
    };

    iced::widget::button::Style {
        background: None,
        text_color,
        border: iced::Border {
            width: 0.0,
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.0),
            radius: iced::border::Radius::from(4.0),
        },
        ..iced::widget::button::Style::default()
    }
}

fn viewer_page_control<'a>(
    app: &'a PDFolioApp,
    current_page: u16,
    page_count: u16,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let numerator: Element<'a, Message> = if app.viewer.page_input_editing {
        text_input("", &app.viewer.jump_input)
            .id(iced::widget::Id::new(PAGE_INPUT_ID))
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .padding([Spacing::XS, Spacing::SM])
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .width(Length::Fixed(app.layout().viewer_page_number_width))
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
            .into()
    } else {
        mouse_area(
            container(
                text(current_page.to_string())
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_secondary)
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
        viewer_page_chevron_button(CHEVRON_LEFT_SVG, tokens)
            .on_press(Message::PreviousPage)
            .width(Length::Fixed(app.layout().viewer_page_chevron_size))
            .height(Length::Fixed(app.layout().viewer_page_chevron_size)),
        numerator,
        text(format!("/ {page_count}"))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
        viewer_page_chevron_button(CHEVRON_RIGHT_SVG, tokens)
            .on_press(Message::NextPage)
            .width(Length::Fixed(app.layout().viewer_page_chevron_size))
            .height(Length::Fixed(app.layout().viewer_page_chevron_size)),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center)
    .into()
}

fn viewer_page_chevron_button<'a>(
    icon: &'static [u8],
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(16.0)
        .height(16.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });

    button(container(icon).center(Length::Fill))
        .padding(0)
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
    let visible = truncate_for_width_with_font(title, width, 0.0, FontSize::MD);
    let is_truncated = visible != title;
    let label = text(visible)
        .size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .color(tokens.text_primary)
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
                .size(FontSize::SM)
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        )
        .padding(Spacing::SM)
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
    text(truncate_for_width_with_font(
        &label,
        width,
        0.0,
        FontSize::SM,
    ))
    .size(FontSize::SM)
    .font(ui_font(FontWeight::MEDIUM))
    .color(tokens.text_secondary)
    .wrapping(Wrapping::None)
    .width(Length::Fill)
    .into()
}

fn viewer_toolbar_title_width(app: &PDFolioApp) -> f32 {
    let selection_reserve = if app.viewer.viewer_text_selection.is_some() {
        app.layout().viewer_toolbar_selection_width + 2.0 * (76.0 + Spacing::SM)
    } else {
        0.0
    };
    let chrome_reserve = 470.0 + selection_reserve;
    (app.viewer.viewport_width - chrome_reserve).clamp(
        app.layout().viewer_toolbar_title_min_width,
        app.layout().viewer_toolbar_title_max_width,
    )
}

fn viewer_zoom_menu_x(app: &PDFolioApp) -> f32 {
    const VIEWER_LIBRARY_BUTTON_WIDTH: f32 = 70.0;
    const VIEWER_OPEN_BUTTON_WIDTH: f32 = 87.0;
    const VIEWER_ZOOM_STEP_BUTTON_WIDTH: f32 = 30.0;

    let zoom_control_right = Spacing::MD
        + VIEWER_LIBRARY_BUTTON_WIDTH
        + Spacing::SM
        + VIEWER_OPEN_BUTTON_WIDTH
        + Spacing::SM
        + viewer_toolbar_title_width(app)
        + Spacing::SM
        + app.layout().viewer_page_control_width
        + Spacing::SM
        + VIEWER_ZOOM_STEP_BUTTON_WIDTH
        + Spacing::SM
        + app.layout().viewer_zoom_control_width;

    (zoom_control_right - app.layout().viewer_zoom_menu_width).max(Spacing::MD)
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
