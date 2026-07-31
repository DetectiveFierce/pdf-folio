//! # Viewer toolbar
//!
//! Top bar under `components::viewer::toolbar` while a document is open.
//!
//! ## Layout
//!
//! Three-column row with equal `Length::Fill` side panes so the center cluster
//! stays fixed when selection tools appear on the right:
//!
//! | Region | Contents |
//! | --- | --- |
//! | **Left** | Library back, Open PDF, resolved document title |
//! | **Center** | Page control pill + zoom − / % / + |
//! | **Right** | Selection tools (when active), find, visibility (eye-off), theme |
//!
//! The document title is [`ViewerRuntime::document_title`](crate::viewer::document::ViewerRuntime):
//! provisional library/path seed, then PDF metadata in the background.
//!
//! ## Menus
//!
//! Zoom presets and the visibility menu (Hide Sidebar? / Hide Comments?) are
//! stacked by [`crate::components::shared::root_surface`] with outside-click
//! capture. The visibility panel’s right edge aligns with the eye-off button.
//! Menu open/close re-applies scroll offsets so chrome does not jump reading
//! position.
//!
//! ## Ownership
//!
//! Composes widgets from [`super::page_controls`] and [`super::zoom`]; emits
//! navigation, zoom, find, and chrome messages. Domain viewer view embeds
//! this bar above the canvas stack.
//!
//! Related: find bar in [`super::find_bar`]; annotations in [`super::annotations`];
//! context menus on the canvas for overlapping actions (zoom, find, TOC).

use crate::components::shared::sidebar::chevron_button;
use crate::components::viewer::page_controls::{
    transparent_toolbar_icon_style, viewer_page_control,
};
use crate::components::viewer::zoom::{zoom_control, zoom_menu};
use crate::style::menu_style_for_class;
use crate::*;
use iced::widget::{column, pin, row, Svg};
use iced::Alignment;
use std::time::Duration;

/// Compose the viewer top toolbar for the current document session.
///
/// Three-column layout: identity chrome on the left, page + zoom dead-center,
/// and selection / find / visibility / theme flush to the right. Zoom lives in
/// the center cluster so it does not shift when selection tools appear.
/// Equal `Length::Fill` side panes keep the center fixed; extra space is
/// between groups, not inside button padding.
pub(crate) fn view_viewer_toolbar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    // Spacing between adjacent controls (not internal button padding).
    let gap = toolbar_layout.spacing.unwrap_or(Spacing::MD);
    let page_count = app.viewer.doc.as_ref().map_or(0, |doc| doc.page_count());
    let current_page = if page_count == 0 {
        0
    } else {
        app.current_page().saturating_add(1).min(page_count)
    };
    // Prefer background-loaded PDF metadata / library title; fall back while loading.
    let document_title = app
        .viewer
        .document_title
        .as_deref()
        .unwrap_or("Open PDF");
    let theme_label = match app.appearance.theme {
        AppTheme::Light => "Dark",
        AppTheme::Dark => "Light",
    };
    let title_width = viewer_toolbar_title_width(app);
    let tool_icon = app
        .layout()
        .metric("ViewerToolbarChrome", "tool_icon_size", 18.0);

    // Left: library / open / document title.
    let left = row![
        viewer_library_back_button(app.layout(), tokens).on_press(Message::BackToLibrary),
        toolbar_button("Open PDF", tokens).on_press(Message::OpenFileDialog),
        viewer_toolbar_title(document_title, title_width, tokens),
    ]
    .spacing(gap)
    .align_y(Alignment::Center);

    // Center: page navigator + zoom (stable; not affected by selection chrome).
    let center = row![
        viewer_page_control(app, current_page, page_count, tokens),
        icon_button("-", tokens).on_press(Message::ZoomOut),
        zoom_control(app, tokens),
        icon_button("+", tokens).on_press(Message::ZoomIn),
    ]
    .spacing(gap)
    .align_y(Alignment::Center);

    // Right: selection tools, find, visibility, theme — flush right.
    let mut right = row![];

    if let Some(selection) = app.viewer.viewer_text_selection {
        let (start, end) = selection.ordered();
        let label = if start.page == end.page {
            let count = end.char_index.saturating_sub(start.char_index) + 1;
            format!("{count} char{} selected", if count == 1 { "" } else { "s" })
        } else {
            format!("{} pages selected", end.page.saturating_sub(start.page) + 1)
        };
        right = right.push(viewer_toolbar_status_label(
            label,
            app.layout().viewer_toolbar_selection_width,
            tokens,
        ));
        if app.can_annotate() {
            right = right.push(viewer_toolbar_icon_button(
                app.layout(),
                tokens,
                ANNOTATE_SVG,
                "annotate selected text",
                Message::StartAnnotationCompose,
                app.layout()
                    .metric("ViewerToolbarChrome", "annotate_icon_size", 22.0),
                !selection.dragging,
            ));
        }
        right = right
            .push(viewer_toolbar_icon_button(
                app.layout(),
                tokens,
                COPY_SVG,
                "copy selected text",
                Message::CopyViewerTextSelection,
                tool_icon,
                !selection.dragging,
            ))
            .push(toolbar_button("Clear", tokens).on_press(Message::ClearViewerTextSelection));
    }

    let right = right
        .push(viewer_toolbar_icon_button(
            app.layout(),
            tokens,
            FIND_SVG,
            "find in document",
            Message::OpenViewerFind,
            // Same paint box as the eye control so the two trailing icons match.
            tool_icon,
            true,
        ))
        .push(viewer_visibility_icon_button(app.layout(), tokens))
        .push(toolbar_button(theme_label, tokens).on_press(Message::ThemeToggled))
        .spacing(gap)
        .align_y(Alignment::Center);

    let toolbar = row![
        container(left)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(Alignment::Center),
        center,
        container(right)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .align_y(Alignment::Center),
    ]
    .spacing(gap)
    .padding([
        toolbar_layout.padding_y(Spacing::SM),
        toolbar_layout.padding_x(Spacing::MD),
    ])
    .height(toolbar_layout.height.unwrap_or(app.layout().toolbar_height))
    .width(Length::Fill)
    .align_y(Alignment::Center);

    container(toolbar)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::ViewerToolbar))
        .into()
}

/// Transparent icon-only toolbar control with tooltip.
fn viewer_toolbar_icon_button<'a>(
    _layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
    icon: &'static [u8],
    tip: &'static str,
    on_press: Message,
    icon_size: f32,
    enabled: bool,
) -> Element<'a, Message> {
    let button_size = (icon_size + 10.0).max(28.0);
    let icon_color = if enabled {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let glyph = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(icon_size)
        .height(icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });

    let mut control = button(
        container(glyph)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill),
    )
    .width(Length::Fixed(button_size))
    .height(Length::Fixed(button_size))
    .padding(0.0)
    .style(move |_, status| transparent_toolbar_icon_style(tokens, status, enabled));

    if enabled {
        control = control.on_press(on_press);
    }

    tooltip(
        control,
        container(
            text(tip)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(400))
    .into()
}

/// Eye-off control that opens the hide-sidebar / hide-comments menu.
fn viewer_visibility_icon_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    viewer_toolbar_icon_button(
        layout,
        tokens,
        EYE_OFF_SVG,
        "visibility",
        Message::ToggleVisibilityMenu,
        layout.metric("ViewerToolbarChrome", "tool_icon_size", 18.0),
        true,
    )
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
        tokens.text_primary,
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

/// Full-window transparent layer that closes the visibility menu on outside click.
pub(crate) fn visibility_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseVisibilityMenu),
    )
    .y(app.layout().toolbar_height)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Positioned visibility dropdown (Hide Sidebar? / Hide Comments?).
///
/// Right edge of the panel aligns with the right edge of the eye-off button
/// (not the theme button further right).
pub(crate) fn view_visibility_menu_dropdown(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let gap = toolbar_layout.spacing.unwrap_or(Spacing::MD);
    let right_padding = toolbar_layout.padding_right(Spacing::MD);
    // Toolbar right: [… find] [eye] [theme] | padding
    // Inset so the menu’s right edge matches the eye’s right edge.
    let right_inset = right_padding + viewer_theme_button_width(app, tokens) + gap;
    pin(
        container(visibility_menu(app, tokens))
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .padding(iced::Padding {
                top: 0.0,
                right: right_inset,
                bottom: 0.0,
                left: 0.0,
            }),
    )
    .y(app.layout().toolbar_height)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Estimated width of the Light/Dark theme toolbar button (label + padding).
fn viewer_theme_button_width(app: &PDFolioApp, tokens: ThemeTokens) -> f32 {
    let label = match app.appearance.theme {
        AppTheme::Light => "Dark",
        AppTheme::Dark => "Light",
    };
    let button_layout = tokens.class_styles[Class::ToolbarButton.index()].layout;
    let text_style = tokens.class_styles[Class::ToolbarButton.index()].text;
    let pad_x = button_layout.padding_x(Spacing::LG);
    let size = text_style.size.unwrap_or(FontSize::MD) as f32;
    // Medium-weight sans approx advance for "Dark"/"Light".
    let text_width = size * 0.58 * label.len() as f32;
    let estimated = pad_x * 2.0 + text_width;
    // Optional layout override; do not let a large stale metric pull the menu
    // too far left of the eye control.
    let override_width = app
        .layout()
        .metric("ViewerToolbarChrome", "theme_button_width", estimated);
    if (override_width - estimated).abs() < 20.0 {
        override_width
    } else {
        estimated
    }
}

/// Two-row Yes/No menu for sidebar and comments visibility.
fn visibility_menu<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let menu_width = app
        .layout()
        .metric("ViewerVisibilityMenu", "menu_width", 220.0);
    let row_height = app
        .layout()
        .metric("ViewerVisibilityMenu", "menu_row_height", 34.0);

    // “Hide X?” Yes = currently hidden; No = currently visible.
    let hide_sidebar = !app.viewer.toc_open;
    let hide_comments = !app.viewer.annotations_visible;

    let options = column![
        visibility_menu_row(
            "Hide Sidebar?",
            hide_sidebar,
            Message::ToggleHideSidebar,
            tokens,
            row_height,
        ),
        visibility_menu_row(
            "Hide Comments?",
            hide_comments,
            Message::ToggleHideComments,
            tokens,
            row_height,
        ),
    ]
    .spacing(0)
    .padding(app.layout().metric("ViewerVisibilityMenu", "menu_padding", 4.0));

    container(options)
        .width(Length::Fixed(menu_width))
        .style(move |_| {
            let menu = menu_style_for_class(tokens, Class::ViewerZoomMenu);
            container::Style {
                background: Some(menu.background),
                border: menu.border,
                shadow: menu.shadow,
                ..container::Style::default()
            }
        })
        .into()
}

/// One visibility row: label on the left, Yes (green) or No (red) on the right.
fn visibility_menu_row<'a>(
    label: &'static str,
    is_yes: bool,
    on_press: Message,
    tokens: ThemeTokens,
    row_height: f32,
) -> Element<'a, Message> {
    let answer = if is_yes { "Yes" } else { "No" };
    // Yes = green, No = red.
    let answer_color = if is_yes {
        Color::from_rgb(0.35, 0.72, 0.45)
    } else {
        tokens.error
    };

    button(
        row![
            text(label)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
            iced::widget::Space::new().width(Length::Fill),
            text(answer)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(answer_color)
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::MD)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .padding([0.0, Spacing::SM])
        .height(Length::Fixed(row_height)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_height))
    .padding(0.0)
    .on_press(on_press)
    .style(move |_, status| button_style(tokens, Class::ViewerZoomMenuItem, status))
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

/// Approximate width of the fixed center cluster (page control + zoom).
fn viewer_toolbar_center_width(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let gap = toolbar_layout.spacing.unwrap_or(Spacing::MD);
    let zoom_step = app
        .layout()
        .metric("ViewerToolbarChrome", "zoom_step_button_width", 30.0);
    app.layout().viewer_page_control_width
        + gap
        + zoom_step
        + gap
        + app.layout().viewer_zoom_control_width
        + gap
        + zoom_step
}

/// Title width for the left identity cluster (library + open + title).
///
/// Uses roughly half the bar (minus center cluster / padding) so the left pane
/// never fights the centered page+zoom cluster or the right tool cluster.
fn viewer_toolbar_title_width(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let gap = toolbar_layout.spacing.unwrap_or(Spacing::MD);
    let padding_x = toolbar_layout.padding_x(Spacing::MD);
    // Equal side panes share (viewport − center cluster − outer padding − gaps).
    let side_pane = ((app.viewer.viewport_width
        - viewer_toolbar_center_width(app)
        - padding_x * 2.0
        - gap * 2.0)
        * 0.5)
        .max(0.0);
    let left_chrome = app
        .layout()
        .metric("ViewerToolbarChrome", "library_button_width", 70.0)
        + gap
        + app
            .layout()
            .metric("ViewerToolbarChrome", "open_button_width", 87.0)
        + gap;
    (side_pane - left_chrome).clamp(
        app.layout().viewer_toolbar_title_min_width,
        app.layout().viewer_toolbar_title_max_width,
    )
}

/// X pin for the zoom dropdown under the center-cluster zoom control.
fn viewer_zoom_menu_x(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let toolbar_layout = tokens.class_styles[Class::ViewerToolbar.index()].layout;
    let gap = toolbar_layout.spacing.unwrap_or(Spacing::MD);
    let padding_x = toolbar_layout.padding_x(Spacing::MD);
    let zoom_step = app
        .layout()
        .metric("ViewerToolbarChrome", "zoom_step_button_width", 30.0);
    let center_width = viewer_toolbar_center_width(app);
    // Center cluster is horizontally centered in the bar.
    let center_left =
        (app.viewer.viewport_width - center_width) * 0.5;
    // Zoom control sits after page control, gap, and the “−” step button.
    let zoom_control_left = center_left
        + app.layout().viewer_page_control_width
        + gap
        + zoom_step
        + gap;
    (zoom_control_left + app.layout().viewer_zoom_control_width
        - app.layout().viewer_zoom_menu_width)
        .max(padding_x)
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
