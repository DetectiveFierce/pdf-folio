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

pub(crate) fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let base_content: Element<'_, Message> =
        if app.mode == AppMode::Viewer && app.viewer.doc.is_some() {
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

    let content = if app.chrome.pending_confirmation.is_some() {
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
                        .font(ui_font(FontWeight::SEMIBOLD))
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
