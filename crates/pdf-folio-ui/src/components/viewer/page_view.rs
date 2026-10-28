//! # Viewer page-view controls
//!
//! Scroll-mode and spread-mode menu under `components::viewer::page_view`, used
//! by the viewer toolbar. The toolbar button opens a dropdown listing
//! [`ViewerScrollMode`] and [`ViewerSpreadMode`] options (PDF.js-style page
//! view), each with a matching icon.
//!
//! ## Ownership
//!
//! Presentation only: reads `app.viewer.viewer_scroll_mode` /
//! `viewer_spread_mode` and emits toggle / mode-selection messages handled by
//! viewer update. Capture layer and dropdown stacking live in
//! [`super::toolbar`].
//!
//! Related: zoom menu in [`super::zoom`]; visibility menu in [`super::toolbar`].

use crate::style::menu_style_for_class;
use crate::*;
use iced::widget::{button, column, container, row, text, Space, Svg};
use iced::{Alignment, Length};

/// Compact document-view icon button that toggles the page-view menu.
///
/// Uses the same paint box as the find / visibility toolbar icons
/// (`tool_icon_size` + 10 → 28px hit target) so the center-cluster glyph
/// sits optically with the right-side tool cluster.
pub(crate) fn page_view_control<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    // Match find / eye-off: 18px glyph in a 28px transparent hit target.
    let icon_size = app
        .layout()
        .metric("ViewerToolbarChrome", "tool_icon_size", 18.0);
    let button_size = (icon_size + 10.0).max(
        app.layout()
            .metric("ViewerPageViewControl", "button_size", 28.0),
    );
    let icon_color = tokens.text_primary;
    let glyph = Svg::new(iced::widget::svg::Handle::from_memory(DOCUMENT_VIEW_SVG))
        .width(icon_size)
        .height(icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });

    let control = button(
        container(glyph)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill),
    )
    .width(Length::Fixed(button_size))
    .height(Length::Fixed(button_size))
    .padding(0.0)
    .on_press(Message::TogglePageViewMenu)
    .style(move |_, status| {
        crate::components::viewer::page_controls::transparent_toolbar_icon_style(
            tokens, status, true,
        )
    });

    tooltip(
        control,
        container(
            text("Page view")
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(std::time::Duration::from_millis(400))
    .into()
}

/// Dropdown listing scroll modes then spread modes, with the active option accented.
pub(crate) fn page_view_menu<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let row_height = app
        .layout()
        .metric("ViewerPageViewMenu", "menu_row_height", 30.0);
    let menu_width = app
        .layout()
        .metric("ViewerPageViewMenu", "menu_width", 200.0);
    let menu_padding = app
        .layout()
        .metric("ViewerPageViewMenu", "menu_padding", 4.0);
    let icon_size = app
        .layout()
        .metric("ViewerPageViewMenu", "row_icon_size", 16.0);

    let mut options = column![].spacing(0).padding(menu_padding);

    for mode in ViewerScrollMode::ALL {
        let active = app.viewer.viewer_scroll_mode == mode;
        options = options.push(page_view_menu_row(
            mode.label(),
            scroll_mode_icon(mode),
            icon_size,
            active,
            Message::ViewerScrollModeSelected(mode),
            tokens,
            row_height,
        ));
    }

    options = options.push(page_view_menu_separator(tokens, menu_width, menu_padding));

    for mode in ViewerSpreadMode::ALL {
        let active = app.viewer.viewer_spread_mode == mode;
        options = options.push(page_view_menu_row(
            mode.label(),
            spread_mode_icon(mode),
            icon_size,
            active,
            Message::ViewerSpreadModeSelected(mode),
            tokens,
            row_height,
        ));
    }

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

/// One labeled row with a leading icon; accented when it matches the active mode.
fn page_view_menu_row<'a>(
    label: &'static str,
    icon: &'static [u8],
    icon_size: f32,
    active: bool,
    on_press: Message,
    tokens: ThemeTokens,
    row_height: f32,
) -> Element<'a, Message> {
    let icon_color = if active {
        tokens.accent
    } else {
        tokens.text_primary
    };
    let glyph = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(icon_size)
        .height(icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });

    let mut label_text = text(label)
        .size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    if active {
        label_text = label_text.color(tokens.accent);
    }

    let pad_x = tokens.class_styles[Class::ViewerZoomMenuItem.index()]
        .layout
        .padding_x(Spacing::SM);

    button(
        row![glyph, label_text]
            .spacing(Spacing::SM)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(row_height))
            .padding([0.0, pad_x]),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_height))
    .padding(0.0)
    .on_press(on_press)
    .style(move |_, status| {
        let mut style = button_style(tokens, Class::ViewerZoomMenuItem, status);
        if active {
            let active_style = tokens.class_styles[Class::ViewerZoomMenuItem.index()]
                .resolve(ComponentState::Active);
            style = style.with_visual_override(active_style);
        }
        style
    })
    .into()
}

/// Thin horizontal rule between scroll-mode and spread-mode sections.
fn page_view_menu_separator<'a>(
    tokens: ThemeTokens,
    menu_width: f32,
    menu_padding: f32,
) -> Element<'a, Message> {
    container(
        container(Space::new().width(Length::Fill).height(1.0)).style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                tokens.border.r,
                tokens.border.g,
                tokens.border.b,
                0.55,
            ))),
            ..container::Style::default()
        }),
    )
    .width(Length::Fixed((menu_width - menu_padding * 2.0).max(1.0)))
    .padding([Spacing::XS, 0.0])
    .center_x(Length::Fill)
    .into()
}

fn scroll_mode_icon(mode: ViewerScrollMode) -> &'static [u8] {
    match mode {
        ViewerScrollMode::Page => SCROLL_PAGE_SVG,
        ViewerScrollMode::Vertical => SCROLL_VERTICAL_SVG,
        ViewerScrollMode::Horizontal => SCROLL_HORIZONTAL_SVG,
        ViewerScrollMode::Wrapped => SCROLL_WRAPPED_SVG,
    }
}

fn spread_mode_icon(mode: ViewerSpreadMode) -> &'static [u8] {
    match mode {
        ViewerSpreadMode::None => SPREAD_NONE_SVG,
        ViewerSpreadMode::Odd => SPREAD_ODD_SVG,
        ViewerSpreadMode::Even => SPREAD_EVEN_SVG,
    }
}
