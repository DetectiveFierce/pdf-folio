//! Library view rendering and shared app shell view composition.

use crate::menu::{
    app_menu_capture_layer, library_sidebar_tab_label, selection_menu_capture_layer,
    view_app_menu_bar, view_app_menu_dropdown, view_selection_menu_dropdown,
};
use crate::viewer::canvas::{ViewerCanvas, ViewerSelectionOverlay};
use crate::viewer::outline::{view_jump_dialog, view_sidebar};
use crate::viewer::zoom::{zoom_control, zoom_menu, ZOOM_CONTROL_WIDTH, ZOOM_MENU_WIDTH};
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{canvas, column, row, stack};
use std::time::Duration;

const VIEWER_TOOLBAR_TITLE_MIN_WIDTH: f32 = 28.0;
const VIEWER_TOOLBAR_TITLE_MAX_WIDTH: f32 = 360.0;
const VIEWER_TOOLBAR_SELECTION_WIDTH: f32 = 116.0;
const VIEWER_FIND_BAR_WIDTH: f32 = 600.0;
const VIEWER_FIND_BAR_HEIGHT: f32 = 42.0;
const VIEWER_PAGE_NUMBER_WIDTH: f32 = 42.0;
const VIEWER_PAGE_CONTROL_WIDTH: f32 = 150.0;
const VIEWER_PAGE_CHEVRON_SIZE: f32 = 28.0;

pub(crate) fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let base_content: Element<'_, Message> = if app.mode == AppMode::Viewer && app.doc.is_some() {
        let sidebar: Element<'_, Message> = if app.toc_open {
            view_sidebar(app).into()
        } else {
            container("").width(Length::Shrink).into()
        };

        let viewer = canvas(ViewerCanvas { app })
            .width(Length::Fill)
            .height(Length::Fill);
        let selection_overlay = canvas(ViewerSelectionOverlay { app })
            .width(Length::Fill)
            .height(Length::Fill);
        let mut viewer_stack = stack![viewer, selection_overlay]
            .width(Length::Fill)
            .height(Length::Fill);
        if !app.toc_open {
            viewer_stack = viewer_stack.push(
                pin(viewer_floating_sidebar_toggle(tokens))
                    .x(Spacing::SM)
                    .y(Spacing::SM),
            );
        }
        if app.viewer_find.open {
            let find_width = VIEWER_FIND_BAR_WIDTH
                .min((app.viewer_viewport_width - Spacing::MD * 2.0).max(320.0));
            viewer_stack = viewer_stack.push(viewer_find_anchor(app, tokens, find_width));
        }
        let mut main = column![].spacing(0);
        if let Some(error) = app.document_error.as_deref() {
            main = main.push(dismissible_error_banner(
                error,
                tokens,
                Message::DismissDocumentError,
            ));
        }
        if app.jump_dialog_open {
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
        if let Some(error) = app.document_error.as_deref() {
            library_shell = library_shell.push(dismissible_error_banner(
                error,
                tokens,
                Message::DismissDocumentError,
            ));
        }
        library_shell.push(view_library(app)).into()
    };

    let menu_content = if app.open_app_menu.is_some() {
        stack![
            base_content,
            app_menu_capture_layer(app),
            view_app_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.open_selection_menu.is_some() {
        stack![
            base_content,
            selection_menu_capture_layer(app),
            view_selection_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.zoom_menu_open {
        stack![
            base_content,
            zoom_menu_capture_layer(app),
            view_zoom_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        base_content
    };

    let content = if app.pending_confirmation.is_some() {
        stack![menu_content, view_confirmation_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.create_folder_dialog_open {
        stack![menu_content, view_create_folder_dialog(app)]
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

    if app.pending_document_open {
        stack![shell, loading_cursor_layer()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    }
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
    let current = app.viewer_find.selected.map_or(0, |index| index + 1);
    let total = app.viewer_find.matches.len();
    let fraction = format!("{current}/{total}");

    let content = row![
        search_input_with_class(
            "Find in Text",
            &app.viewer_find.query,
            tokens,
            Class::SearchInput,
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
        checkbox(app.viewer_find.highlight_all)
            .label("Highlight All")
            .on_toggle(Message::ViewerFindHighlightAllToggled)
            .size(16.0)
            .text_size(FontSize::SM),
        checkbox(app.viewer_find.match_case)
            .label("Match Case")
            .on_toggle(Message::ViewerFindMatchCaseToggled)
            .size(16.0)
            .text_size(FontSize::SM),
        checkbox(app.viewer_find.match_diacritics)
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
    .height(VIEWER_FIND_BAR_HEIGHT)
    .align_y(iced::Alignment::Center);

    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(VIEWER_FIND_BAR_HEIGHT))
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuPanel);
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
    .style(move |_, status| crate::style::button_style(tokens, Class::ToolbarButton, status))
}

fn loading_cursor_layer() -> Element<'static, Message> {
    mouse_area(container("").width(Length::Fill).height(Length::Fill))
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
    let tokens = app.theme.tokens(&app.style_book);
    let page_count = app.doc.as_ref().map_or(0, |doc| doc.page_count());
    let current_page = if page_count == 0 {
        0
    } else {
        app.current_page().saturating_add(1).min(page_count)
    };
    let document_title = app
        .doc
        .as_ref()
        .and_then(|doc| doc.path().file_name())
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("Open PDF");
    let theme_label = match app.theme {
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

    if let Some(selection) = app.viewer_text_selection {
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
                VIEWER_TOOLBAR_SELECTION_WIDTH,
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
        .style(move |_| container_style(tokens, Class::Toolbar))
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
    let numerator: Element<'a, Message> = if app.page_input_editing {
        text_input("", &app.jump_input)
            .id(iced::widget::Id::new(PAGE_INPUT_ID))
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .padding([Spacing::XS, Spacing::SM])
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .width(Length::Fixed(VIEWER_PAGE_NUMBER_WIDTH))
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
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
            .width(Length::Fixed(VIEWER_PAGE_NUMBER_WIDTH))
            .height(Length::Fixed(VIEWER_PAGE_CHEVRON_SIZE))
            .center(Length::Fill),
        )
        .on_double_click(Message::StartPageInputEdit)
        .into()
    };

    row![
        viewer_page_chevron_button(CHEVRON_LEFT_SVG, tokens)
            .on_press(Message::PreviousPage)
            .width(Length::Fixed(VIEWER_PAGE_CHEVRON_SIZE))
            .height(Length::Fixed(VIEWER_PAGE_CHEVRON_SIZE)),
        numerator,
        text(format!("/ {page_count}"))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
        viewer_page_chevron_button(CHEVRON_RIGHT_SVG, tokens)
            .on_press(Message::NextPage)
            .width(Length::Fixed(VIEWER_PAGE_CHEVRON_SIZE))
            .height(Length::Fixed(VIEWER_PAGE_CHEVRON_SIZE)),
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
        .style(move |_, status| crate::style::button_style(tokens, Class::ToolbarButton, status))
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
    let selection_reserve = if app.viewer_text_selection.is_some() {
        VIEWER_TOOLBAR_SELECTION_WIDTH + 2.0 * (76.0 + Spacing::SM)
    } else {
        0.0
    };
    let chrome_reserve = 470.0 + selection_reserve;
    (app.viewport_width - chrome_reserve).clamp(
        VIEWER_TOOLBAR_TITLE_MIN_WIDTH,
        VIEWER_TOOLBAR_TITLE_MAX_WIDTH,
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
        + VIEWER_PAGE_CONTROL_WIDTH
        + Spacing::SM
        + VIEWER_ZOOM_STEP_BUTTON_WIDTH
        + Spacing::SM
        + ZOOM_CONTROL_WIDTH;

    (zoom_control_right - ZOOM_MENU_WIDTH).max(Spacing::MD)
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

pub(crate) fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let entries = app.visible_library_entries();
    let child_folders = app.child_folders();
    let render_items = library_render_items(app, &entries);
    let folder_section_height = folder_cards_section_height(app, child_folders.len());
    let entry_scroll_offset = (app.library_scroll_offset - folder_section_height).max(0.0);
    let window = app.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
    let mut header = row![];
    if !app.library_tag_sidebar_open {
        header = header.push(sidebar_chevron_button(
            CHEVRON_RIGHT_SVG,
            "Expand Sidebar",
            Message::ExpandLibrarySidebar,
            tokens,
        ));
    }
    let mut header = header
        .push(
            search_input_with_class(
                "Search library",
                &app.search_query,
                tokens,
                Class::LibrarySearchInput,
                Message::SearchQueryChanged,
            )
            .id(Id::new(LIBRARY_SEARCH_INPUT_ID))
            .width(Length::Fill),
        )
        .push(
            pick_list(
                LIBRARY_SORT_OPTIONS,
                Some(app.library_sort_mode),
                Message::LibrarySortChanged,
            )
            .placeholder("Sort")
            .width(190.0)
            .menu_height(360.0)
            .padding([Spacing::SM, Spacing::MD])
            .text_size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
            .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown)),
        )
        .push(library_layout_toggle_button(app, tokens))
        .push(library_metadata_density_picker(app, tokens));
    if app.doc.is_some() {
        header = header.push(toolbar_button("Viewer", tokens).on_press(Message::BackToViewer));
    }
    let header = header
        .push(library_new_folder_button(tokens).on_press(Message::OpenCreateFolderDialog))
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center);
    let header = container(header)
        .width(Length::Fill)
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::LibraryControlBar));

    let reorder_hint = if app.can_drag_reorder_library() {
        "Manual reorder enabled"
    } else {
        "Reordering requires unfiltered Manual sort"
    };
    let context_row = if app.selected_library_entries.is_empty() {
        view_library_breadcrumb_row(app, tokens, reorder_hint)
    } else {
        view_library_selection_status_row(app, tokens, reorder_hint)
    };
    let mut content = column![header, context_row,]
        .spacing(Spacing::MD)
        .padding(Spacing::LG);
    if let Some(progress) = app.bulk_operation_progress.as_ref() {
        content = content.push(bulk_operation_progress_banner(app, progress, tokens));
    }
    if let Some(error) = app.library_error.as_deref() {
        content = content.push(dismissible_error_banner(
            error,
            tokens,
            Message::DismissLibraryError,
        ));
    }

    if entries.is_empty() && child_folders.is_empty() {
        content = content.push(empty_state(
            if app.selected_folder.is_some() {
                "This folder is empty."
            } else {
                "Import a folder of PDFs to build your library."
            },
            tokens,
        ));
    } else if app.compact_view_mode {
        let mut rows = column![].spacing(Spacing::SM);
        let top_spacer = window.start as f32 * app.library_row_height();
        let bottom_spacer =
            entries.len().saturating_sub(window.end) as f32 * app.library_row_height();
        if top_spacer > 0.0 {
            rows = rows.push(container("").height(top_spacer));
        }
        for item in render_items[window.clone()].iter().cloned() {
            rows = rows.push(match item {
                LibraryRenderItem::Entry(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Normal)
                }
                LibraryRenderItem::Ghost(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Placeholder)
                }
                LibraryRenderItem::DropZone(_) => library_drop_zone_row(app, tokens),
            });
        }
        if bottom_spacer > 0.0 {
            rows = rows.push(container("").height(bottom_spacer));
        }
        let scroll_content = if child_folders.is_empty() {
            rows
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), rows]
                .spacing(Spacing::MD)
        };
        content = content.push(library_scrollable(scroll_content, tokens));
    } else {
        let layout = app.library_render_item_masonry_layout(&render_items);
        let mut grid = row![]
            .spacing(app.layout().library_masonry_gap)
            .height(layout.content_height);
        for column_items in &layout.columns {
            let mut stack = column![]
                .width(app.library_grid_card_width())
                .height(layout.content_height);
            let mut cursor_y = 0.0;
            for item_layout in column_items {
                let bottom = item_layout.top + item_layout.height;
                let visible_top = entry_scroll_offset
                    - app.layout().library_overscan_rows as f32 * app.library_row_height();
                let visible_bottom = entry_scroll_offset
                    + app.library_viewport_height.max(1.0)
                    + app.layout().library_overscan_rows as f32 * app.library_row_height();
                if bottom < visible_top || item_layout.top > visible_bottom {
                    continue;
                }

                let spacer = item_layout.top - cursor_y;
                if spacer > 0.0 {
                    stack = stack.push(container("").height(spacer));
                }
                if let Some(item) = render_items.get(item_layout.index).cloned() {
                    stack = stack.push(match item {
                        LibraryRenderItem::Entry(entry) => {
                            library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Normal)
                        }
                        LibraryRenderItem::Ghost(entry) => library_entry_card(
                            app,
                            entry,
                            tokens,
                            LibraryEntryRenderMode::Placeholder,
                        ),
                        LibraryRenderItem::DropZone(_) => library_drop_zone_card(app, tokens),
                    });
                    cursor_y = bottom;
                }
            }
            let trailing = layout.content_height - cursor_y;
            if trailing > 0.0 {
                stack = stack.push(container("").height(trailing));
            }
            grid = grid.push(stack);
        }
        grid = grid.push(container("").width(app.layout().library_scrollbar_gutter));
        let scroll_content = if child_folders.is_empty() {
            column![grid]
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), grid]
                .spacing(Spacing::MD)
        };
        content = content.push(library_scrollable(scroll_content, tokens));
    }

    let main_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell));

    let mut layout = row![].height(Length::Fill);
    if app.library_tag_sidebar_open {
        layout = layout.push(view_library_tag_sidebar(app));
    }
    layout = layout.push(main_content);
    layout.height(Length::Fill).into()
}

pub(crate) fn view_library_breadcrumb_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let breadcrumbs = app.folder_breadcrumbs();
    let active_index = breadcrumbs.len().saturating_sub(1);
    let mut trail = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    for (index, (label, folder_id)) in breadcrumbs.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(
                text(">")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            );
        }

        trail = trail.push(breadcrumb_button(
            label,
            folder_id,
            index == active_index,
            tokens,
        ));
    }

    row![
        row![
            trail.width(Length::Shrink),
            library_quick_filter_chips(app, tokens),
            library_filter_summary(app, tokens),
            library_grid_zoom_control(app, tokens),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
        text(reorder_hint)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(crate) fn library_quick_filter_chips<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut chips = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    let missing_active = app.missing_filter_active;
    let library_menu_text = tokens.class_styles[Class::LibraryControlBar.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_secondary);
    chips = chips.push(
        button(
            text("Missing")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(library_menu_text),
        )
        .on_press(Message::MissingFilterChanged(!missing_active))
        .padding([Spacing::XS, Spacing::MD])
        .style(move |_, status| {
            if missing_active {
                crate::style::button_style(tokens, Class::LibraryImportButton, status)
            } else {
                crate::style::button_style(tokens, Class::TagPill, status)
            }
        }),
    );

    chips.into()
}

pub(crate) fn library_filter_summary<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut labels = Vec::new();
    if let Some(folder_name) = app.selected_folder_name() {
        labels.push(format!("Folder: {folder_name}"));
    }
    if let Some(tag) = app.active_tag_filter.as_ref() {
        labels.push(format!("Tag: {tag}"));
    }
    if let Some(filter) = app.active_reading_filter {
        labels.push(format!("Reading: {}", filter.label()));
    }
    if app.missing_filter_active {
        labels.push(String::from("Missing files"));
    }
    let query = app.search_query.trim();
    if !query.is_empty() {
        labels.push(format!("Search: {query}"));
    }

    if labels.is_empty() {
        return container("").width(Length::Shrink).into();
    }

    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for label in labels {
        row = row.push(
            container(
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_primary)
                    .wrapping(Wrapping::None),
            )
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| container_style(tokens, Class::TagPill)),
        );
    }

    row = row.push(
        tag_pill("Clear filters", tokens)
            .on_press(Message::ClearLibraryFilters)
            .padding([Spacing::XS, Spacing::MD]),
    );
    row.into()
}

pub(crate) fn view_library_selection_status_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let selected_count = app.selected_library_entries.len();
    let mut details = row![
        master_checkbox(
            app.master_checkbox_state(),
            tokens,
            Message::MasterCheckboxClicked
        ),
        text(format!("{} selected", format_count(selected_count, "PDF")))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.accent),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    if let Some(status) = app.library_status.as_deref() {
        details = details.push(
            text(status)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary),
        );
    }

    row![
        details,
        text(reorder_hint)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(crate) fn breadcrumb_button<'a>(
    label: String,
    folder_id: Option<FolderId>,
    active: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.accent
            })
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_, status| {
        if active {
            let active_style =
                tokens.class_styles[Class::SidebarRow.index()].resolve(ComponentState::Active);
            crate::style::button_style(tokens, Class::SidebarRow, status)
                .with_visual_override(active_style)
        } else {
            crate::style::button_style(tokens, Class::SidebarRow, status)
        }
    })
    .on_press(Message::FolderSelected(folder_id))
    .into()
}

pub(crate) fn library_scrollable<'a>(
    content: iced::widget::Column<'a, Message>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    scrollable(content)
        .id(Id::new(LIBRARY_SCROLLABLE_ID))
        .height(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::LibraryRow, status))
        .on_scroll(|viewport| {
            let offset = viewport.absolute_offset();
            let bounds = viewport.bounds();
            Message::LibraryScrolled {
                offset_y: offset.y,
                viewport_x: bounds.x,
                viewport_y: bounds.y,
                viewport_width: bounds.width,
                viewport_height: bounds.height,
            }
        })
        .into()
}

pub(crate) fn view_confirmation_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let Some(action) = app.pending_confirmation.as_ref() else {
        return container("").into();
    };
    let (title, body, confirm_label) = confirmation_copy(action, app);
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
            .color(tokens.text_primary),
        text(body).size(FontSize::MD).color(tokens.text_secondary),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelConfirmation),
            toolbar_button(confirm_label, tokens).on_press(Message::ConfirmPendingAction),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

pub(crate) fn bulk_operation_progress_banner<'a>(
    app: &'a PDFolioApp,
    progress: &'a BulkOperationProgress,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let elapsed = app
        .animation_now
        .saturating_duration_since(progress.started_at)
        .as_secs_f32();
    let value = indeterminate_progress_value(elapsed);
    let label = format!("{} {} PDFs...", progress.label, progress.total);

    container(
        column![
            row![
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text("Working in background")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            ]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center),
            progress_bar(value, tokens),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

pub(crate) fn indeterminate_progress_value(elapsed_secs: f32) -> f32 {
    let sweep = (elapsed_secs * 0.72).fract();
    (0.18 + 0.64 * (0.5 - (sweep - 0.5).abs()) * 2.0).clamp(0.0, 1.0)
}

pub(crate) fn view_create_folder_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let parent = app
        .selected_folder_name()
        .unwrap_or_else(|| String::from("Library"));
    let dialog = column![
        text("New Folder")
            .size(FontSize::HEADING)
            .color(tokens.text_primary),
        text(format!("Create a folder in {parent}."))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        text_input("Folder name", &app.new_folder_name)
            .on_input(Message::NewFolderNameChanged)
            .on_submit(Message::CreateFolder)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
            toolbar_button("Create", tokens).on_press(Message::CreateFolder),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

pub(crate) fn confirmation_copy<'a>(
    action: &'a ConfirmationAction,
    app: &'a PDFolioApp,
) -> (&'a str, String, &'a str) {
    match action {
        ConfirmationAction::BulkResetDisplayMetadata => (
            "Reset metadata?",
            format!(
                "This will clear display title and author edits for {} selected PDFs.",
                app.selected_library_entries.len()
            ),
            "Reset",
        ),
        ConfirmationAction::BulkDeleteFromLibrary => (
            "Delete from library?",
            format!(
                "This removes library metadata for {} selected PDFs. The PDF files remain on disk.",
                app.selected_library_entries.len()
            ),
            "Delete",
        ),
        ConfirmationAction::ResetDetailsMetadata(_) => (
            "Reset PDF details?",
            String::from("This clears the edited display title and author for this PDF."),
            "Reset",
        ),
        ConfirmationAction::DeleteFolder(folder_id) => (
            "Delete folder?",
            format!(
                "This removes the folder \"{}\" and any nested folders. PDFs remain in the library and on disk.",
                app.library_folders
                    .iter()
                    .find(|folder| &folder.id == folder_id)
                    .map_or("Selected folder", |folder| folder.name.as_str())
            ),
            "Delete",
        ),
    }
}

pub(crate) fn view_folder_cards<'a>(
    app: &'a PDFolioApp,
    folders: Vec<Folder>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let active_folder_drag = app.folder_drag.as_ref().filter(|drag| drag.active);
    let mut rows = column![].spacing(Spacing::SM);
    for chunk in folders.chunks(folder_cards_per_row(app)) {
        let mut card_row = row![].spacing(app.layout().library_masonry_gap);
        for folder in chunk {
            let mode = if active_folder_drag.is_some_and(|drag| drag.folder_id == folder.id) {
                if active_folder_drag
                    .and_then(|drag| drag.drop_target.as_ref())
                    .is_some()
                {
                    FolderCardRenderMode::NestingTarget
                } else {
                    FolderCardRenderMode::Placeholder
                }
            } else {
                FolderCardRenderMode::Normal
            };
            card_row = card_row.push(folder_grid_card(app, folder.clone(), tokens, mode));
        }
        rows = rows.push(card_row);
    }
    rows.into()
}

pub(crate) fn folder_cards_per_row(app: &PDFolioApp) -> usize {
    let available_width =
        (app.library_viewport_width - app.layout().library_scrollbar_gutter).max(1.0);
    let card_pitch = app.library_grid_card_width() + app.layout().library_masonry_gap;
    ((available_width + app.layout().library_masonry_gap) / card_pitch)
        .floor()
        .max(1.0) as usize
}

pub(crate) fn folder_cards_section_height(app: &PDFolioApp, folder_count: usize) -> f32 {
    if folder_count == 0 {
        return 0.0;
    }

    let rows = folder_count.div_ceil(folder_cards_per_row(app)).max(1);
    rows as f32 * app.layout().library_folder_grid_row_height
        + rows.saturating_sub(1) as f32 * Spacing::SM
        + Spacing::MD
}

pub(crate) fn folder_grid_card<'a>(
    app: &'a PDFolioApp,
    folder: Folder,
    tokens: ThemeTokens,
    mode: FolderCardRenderMode,
) -> Element<'a, Message> {
    let folder_id = folder.id.clone();
    let drop_active = app.active_folder_drop_target() == Some(&folder.id);
    let flash_active = app.folder_drop_flash_active(&folder.id);
    let smart_counts = app.folder_smart_counts(Some(&folder.id));
    let child_count = app
        .library_folders
        .iter()
        .filter(|child| child.parent_id.as_ref() == Some(&folder.id))
        .count();
    let meta = folder_meta_label(smart_counts, child_count);
    let folder_title_size = app.library_card_font_size(FontSize::CONTROL);
    let folder_meta_size = app.library_card_font_size(FontSize::SM);
    let folder_text_width = (app.library_grid_card_width() - 72.0).max(16.0);
    let content_alpha = folder_card_content_alpha(app, mode);
    let title =
        truncate_for_width_with_font(&folder.name, folder_text_width, 0.0, folder_title_size);
    let meta = truncate_for_width_with_font(&meta, folder_text_width, 0.0, folder_meta_size);
    let content = row![
        folder_icon(tokens, content_alpha),
        column![
            text(title)
                .size(folder_title_size)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(with_alpha(tokens.text_primary, content_alpha))
                .wrapping(Wrapping::None),
            text(meta)
                .size(folder_meta_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(with_alpha(tokens.text_secondary, content_alpha))
                .wrapping(Wrapping::None),
        ]
        .spacing(app.library_card_spacing().min(Spacing::XS))
        .width(Length::Fill),
    ]
    .spacing(app.library_card_spacing().max(Spacing::XS))
    .padding(app.library_card_padding().min(Spacing::MD))
    .height(app.layout().library_folder_grid_row_height)
    .align_y(iced::Alignment::Center);

    let card = container(content)
        .width(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::LibraryFolderCard);
            if matches!(mode, FolderCardRenderMode::Placeholder) {
                let placeholder_style = tokens.class_styles[Class::LibraryFolderCard.index()]
                    .resolve(ComponentState::Disabled);
                style = style.with_visual_override(placeholder_style);
            }
            if drop_active || flash_active || matches!(mode, FolderCardRenderMode::NestingTarget) {
                let drop_style = container_style(tokens, Class::FolderDropTarget);
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .width(app.library_grid_card_width());

    if mode == FolderCardRenderMode::Floating {
        return card.into();
    }

    let area = mouse_area(card)
        .on_enter(Message::FolderDropTargetChanged(Some(folder_id)))
        .on_exit(Message::FolderDropTargetChanged(None));
    if mode == FolderCardRenderMode::Normal {
        area.on_press(Message::BeginFolderDrag(folder.id.clone()))
            .on_release(Message::EndFolderDrag)
            .interaction(mouse::Interaction::Grab)
            .into()
    } else {
        area.into()
    }
}

pub(crate) fn folder_icon<'a>(tokens: ThemeTokens, alpha: f32) -> Element<'a, Message> {
    container(
        text("DIR")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(with_alpha(tokens.accent, alpha)),
    )
    .center(38.0)
    .height(28.0)
    .style(move |_| {
        let mut style = container_style(tokens, Class::TagPill);
        style.background = Some(iced::Background::Color(mix_color(
            tokens.surface,
            tokens.accent,
            0.18,
        )));
        style
    })
    .into()
}

pub(crate) fn folder_card_content_alpha(app: &PDFolioApp, mode: FolderCardRenderMode) -> f32 {
    if mode == FolderCardRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

pub(crate) fn folder_meta_label(counts: FolderSmartCounts, child_count: usize) -> String {
    let mut parts = Vec::new();
    if counts.total > 0 {
        parts.push(format_count(counts.total, "PDF"));
    }
    if child_count > 0 {
        parts.push(format_count(child_count, "Folder"));
    }
    if counts.in_progress > 0 {
        parts.push(format!("{} reading", counts.in_progress));
    }
    if counts.missing > 0 {
        parts.push(format!("{} missing", counts.missing));
    }

    if parts.is_empty() {
        String::from("Empty")
    } else {
        parts.join(" . ")
    }
}

pub(crate) fn folder_sidebar_count_label(counts: FolderSmartCounts) -> String {
    format_count(counts.total, "PDF")
}

pub(crate) fn format_count(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

pub(crate) fn scroll_library_to_offset_task(offset_y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(LIBRARY_SCROLLABLE_ID),
        operation::AbsoluteOffset {
            x: Some(0.0),
            y: Some(offset_y.max(0.0)),
        },
    )
}

impl LibraryRenderItem {
    pub(crate) fn entry(&self) -> &LibraryEntry {
        match self {
            Self::Entry(entry) | Self::Ghost(entry) | Self::DropZone(entry) => entry,
        }
    }
}

pub(crate) fn library_render_items(
    app: &PDFolioApp,
    entries: &[LibraryEntry],
) -> Vec<LibraryRenderItem> {
    let Some(drag) = app.library_drag.as_ref().filter(|drag| drag.active) else {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    };
    if !drag.multi {
        let Some(ghost_entry) = entries
            .iter()
            .find(|entry| entry.id == drag.entry_id)
            .cloned()
        else {
            return entries
                .iter()
                .cloned()
                .map(LibraryRenderItem::Entry)
                .collect();
        };

        let compact_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.id != drag.entry_id)
            .cloned()
            .collect();
        let target_index = drag.target_index.min(compact_entries.len());

        let mut items = Vec::with_capacity(entries.len());
        for index in 0..=compact_entries.len() {
            if target_index == index {
                items.push(LibraryRenderItem::Ghost(ghost_entry.clone()));
            }

            if let Some(entry) = compact_entries.get(index) {
                items.push(LibraryRenderItem::Entry(entry.clone()));
            }
        }

        return items;
    }

    let dragged_ids = drag.entry_ids.iter().cloned().collect::<HashSet<_>>();
    let placeholder_entries = entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .cloned()
        .collect::<Vec<_>>();
    if placeholder_entries.is_empty() {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    }

    let drop_zone_entry = placeholder_entries[0].clone();
    let target_index = drag
        .target_index
        .min(entries.len().saturating_sub(placeholder_entries.len()));
    let mut compact_index = 0;
    let mut drop_zone_inserted = false;
    let mut items = Vec::with_capacity(entries.len() + 1);
    for entry in entries {
        if dragged_ids.contains(&entry.id) {
            items.push(LibraryRenderItem::Ghost(entry.clone()));
        } else {
            if !drop_zone_inserted && drag.drop_target.is_none() && compact_index == target_index {
                items.push(LibraryRenderItem::DropZone(drop_zone_entry.clone()));
                drop_zone_inserted = true;
            }
            items.push(LibraryRenderItem::Entry(entry.clone()));
            compact_index += 1;
        }
    }
    if !drop_zone_inserted && drag.drop_target.is_none() {
        items.push(LibraryRenderItem::DropZone(drop_zone_entry));
    }

    items
}

pub(crate) fn shortest_column_index(column_heights: &[f32]) -> usize {
    column_heights
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(crate) fn masonry_target_index(
    layout: &LibraryMasonryLayout,
    column_index: usize,
    content_y: f32,
) -> Option<usize> {
    let column = layout.columns.get(column_index)?;
    if column.is_empty() {
        return Some(layout.columns.iter().flatten().count());
    }

    column
        .iter()
        .find(|item| content_y < item.top + item.height / 2.0)
        .map(|item| item.index)
        .or_else(|| column.last().map(|item| item.index + 1))
}

pub(crate) fn floating_library_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app.library_drag.as_ref().filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let visible_entries = app.visible_library_entries();
    let entry = visible_entries
        .iter()
        .find(|entry| entry.id == drag.entry_id)?
        .clone();

    let preview = if drag.multi {
        multi_drag_stack_preview(app, drag, &visible_entries, tokens)?
    } else if app.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    let x_offset = if app.compact_view_mode {
        app.layout().library_drag_preview_list_x_offset
    } else {
        app.layout().library_drag_preview_grid_x_offset
    };
    let y_offset = if app.compact_view_mode {
        app.layout().library_drag_preview_list_y_offset
    } else {
        app.layout().library_drag_preview_grid_y_offset
    };

    Some(
        pin(preview)
            .x((cursor.x - x_offset).max(0.0))
            .y((cursor.y - y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

pub(crate) fn floating_folder_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app.folder_drag.as_ref().filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let folder = app
        .library_folders
        .iter()
        .find(|folder| folder.id == drag.folder_id)?
        .clone();
    let preview = container(folder_grid_card(
        app,
        folder,
        tokens,
        FolderCardRenderMode::Floating,
    ))
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        pin(preview)
            .x((cursor.x - app.layout().library_drag_preview_grid_x_offset).max(0.0))
            .y((cursor.y - app.layout().library_drag_preview_grid_y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

pub(crate) fn multi_drag_stack_preview<'a>(
    app: &'a PDFolioApp,
    drag: &LibraryDragState,
    visible_entries: &[LibraryEntry],
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let dragged_ids = drag.entry_ids.iter().collect::<HashSet<_>>();
    let mut entries = visible_entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }

    while entries.len() < 3 {
        let entry = entries.last().cloned()?;
        entries.push(entry);
    }

    let rear = drag_stack_card(app, entries[2].clone(), tokens);
    let middle = drag_stack_card(app, entries[1].clone(), tokens);
    let front = drag_stack_card(app, entries[0].clone(), tokens);
    let badge = container(
        text(format_count(drag.entry_ids.len(), "PDF"))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_primary),
    )
    .padding([Spacing::XS, Spacing::MD])
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        stack![
            pin(rear).x(Spacing::LG).y(Spacing::LG),
            pin(middle).x(Spacing::SM).y(Spacing::SM),
            pin(front).x(0.0).y(0.0),
            pin(badge).x(Spacing::MD).y(Spacing::MD),
        ]
        .into(),
    )
}

pub(crate) fn drag_stack_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let card = if app.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    container(card)
        .style(move |_| container_style(tokens, Class::DragStackGhost))
        .into()
}

pub(crate) fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.theme.tokens(&app.style_book);
    let sidebar_width = app.library_tag_sidebar_width;
    let sidebar_body = if let Some(entry) = app.primary_selected_entry() {
        view_selected_pdf_sidebar(app, entry, sidebar_width, tokens)
    } else if !app.selected_library_entries.is_empty() {
        view_multi_selection_sidebar(app, sidebar_width, tokens)
    } else {
        view_library_navigation_sidebar(app, sidebar_width, tokens)
    };

    let sidebar = container(sidebar_body)
        .width(sidebar_width)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::Sidebar));

    let handle_color = if app.resizing_library_tag_sidebar {
        tokens.focus
    } else {
        tokens.border
    };
    let handle_visual_width = if app.resizing_library_tag_sidebar {
        app.layout().sidebar_resize_handle_width
    } else {
        app.layout().sidebar_resize_handle_visual_width
    };
    let resize_handle = mouse_area(
        container(
            container("")
                .width(handle_visual_width)
                .height(Length::Fill)
                .style(move |_| {
                    let mut style = container_style(tokens, Class::Sidebar);
                    style.background = Some(iced::Background::Color(handle_color));
                    style
                }),
        )
        .width(app.layout().sidebar_resize_handle_width)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(Message::BeginTagSidebarResize)
    .on_release(Message::EndTagSidebarResize)
    .interaction(mouse::Interaction::ResizingHorizontally);

    row![sidebar, resize_handle].height(Length::Fill).into()
}

pub(crate) fn view_library_navigation_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let heading = container(
        row![
            section_heading("Explorer", tokens).width(Length::Fill),
            sidebar_chevron_button(
                CHEVRON_LEFT_SVG,
                "Collapse Sidebar",
                Message::CollapseLibrarySidebar,
                tokens,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding(Spacing::MD);

    let sidebar_tab_component = tokens.class_styles[Class::SidebarTab.index()];
    let sidebar_tab_layout = sidebar_tab_component.layout;
    let sidebar_tab_style = sidebar_tab_component.resolve(ComponentState::Normal);
    let tab_area_background = sidebar_tab_style
        .background
        .unwrap_or_else(|| sidebar_tab_area_background(tokens));
    let file_tree_component = tokens.class_styles[Class::FileTree.index()];
    let file_tree_layout = file_tree_component.layout;
    let file_tree_style = file_tree_component.resolve(ComponentState::Normal);
    let content_background = file_tree_style
        .background
        .or_else(|| {
            sidebar_tab_component
                .resolve(ComponentState::Active)
                .background
        })
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let tabs = container(
        row![
            sidebar_tab_button(
                LibrarySidebarTab::Files,
                app.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
            sidebar_tab_button(
                LibrarySidebarTab::Tags,
                app.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
        ]
        .spacing(sidebar_tab_layout.spacing.unwrap_or(Spacing::XS))
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(iced::Padding {
        top: sidebar_tab_layout.margin_top(Spacing::XS),
        right: sidebar_tab_layout.margin_right(Spacing::SM),
        bottom: sidebar_tab_layout.margin_bottom(Spacing::XS),
        left: sidebar_tab_layout.margin_left(Spacing::SM),
    })
    .style(move |_| {
        let mut style = container_style(tokens, Class::Sidebar);
        style.background = Some(iced::Background::Color(tab_area_background));
        style.border.width = 0.0;
        style
    });

    let body = match app.library_sidebar_tab {
        LibrarySidebarTab::Files => view_file_tree_sidebar(app, sidebar_width, tokens),
        LibrarySidebarTab::Tags => view_tag_tree_sidebar(app, sidebar_width, tokens),
    };

    let body_scroll = scrollable(body)
        .direction(sidebar_scroll_direction())
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status));

    let padded_body = container(body_scroll)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: file_tree_layout.padding_top(0.0),
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

    let tabbed_body = container(column![tabs, padded_body].spacing(0).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::FileTree);
            if file_tree_style.background.is_none() {
                style.background = Some(iced::Background::Color(content_background));
            }
            style
        });

    let mut content = column![heading].spacing(Spacing::SM).height(Length::Fill);
    if let Some(panel) = selected_folder_actions_panel(app, sidebar_width, tokens) {
        content = content.push(container(panel).padding([0.0, Spacing::MD]));
    }
    content = content.push(tabbed_body);

    container(content).height(Length::Fill).into()
}

pub(crate) fn sidebar_tab_button<'a>(
    tab: LibrarySidebarTab,
    active_tab: LibrarySidebarTab,
    tokens: ThemeTokens,
    labels: &'a crate::style::AppLabelTokens,
) -> iced::widget::Button<'a, Message> {
    let active = tab == active_tab;
    let component = tokens.class_styles[Class::SidebarTab.index()];
    let layout = component.layout;
    let text_style = component.text;
    let normal_style = component.resolve(ComponentState::Normal);
    let active_style = component.resolve(ComponentState::Active);
    button(
        text(library_sidebar_tab_label(labels, tab))
            .size(text_style.size.unwrap_or(FontSize::MD))
            .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(if active {
                active_style.text_color.unwrap_or(tokens.text_primary)
            } else {
                normal_style.text_color.unwrap_or(tokens.text_secondary)
            }),
    )
    .height(layout.height.unwrap_or(30.0))
    .width(Length::FillPortion(layout.width_portion.unwrap_or(1)))
    .padding(iced::Padding {
        top: layout.padding_top(Spacing::XS),
        right: layout.padding_right(Spacing::MD),
        bottom: layout.padding_bottom(Spacing::XS),
        left: layout.padding_left(Spacing::MD),
    })
    .style(move |_, status| {
        let style = crate::style::button_style(tokens, Class::SidebarTab, status);
        let state = if active {
            ComponentState::Active
        } else {
            match status {
                iced::widget::button::Status::Active => ComponentState::Normal,
                iced::widget::button::Status::Hovered => ComponentState::Hovered,
                iced::widget::button::Status::Pressed => ComponentState::Pressed,
                iced::widget::button::Status::Disabled => ComponentState::Disabled,
            }
        };
        let state_style = component.resolve(state);
        style.with_visual_override(state_style)
    })
    .on_press(Message::LibrarySidebarTabChanged(tab))
}

pub(crate) fn sidebar_tab_area_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, Color::BLACK, 0.34)
    } else {
        mix_color(tokens.surface_raised, Color::BLACK, 0.09)
    }
}

pub(crate) fn sidebar_tab_content_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, tokens.surface_raised, 0.62)
    } else {
        tokens.surface
    }
}

pub(crate) fn is_dark_surface(color: Color) -> bool {
    color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722 < 0.5
}

pub(crate) fn view_file_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let library_counts = app.folder_smart_counts(None);
    let mut tree = column![file_tree_row(
        "Library",
        Some(folder_sidebar_count_label(library_counts)),
        0,
        app.selected_folder.is_none(),
        true,
        app.library_tree_root_expanded,
        Message::ToggleLibraryTreeRoot,
        Message::FolderSelected(None),
        sidebar_width,
        tokens,
        false,
    ),]
    .spacing(0);

    if app.library_tree_root_expanded {
        tree = tree.push(folder_sidebar_rows(app, None, 1, sidebar_width, tokens));
    }

    tree.into()
}

pub(crate) fn selected_folder_actions_panel<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let folder = app.selected_folder()?;
    let parent_id = folder.parent_id.clone();
    let has_parent = parent_id.is_some();
    let has_grandparent = parent_id.as_ref().is_some_and(|parent_id| {
        app.library_folders
            .iter()
            .find(|candidate| &candidate.id == parent_id)
            .and_then(|parent| parent.parent_id.as_ref())
            .is_some()
    });
    let can_move_earlier = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, _, index)| index > 0);
    let can_move_later = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, folder_ids, index)| index + 1 < folder_ids.len());
    let input_width = (sidebar_width - Spacing::XL * 2.0).max(80.0);
    let mut actions = column![
        text("Folder")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(sidebar_folder_card_title_color(tokens)),
        text_input("Folder name", &app.folder_rename_input)
            .on_input(Message::FolderRenameInputChanged)
            .on_submit(Message::RenameSelectedFolder)
            .id(Id::new(LIBRARY_FOLDER_RENAME_INPUT_ID))
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(input_width),
        row![
            sidebar_folder_action_button("Rename", tokens).on_press(Message::RenameSelectedFolder),
            sidebar_folder_action_button("Delete", tokens)
                .on_press(Message::RequestDeleteSelectedFolder),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
        row![
            maybe_sidebar_folder_action_button(
                "Earlier",
                tokens,
                can_move_earlier,
                Message::MoveSelectedFolderEarlier,
            ),
            maybe_sidebar_folder_action_button(
                "Later",
                tokens,
                can_move_later,
                Message::MoveSelectedFolderLater,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::SM);

    if has_parent {
        actions = actions.push(
            sidebar_folder_action_button("Move to root", tokens)
                .on_press(Message::MoveSelectedFolderToRoot)
                .width(Length::Fill),
        );
    }
    if has_grandparent {
        actions = actions.push(
            sidebar_folder_action_button("Move up", tokens)
                .on_press(Message::MoveSelectedFolderUp)
                .width(Length::Fill),
        );
    }

    Some(
        container(actions)
            .width(Length::Fill)
            .padding(Spacing::MD)
            .style(move |_| container_style(tokens, Class::SidebarFolderCard))
            .into(),
    )
}

pub(crate) fn view_tag_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let all_tags = app.all_tags();
    let mut tags = column![
        file_tree_row(
            "All tags",
            Some(format_count(app.library_entries.len(), "PDF")),
            0,
            app.active_tag_filter.is_none(),
            !all_tags.is_empty(),
            true,
            Message::TagFilterChanged(None),
            Message::TagFilterChanged(None),
            sidebar_width,
            tokens,
            false,
        ),
        section_heading("Tags", tokens),
    ]
    .spacing(Spacing::SM);

    for tag in all_tags {
        let count = app
            .library_entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == &tag))
            .count();
        let active = app.active_tag_filter.as_ref() == Some(&tag);
        tags = tags.push(file_tree_row(
            tag.clone(),
            Some(format_count(count, "PDF")),
            1,
            active,
            false,
            false,
            Message::TagFilterChanged(Some(tag.clone())),
            Message::TagFilterChanged(Some(tag)),
            sidebar_width,
            tokens,
            false,
        ));
    }

    tags.into()
}

pub(crate) fn view_selected_pdf_sidebar<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let title = entry_title(&entry);
    let author = entry_author(&entry);
    let path_label = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown file");
    let folder_label = if entry.folders.is_empty() {
        String::from("No folders")
    } else {
        entry
            .folders
            .iter()
            .map(|folder| folder.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let tags_label = if entry.tags.is_empty() {
        String::from("No tags")
    } else {
        entry.tags.join(", ")
    };
    let progress_label = selected_pdf_progress_label(&entry);
    let status_label = if entry.missing {
        "Missing file"
    } else {
        "Available"
    };
    let duplicate_label = duplicate_status_label(app, &entry);
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("PDF Details", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let mut content = column![
        heading,
        thumbnail_element(app, &entry.id, tokens, details_width.min(160.0), 1.0),
        text(truncate_for_width(&title, details_width, 0.0))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens))
            .wrapping(Wrapping::None),
        text(truncate_for_width(&author, details_width, 0.0))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(sidebar_detail_secondary_color(tokens))
            .wrapping(Wrapping::None),
        sidebar_detail_row("Status", status_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Pages", page_count_label(&entry), details_width, tokens),
        sidebar_detail_row("Progress", progress_label, details_width, tokens),
        sidebar_detail_row("Size", file_size_label(&entry), details_width, tokens),
        sidebar_detail_row("Duplicates", duplicate_label, details_width, tokens),
        sidebar_detail_row("Opened", last_opened_label(&entry), details_width, tokens),
        sidebar_detail_row(
            "Added",
            format!("Added {}", entry.added_at.format("%b %-d, %Y")),
            details_width,
            tokens
        ),
        sidebar_detail_row("File", path_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Folders", folder_label, details_width, tokens),
        sidebar_detail_row("Tags", tags_label, details_width, tokens),
        sidebar_action_button("Open PDF", tokens)
            .on_press(Message::OpenLibraryEntry(entry.id.clone())),
        sidebar_action_button("Reveal in file manager", tokens)
            .on_press(Message::RevealEntryInFileManager(entry.id.clone())),
        sidebar_action_button("Open containing folder", tokens)
            .on_press(Message::OpenEntryContainingFolder(entry.id.clone())),
    ];
    if entry.missing {
        content = content.push(
            sidebar_action_button("Relink missing file", tokens)
                .on_press(Message::RelinkMissingEntry(entry.id.clone())),
        );
    }
    let content = content
        .push(
            sidebar_action_button("Clear selection", tokens)
                .on_press(Message::ClearLibrarySelection),
        )
        .spacing(Spacing::SM)
        .padding(Spacing::MD);

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction())
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

fn sidebar_scroll_direction() -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(4.0)
            .scroller_width(2.0)
            .anchor(Anchor::End),
    )
}

pub(crate) fn view_multi_selection_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let selected_entries = app.selected_entries();
    let selected_count = selected_entries.len();
    let total_pages: u32 = selected_entries
        .iter()
        .filter_map(|entry| entry.page_count.map(u32::from))
        .sum();
    let missing_count = selected_entries
        .iter()
        .filter(|entry| entry.missing)
        .count();
    let total_size_label = total_file_size_label(&selected_entries);
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("Selection", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let content = column![
        heading,
        text(format_count(selected_count, "PDF"))
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(sidebar_detail_primary_color(tokens)),
        sidebar_detail_row(
            "Known pages",
            if total_pages == 0 {
                String::from("Unknown")
            } else {
                total_pages.to_string()
            },
            details_width,
            tokens,
        ),
        sidebar_detail_row(
            "Missing files",
            missing_count.to_string(),
            details_width,
            tokens,
        ),
        sidebar_detail_row("Total size", total_size_label, details_width, tokens),
        sidebar_action_button("Clear selection", tokens).on_press(Message::ClearLibrarySelection),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(content)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
        .into()
}

pub(crate) fn sidebar_detail_row<'a>(
    label: &'a str,
    value: String,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(sidebar_detail_secondary_color(tokens)),
            text(truncate_for_width(&value, width, 0.0))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::REGULAR))
                .color(sidebar_detail_primary_color(tokens))
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

pub(crate) fn sidebar_detail_primary_color(tokens: ThemeTokens) -> Color {
    mix_color(tokens.text_secondary, tokens.text_primary, 0.52)
}

pub(crate) fn sidebar_detail_secondary_color(tokens: ThemeTokens) -> Color {
    with_alpha(tokens.text_secondary, 0.88)
}

pub(crate) fn sidebar_folder_card_title_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarFolderCardTitle.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or_else(|| sidebar_detail_secondary_color(tokens))
}

pub(crate) fn folder_sidebar_text_input_style(
    tokens: ThemeTokens,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    text_input_style(tokens, Class::SidebarFolderTextInput, status)
}

pub(crate) fn selected_pdf_progress_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || format!("Page {}", u32::from(entry.last_page) + 1),
        |page_count| {
            let current_page = entry.last_page.saturating_add(1).min(page_count.max(1));
            format!(
                "{} of {} ({:.0}%)",
                current_page,
                page_count,
                f32::from(current_page) / f32::from(page_count.max(1)) * 100.0
            )
        },
    )
}

pub(crate) fn duplicate_status_label(app: &PDFolioApp, entry: &LibraryEntry) -> String {
    let duplicate_count = app
        .library_entries
        .iter()
        .filter(|candidate| candidate.id == entry.id)
        .count()
        .saturating_sub(1);
    duplicate_status_label_for_count(duplicate_count)
}

pub(crate) fn duplicate_status_label_for_count(duplicate_count: usize) -> String {
    if duplicate_count == 0 {
        String::from("Unique content hash")
    } else {
        format_count(duplicate_count, "matching duplicate")
    }
}

pub(crate) fn folder_sidebar_rows<'a>(
    app: &'a PDFolioApp,
    parent_id: Option<&'a FolderId>,
    depth: usize,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(0);
    let mut children: Vec<&Folder> = app
        .library_folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == parent_id)
        .collect();
    children.sort_by_key(|folder| (folder.manual_order, folder.name.to_lowercase()));

    for folder in children {
        let has_children = app
            .library_folders
            .iter()
            .any(|child| child.parent_id.as_ref() == Some(&folder.id));
        let expanded = !app.collapsed_library_tree_folders.contains(&folder.id);
        let active = app.selected_folder.as_ref() == Some(&folder.id);
        let drop_active = app.active_folder_drop_target() == Some(&folder.id);
        let flash_active = app.folder_drop_flash_active(&folder.id);
        let counts = app.folder_smart_counts(Some(&folder.id));
        let row = file_tree_row(
            &folder.name,
            Some(folder_sidebar_count_label(counts)),
            depth,
            active,
            has_children,
            expanded,
            Message::ToggleLibraryTreeFolder(folder.id.clone()),
            Message::FolderSelected(Some(folder.id.clone())),
            sidebar_width,
            tokens,
            drop_active || flash_active,
        );
        rows = rows.push(
            mouse_area(row)
                .on_enter(Message::FolderDropTargetChanged(Some(folder.id.clone())))
                .on_exit(Message::FolderDropTargetChanged(None))
                .on_press(Message::BeginFolderDrag(folder.id.clone()))
                .on_release(Message::EndFolderDrag),
        );
        if expanded {
            rows = rows.push(folder_sidebar_rows(
                app,
                Some(&folder.id),
                depth.saturating_add(1),
                sidebar_width,
                tokens,
            ));
        }
    }

    rows.into()
}

pub(crate) fn file_tree_row<'a>(
    label: impl Into<String>,
    meta: Option<String>,
    depth: usize,
    active: bool,
    has_children: bool,
    expanded: bool,
    toggle_message: Message,
    message: Message,
    sidebar_width: f32,
    tokens: ThemeTokens,
    drop_active: bool,
) -> Element<'a, Message> {
    let label = label.into();
    let file_tree_style = tokens.class_styles[Class::FileTree.index()];
    let fold_button_component = tokens.class_styles[Class::FileTreeFoldButton.index()];
    let fold_button_layout = fold_button_component.layout;
    let fold_button_normal_style = fold_button_component.resolve(ComponentState::Normal);
    let fold_button_hovered_style = fold_button_component.resolve(ComponentState::Hovered);
    let normal_style = file_tree_style.resolve(ComponentState::Normal);
    let active_style = file_tree_style.resolve(ComponentState::Active);
    let content_background = normal_style
        .background
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let indent = (depth as f32 * 12.0).min(72.0);
    let fold_width = fold_button_layout.width.unwrap_or(16.0);
    let meta_width = meta
        .as_ref()
        .map_or(0.0, |value| (value.len() as f32 * 6.0).clamp(52.0, 128.0));
    let row_padding = Spacing::SM * 2.0;
    let row_spacing = Spacing::XS * if meta.is_some() { 3.0 } else { 2.0 };
    let label_width =
        (sidebar_width - row_padding - indent - fold_width - meta_width - row_spacing).max(42.0);
    let text_color = if active || drop_active {
        active_style.text_color.unwrap_or(tokens.text_primary)
    } else {
        normal_style.text_color.unwrap_or(tokens.text_secondary)
    };

    let chevron: Element<'_, Message> = if has_children {
        let icon = Svg::new(iced::widget::svg::Handle::from_memory(if expanded {
            CHEVRON_DOWN_SVG
        } else {
            CHEVRON_RIGHT_SVG
        }))
        .width(13.0)
        .height(13.0)
        .style(move |_, status| iced::widget::svg::Style {
            color: Some(match status {
                iced::widget::svg::Status::Hovered => fold_button_hovered_style
                    .text_color
                    .unwrap_or(tokens.text_primary),
                iced::widget::svg::Status::Idle => fold_button_normal_style
                    .text_color
                    .unwrap_or(tokens.text_secondary),
            }),
        });

        button(
            container(icon)
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill),
        )
        .width(fold_button_layout.width.unwrap_or(16.0))
        .height(fold_button_layout.height.unwrap_or(20.0))
        .padding(fold_button_layout.padding_top(0.0))
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::FileTreeFoldButton, status)
        })
        .on_press(toggle_message)
        .into()
    } else {
        container("").width(16.0).height(20.0).into()
    };

    let mut content = row![
        container("").width(indent),
        chevron,
        text(file_tree_label(&label, label_width))
            .size(FILE_TREE_LABEL_SIZE)
            .line_height(1.12)
            .font(file_tree_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(text_color)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(label_width)),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    if let Some(meta) = meta {
        content = content.push(
            text(meta)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None)
                .width(Length::Fixed(meta_width))
                .align_x(iced::alignment::Horizontal::Right),
        );
    }

    let row_button = button(content)
        .height(FILE_TREE_ROW_HEIGHT)
        .width(Length::Fill)
        .padding([3.0, Spacing::SM])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            let state = if active || drop_active {
                ComponentState::Active
            } else if hovered {
                ComponentState::Hovered
            } else {
                ComponentState::Normal
            };
            let mut style = crate::style::button_style(tokens, Class::FileTree, status);
            apply_file_tree_state_style(&mut style, tokens, state, content_background);
            if drop_active {
                let drop_style = crate::style::button_style(
                    tokens,
                    Class::FolderDropTarget,
                    button::Status::Active,
                );
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .on_press(message);

    if active || drop_active {
        if let Some(border) = side_border_for_class(tokens, Class::FileTree, ComponentState::Active)
        {
            side_border(row_button, border)
        } else {
            row_button.into()
        }
    } else {
        row_button.into()
    }
}

pub(crate) fn apply_file_tree_state_style(
    style: &mut button::Style,
    tokens: ThemeTokens,
    state: ComponentState,
    fallback_background: Color,
) {
    let state_style = tokens.class_styles[Class::FileTree.index()].resolve(state);
    let fallback_style = crate::style::tokens::VisualStyle {
        background: Some(state_style.background.unwrap_or(fallback_background)),
        ..state_style
    };
    *style = style.with_visual_override(fallback_style);
}

pub(crate) fn sidebar_chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    chevron_button(icon, tooltip_label, message, tokens, false)
}

fn chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
    transparent: bool,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(18.0)
        .height(18.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(28.0)
    .height(28.0)
    .padding(0)
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::SidebarToggleButton, status);
        if transparent {
            style.background = None;
            style.border.width = 0.0;
            style.shadow = iced::Shadow::default();
        }
        style
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
            .unwrap_or_else(|| with_alpha(tokens.text_secondary, 0.42))
    }
}

pub(crate) fn library_layout_toggle_button(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let (icon, tooltip_label) = if app.compact_view_mode {
        (GRID_LAYOUT_SVG, "Switch to grid")
    } else {
        (LIST_LAYOUT_SVG, "Switch to list")
    };
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(18.0)
        .height(18.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_primary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(34.0)
    .height(34.0)
    .padding(0)
    .style(move |_, status| crate::style::button_style(tokens, Class::LibraryViewToggle, status))
    .on_press(Message::ToggleViewMode);

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

pub(crate) fn library_grid_zoom_control<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let control = row![
        text("Grid")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
        slider(
            LIBRARY_GRID_ZOOM_MIN..=app.library_grid_zoom_max(),
            app.library_grid_zoom(),
            Message::LibraryGridZoomChanged,
        )
        .step(LIBRARY_GRID_ZOOM_STEP)
        .width(150.0)
        .style(move |_, status| slider_style(tokens, Class::LibraryGridZoomSlider, status)),
        text(app.library_grid_zoom_label())
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .width(44.0),
    ]
    .spacing(Spacing::SM)
    .align_y(iced::Alignment::Center);

    tooltip(
        control,
        container(
            text("Grid zoom")
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

pub(crate) fn library_new_folder_button<'a>(
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text("New folder")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::LibraryImportButton, status))
}

pub(crate) fn library_drop_zone_card<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(app.library_card_font_size(FontSize::SM))
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
    )
    .width(app.library_grid_card_width())
    .height(app.library_card_estimated_height(&EntryId::new("__drop_zone__")))
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

pub(crate) fn library_drop_zone_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
    )
    .width(Length::Fill)
    .height(app.layout().library_list_row_height)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

pub(crate) fn library_metadata_density_picker<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    pick_list(
        LIBRARY_METADATA_DENSITY_OPTIONS,
        Some(app.library_metadata_density),
        Message::LibraryMetadataDensityChanged,
    )
    .placeholder("Metadata")
    .width(130.0)
    .padding([Spacing::SM, Spacing::MD])
    .text_size(FontSize::MD)
    .font(ui_font(FontWeight::MEDIUM))
    .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
    .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown))
    .into()
}

pub(crate) fn library_entry_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let author = entry
        .display_author
        .clone()
        .or_else(|| entry.author.clone())
        .unwrap_or_else(|| String::from("Unknown author"));
    let metadata_label = library_card_metadata_label(app.library_metadata_density, &entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let progress_value = progress_fraction(&entry);
    let media = card_thumbnail_media(app, &entry_id, tokens, content_alpha);
    let title_font_size = app.library_card_title_font_size();
    let metadata_font_size = app.library_card_font_size(FontSize::SM);
    let text_width = app.library_card_title_width();
    let author = truncate_for_width_with_font(&author, text_width, 0.0, metadata_font_size);
    let metadata_label = metadata_label
        .map(|label| truncate_for_width_with_font(&label, text_width, 0.0, metadata_font_size));
    let search_match = search_match
        .map(|label| truncate_for_width_with_font(&label, text_width, 0.0, metadata_font_size));
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = LIBRARY_CARD_HOVER_LIFT * (1.0 - hover_progress);
    let bottom_lift_space = LIBRARY_CARD_HOVER_LIFT * hover_progress;
    let mut info = column![
        truncated_title(title, text_width, tokens, content_alpha, title_font_size),
        text(author)
            .size(metadata_font_size)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary)
            .wrapping(Wrapping::None),
    ]
    .spacing(app.library_card_spacing())
    .padding(app.library_card_padding())
    .height(app.library_card_info_height())
    .width(Length::Fill);
    if let Some(metadata_label) = metadata_label {
        info = info.push(
            text(metadata_label)
                .size(metadata_font_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(text_secondary)
                .wrapping(Wrapping::None),
        );
    }
    if let Some(search_match) = search_match {
        info = info.push(
            text(search_match)
                .size(metadata_font_size)
                .font(ui_font(FontWeight::MEDIUM))
                .color(accent)
                .wrapping(Wrapping::None),
        );
    }
    info = info.push(progress_bar(progress_value, tokens));

    if mode == LibraryEntryRenderMode::Normal && app.tag_entry_id.as_ref() == Some(&entry_id) {
        info = info.push(
            text_input("Tag", &app.tag_input)
                .on_input(Message::TagInputChanged)
                .on_submit(Message::SubmitTag),
        );
    }
    let checkbox_visible = selected
        || !app.selected_library_entries.is_empty()
        || app.library_card_hover_progress(&entry_id) > 0.01;
    let media = if mode == LibraryEntryRenderMode::Normal && checkbox_visible {
        stack![
            media,
            container(selection_checkbox(
                selected,
                tokens,
                Message::EntryCheckboxToggled(entry_id.clone())
            ))
            .padding(Spacing::SM)
            .width(Length::Shrink)
            .height(Length::Shrink),
        ]
        .into()
    } else {
        media
    };
    let body = column![media, info].spacing(0).width(Length::Fill);
    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(app.library_grid_card_width())
    } else {
        Length::Fixed(app.library_grid_card_width())
    };
    let surface = container(body).width(width).clip(true).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryCard, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app.library_drag.as_ref().is_some_and(|drag| drag.active) {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

pub(crate) fn library_entry_row<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
    mode: LibraryEntryRenderMode,
) -> Element<'a, Message> {
    let entry_id = entry.id.clone();
    let selected = app.selected_library_entries.contains(&entry_id);
    let title = entry_title(&entry);
    let details = library_row_metadata_label(app.library_metadata_density, &entry);
    let tags = entry.tags.clone();
    let progress_value = progress_fraction(&entry);
    let search_match = library_search_match_label(app, &entry, &entry_id);
    let content_alpha = library_entry_content_alpha(app, mode);
    let hover_progress = if mode == LibraryEntryRenderMode::Normal {
        app.library_card_hover_progress(&entry_id)
    } else {
        0.0
    };
    let top_lift_space = LIBRARY_ROW_HOVER_LIFT * (1.0 - hover_progress);
    let bottom_lift_space = LIBRARY_ROW_HOVER_LIFT * hover_progress;
    let text_secondary = with_alpha(tokens.text_secondary, content_alpha);
    let accent = with_alpha(tokens.accent, content_alpha);
    let mut detail_column = column![
        truncated_title(
            title,
            app.layout().library_row_title_width,
            tokens,
            content_alpha,
            16
        ),
        text(details)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(text_secondary),
    ]
    .spacing(Spacing::XS)
    .width(Length::Fill);
    if let Some(match_label) = search_match {
        detail_column = detail_column.push(
            text(match_label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(accent),
        );
    }
    detail_column = detail_column.push(if mode != LibraryEntryRenderMode::Normal {
        ghost_tags_row(tags, tokens, content_alpha)
    } else {
        tags_row(entry_id.clone(), tags, tokens)
    });
    let checkbox_lane: Element<'a, Message> = if mode == LibraryEntryRenderMode::Normal
        && (selected
            || !app.selected_library_entries.is_empty()
            || app.library_card_hover_progress(&entry_id) > 0.01)
    {
        selection_checkbox(
            selected,
            tokens,
            Message::EntryCheckboxToggled(entry_id.clone()),
        )
        .into()
    } else {
        container("").width(Length::Fixed(24.0)).into()
    };
    let row_content = row![
        checkbox_lane,
        thumbnail_element(
            app,
            &entry_id,
            tokens,
            app.layout().library_row_thumbnail_width,
            content_alpha
        ),
        detail_column,
        column![progress_bar(progress_value, tokens),]
            .spacing(Spacing::XS)
            .width(app.layout().library_row_progress_width),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let width = if mode == LibraryEntryRenderMode::Floating {
        Length::Fixed(720.0)
    } else {
        Length::Fill
    };
    let surface = container(row_content).width(width).style(move |_| {
        library_entry_container_style(tokens, Class::LibraryRow, mode, selected, hover_progress)
    });
    let lifted_surface = column![
        container("").height(top_lift_space),
        surface,
        container("").height(bottom_lift_space),
    ]
    .spacing(0)
    .width(width);

    if mode != LibraryEntryRenderMode::Normal {
        lifted_surface.into()
    } else {
        let area = mouse_area(lifted_surface)
            .on_enter(Message::LibraryEntryHoverChanged(entry_id.clone(), true))
            .on_exit(Message::LibraryEntryHoverChanged(entry_id.clone(), false))
            .on_press(Message::BeginLibraryEntryDrag(entry_id.clone()))
            .on_release(Message::EndLibraryEntryDrag);
        if app.library_drag.as_ref().is_some_and(|drag| drag.active) {
            area.interaction(mouse::Interaction::Grabbing).into()
        } else {
            area.into()
        }
    }
}

pub(crate) fn library_entry_container_style(
    tokens: ThemeTokens,
    class: Class,
    mode: LibraryEntryRenderMode,
    selected: bool,
    hover_progress: f32,
) -> iced::widget::container::Style {
    let mut style = container_style(tokens, class);
    match mode {
        LibraryEntryRenderMode::Normal => {
            let hover_progress = hover_progress.clamp(0.0, 1.0);
            let normal_style = tokens.class_styles[class.index()].resolve(ComponentState::Normal);
            let hovered_style = tokens.class_styles[class.index()].resolve(ComponentState::Hovered);
            let normal_background = normal_style
                .background
                .or_else(|| {
                    style.background.and_then(|background| match background {
                        iced::Background::Color(color) => Some(color),
                        _ => None,
                    })
                })
                .unwrap_or(tokens.surface_raised);
            let hovered_background = hovered_style
                .background
                .unwrap_or_else(|| mix_color(normal_background, tokens.accent, 0.14));
            let normal_border = normal_style.border_color.unwrap_or(style.border.color);
            let hovered_border = hovered_style
                .border_color
                .unwrap_or_else(|| mix_color(normal_border, tokens.accent, 0.42));

            if !selected && hover_progress > 0.0 {
                style.background = Some(iced::Background::Color(mix_color(
                    normal_background,
                    hovered_background,
                    hover_progress,
                )));
                style.border.color = mix_color(normal_border, hovered_border, hover_progress);
            }

            style.shadow = iced::Shadow {
                color: with_alpha(tokens.shadow, 0.20 + 0.10 * hover_progress),
                offset: iced::Vector::new(0.0, 1.0 + 4.0 * hover_progress),
                blur_radius: 7.0 + 7.0 * hover_progress,
            };
            if selected {
                let selected_style =
                    tokens.class_styles[class.index()].resolve(ComponentState::Selected);
                if let Some(background) = selected_style.background {
                    style.background = Some(iced::Background::Color(background));
                }
                if let Some(border_color) = selected_style.border_color {
                    style.border.color = border_color;
                }
                if let Some(border_width) = selected_style.border_width {
                    style.border.width = border_width;
                }
                style.shadow = iced::Shadow {
                    color: with_alpha(tokens.shadow, 0.24 + 0.10 * hover_progress),
                    offset: iced::Vector::new(0.0, 2.0 + 4.0 * hover_progress),
                    blur_radius: 9.0 + 7.0 * hover_progress,
                };
            }
        }
        LibraryEntryRenderMode::Placeholder => {
            let placeholder_style =
                tokens.class_styles[class.index()].resolve(ComponentState::Disabled);
            style = style.with_visual_override(placeholder_style);
        }
        LibraryEntryRenderMode::Floating => {
            let floating_style = tokens.class_styles[class.index()].resolve(ComponentState::Active);
            style = style.with_visual_override(floating_style);
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 10.0),
                blur_radius: 18.0,
            };
        }
    }
    style
}

pub(crate) fn library_entry_content_alpha(app: &PDFolioApp, mode: LibraryEntryRenderMode) -> f32 {
    if mode == LibraryEntryRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

pub(crate) fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

pub(crate) fn card_thumbnail_media<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let width = app.library_grid_card_width();
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, app.thumbnail_size_for_grid_zoom()) {
        let height = (width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1)))
            .min(app.library_card_media_max_height());
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover)
                .border_radius(iced::border::bottom(crate::style::Radius::MD))
                .opacity(alpha),
        )
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    } else {
        container(document_preview_lines(
            width,
            app.library_card_media_max_height(),
            tokens,
            alpha,
        ))
        .center(width)
        .height(app.library_card_media_max_height())
        .style(move |_| flush_media_style(tokens, alpha))
        .into()
    }
}

pub(crate) fn document_preview_lines<'a>(
    width: f32,
    height: f32,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let line_widths = [0.68, 0.98, 0.78, 0.92, 0.54, 0.74, 0.98, 0.62];
    let mut lines = column![].spacing(7.0);
    for (index, fraction) in line_widths.into_iter().enumerate() {
        let color = if index == 0 {
            with_alpha(tokens.accent, alpha * 0.78)
        } else {
            with_alpha(tokens.text_secondary, alpha * 0.68)
        };
        lines = lines.push(
            container("")
                .width((width * fraction).max(12.0))
                .height(if index == 0 { 4.0 } else { 2.0 })
                .style(move |_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        radius: 1.0.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::container::Style::default()
                }),
        );
    }

    container(lines)
        .padding([14.0, 14.0])
        .width(width)
        .height(height)
        .into()
}

pub(crate) fn flush_media_style(tokens: ThemeTokens, alpha: f32) -> iced::widget::container::Style {
    let mut background = mix_color(tokens.background, tokens.surface_raised, 0.42);
    background.a *= alpha.clamp(0.0, 1.0);

    iced::widget::container::Style {
        background: Some(iced::Background::Color(background)),
        text_color: Some(with_alpha(tokens.text_secondary, alpha)),
        border: iced::Border {
            width: 0.0,
            color: with_alpha(tokens.border, alpha),
            radius: iced::border::top(crate::style::Radius::MD),
        },
        ..iced::widget::container::Style::default()
    }
}

pub(crate) fn thumbnail_element<'a>(
    app: &'a PDFolioApp,
    entry_id: &EntryId,
    tokens: ThemeTokens,
    width: f32,
    alpha: f32,
) -> Element<'a, Message> {
    let max_height = width * 1.32;
    if let Some(thumbnail) = app.thumbnail_for_entry(entry_id, ThumbnailSize::Default) {
        let height = width * f32::from(thumbnail.height) / f32::from(thumbnail.width.max(1));
        let display_height = height.min(max_height);
        container(
            image(thumbnail.handle.clone())
                .width(width)
                .height(height)
                .opacity(alpha),
        )
        .width(width)
        .height(display_height)
        .clip(true)
        .style(move |_| {
            let mut style = container_style(tokens, Class::PagePlaceholder);
            style.background = Some(iced::Background::Color(mix_color(
                tokens.background,
                tokens.surface_raised,
                0.42,
            )));
            style.border.color = mix_color(tokens.border, tokens.background, 0.28);
            if alpha < 1.0 {
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
            }
            style
        })
        .into()
    } else {
        container(
            text("PDF")
                .size(FontSize::SM)
                .color(with_alpha(tokens.text_secondary, alpha)),
        )
        .center(width)
        .height(max_height)
        .style(move |_| {
            let mut style = container_style(tokens, Class::PagePlaceholder);
            if alpha < 1.0 {
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
            }
            style
        })
        .into()
    }
}

pub(crate) fn tags_row<'a>(
    entry_id: EntryId,
    tags: Vec<String>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(
            tag_pill(tag.clone(), tokens).on_press(Message::TagFilterChanged(Some(tag.clone()))),
        );
    }
    row.push(tag_pill("+ tag", tokens).on_press(Message::StartTagEntry(entry_id)))
        .into()
}

pub(crate) fn ghost_tags_row<'a>(
    tags: Vec<String>,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(
            container(
                text(tag)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(with_alpha(tokens.text_secondary, alpha)),
            )
            .padding([Spacing::XS, Spacing::SM])
            .style(move |_| {
                let mut style = container_style(tokens, Class::TagPill);
                if let Some(iced::Background::Color(mut background)) = style.background {
                    background.a *= alpha;
                    style.background = Some(iced::Background::Color(background));
                }
                style.border.color = with_alpha(style.border.color, alpha);
                style
            }),
        );
    }
    row.into()
}
