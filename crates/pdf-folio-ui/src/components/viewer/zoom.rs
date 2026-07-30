//! # Viewer zoom controls
//!
//! Zoom percentage control and preset menu used by the viewer toolbar.

use crate::style::menu_style_for_class;
use crate::viewer::rendering::{zoom_percent, zoom_percent_label, ZoomPreset, ZOOM_INPUT_ID};
use crate::*;
use iced::widget::text::Wrapping;
use iced::widget::{button, column, row, Svg};
use iced::{alignment, Alignment};

const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;

/// Compact zoom readout and buttons for the viewer toolbar.
pub(crate) fn zoom_control<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let value: Element<'a, Message> = if app.viewer.zoom_editing {
        text_input("", &app.viewer.zoom_input)
            .id(iced::widget::Id::new(ZOOM_INPUT_ID))
            .on_input(Message::ZoomInputChanged)
            .on_submit(Message::SubmitZoomInput)
            .padding([Spacing::XS, Spacing::SM])
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .width(Length::Fixed(app.layout().metric(
                "ViewerZoomControl",
                "input_width",
                58.0,
            )))
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
            .into()
    } else {
        mouse_area(
            container(
                text(zoom_percent_label(app.viewer.zoom_width))
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_secondary)
                    .wrapping(Wrapping::None),
            )
            .width(Length::Fixed(app.layout().metric(
                "ViewerZoomControl",
                "input_width",
                58.0,
            )))
            .height(Length::Fixed(app.layout().metric(
                "ViewerZoomControl",
                "button_size",
                28.0,
            )))
            .center(Length::Fill),
        )
        .on_double_click(Message::StartZoomInputEdit)
        .into()
    };

    row![
        value,
        zoom_chevron_button(app.layout(), tokens)
            .on_press(Message::ToggleZoomMenu)
            .width(Length::Fixed(app.layout().metric(
                "ViewerZoomControl",
                "button_size",
                28.0
            ),))
            .height(Length::Fixed(app.layout().metric(
                "ViewerZoomControl",
                "button_size",
                28.0
            ),))
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .width(Length::Fixed(app.layout().viewer_zoom_control_width))
    .into()
}

/// Expanded zoom menu with presets and fit-width options.
pub(crate) fn zoom_menu<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let mut options = column![].spacing(0).padding(app.layout().metric(
        "ViewerZoomControl",
        "menu_padding",
        0.0,
    ));

    for (index, preset) in ZoomPreset::ALL.into_iter().enumerate() {
        let active = preset_matches_current(preset, app);
        options = options.push(zoom_menu_row(
            preset,
            index % 2 == 1,
            active,
            tokens,
            app.layout().viewer_zoom_menu_row_height,
        ));
    }

    container(options)
        .width(Length::Fixed(app.layout().viewer_zoom_menu_width))
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

fn zoom_chevron_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(CHEVRON_DOWN_SVG))
        .width(layout.metric("ViewerZoomControl", "icon_size", 16.0))
        .height(layout.metric("ViewerZoomControl", "icon_size", 16.0))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });

    button(container(icon).center(Length::Fill))
        .padding(layout.metric("ViewerZoomControl", "button_padding", 0.0))
        .style(move |_, status| button_style(tokens, Class::ViewerToolbarButton, status))
}

fn zoom_menu_row<'a>(
    preset: ZoomPreset,
    _striped: bool,
    active: bool,
    tokens: ThemeTokens,
    row_height: f32,
) -> Element<'a, Message> {
    let mut label = text(preset.to_string())
        .size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    if active {
        label = label.color(tokens.accent);
    }

    button(
        container(label)
            .width(Length::Fill)
            .height(Length::Fixed(row_height))
            .padding([
                0.0,
                tokens.class_styles[Class::ViewerZoomMenuItem.index()]
                    .layout
                    .padding_x(Spacing::SM),
            ])
            .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(row_height))
    .padding(
        tokens.class_styles[Class::ViewerZoomMenuItem.index()]
            .layout
            .padding_x(0.0),
    )
    .on_press(Message::ZoomPresetSelected(preset))
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

fn preset_matches_current(preset: ZoomPreset, app: &PDFolioApp) -> bool {
    match preset {
        ZoomPreset::Percent(percent) => zoom_percent(app.viewer.zoom_width) == percent,
        _ => preset.width_for(app) == app.viewer.zoom_width,
    }
}
