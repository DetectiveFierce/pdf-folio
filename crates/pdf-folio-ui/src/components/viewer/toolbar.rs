//! # Viewer toolbar
//!
//! Top bar under `components::viewer::toolbar` while a document is open:
//! back to library, open PDF, document title, page controls, zoom, find,
//! theme toggle, and sidebar affordances. Also hosts the zoom dropdown and
//! its outside-click capture layer, plus the floating “show outline” control
//! when the sidebar is closed.
//!
//! ## Ownership
//!
//! Composes widgets from [`super::page_controls`] and [`super::zoom`]; emits
//! navigation, zoom, find, and chrome messages. Domain viewer view embeds
//! this bar above the canvas stack.
//!
//! Related: find bar in [`super::find_bar`]; context menus on the canvas for
//! overlapping actions (zoom, find, TOC).

use crate::components::shared::sidebar::chevron_button;
use crate::components::viewer::page_controls::viewer_page_control;
use crate::components::viewer::zoom::{zoom_control, zoom_menu};
use crate::*;
use iced::widget::{pin, row, Svg};

/// Compose the viewer top toolbar for the current document session.
pub(crate) fn view_viewer_toolbar(app: &PDFolioApp) -> Element<'_, Message> {
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

/// “← Library” toolbar button that returns to library mode.
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

/// Full-window transparent layer that closes the zoom menu on outside click.
pub(crate) fn zoom_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseZoomMenu),
    )
    .y(app.layout().toolbar_height)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Positioned zoom preset dropdown panel (content from [`super::zoom::zoom_menu`]).
pub(crate) fn view_zoom_menu_dropdown(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    pin(zoom_menu(app, tokens))
        .x(viewer_zoom_menu_x(app))
        .y(app.layout().toolbar_height)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Truncated document title for the toolbar, with a tooltip when ellipsized.
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

/// Compact status text for selection counts and similar toolbar feedback.
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

/// Available title width after reserving fixed chrome and optional selection controls.
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

/// X pin for the zoom dropdown so it aligns under the zoom control.
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

/// Floating chevron shown when the outline sidebar is closed (“Show Contents”).
pub(crate) fn viewer_floating_sidebar_toggle<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    chevron_button(
        CHEVRON_RIGHT_SVG,
        "Show Contents",
        Message::ToggleSidebar,
        tokens,
        true,
    )
}

/// Resolve themed text color for `class`/`state`, falling back to `fallback`.
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
