//! # Page navigation controls
//!
//! Page chrome under `components::viewer::page_controls` for the open
//! document: previous/next chevrons, current-page readout with inline edit,
//! and a modal jump-to-page dialog.
//!
//! ## Ownership
//!
//! Embedded in [`super::toolbar`]; emits `PreviousPage` / `NextPage` /
//! `StartPageInputEdit` / `Jump*` / `CloseOverlay` messages handled by viewer
//! and shell update. Does not own document page count—callers pass
//! `current_page` and `page_count`.
//!
//! Related: zoom readout in [`super::zoom`]; outline jumps in [`super::outline`].

use crate::*;
use iced::widget::{row, Svg};
use iced::{Background, Border};

/// Compact prev / page-number / next control for the viewer toolbar.
///
/// Rendered as a single cohesive pill: transparent chevrons flank a
/// `current / total` readout. Double-clicking the page number starts inline
/// edit (`page_input_editing`).
pub(crate) fn viewer_page_control<'a>(
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
    let primary_color = tokens.text_primary;
    let chevron_size = app.layout().viewer_page_chevron_size;
    let number_width = app.layout().viewer_page_number_width;
    let text_size = control_text.size.unwrap_or(FontSize::MD);
    let text_weight = control_text.weight.unwrap_or(FontWeight::MEDIUM);

    let numerator: Element<'a, Message> = if app.viewer.page_input_editing {
        text_input("", &app.viewer.jump_input)
            .id(iced::widget::Id::new(PAGE_INPUT_ID))
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .padding([
                control_layout.padding_y(Spacing::XS),
                control_layout.padding_x(Spacing::SM),
            ])
            .size(text_size)
            .font(ui_font(text_weight))
            .width(Length::Fixed(number_width))
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
            .into()
    } else {
        mouse_area(
            container(
                row![
                    text(current_page.to_string())
                        .size(text_size)
                        .font(ui_font(FontWeight::SEMIBOLD))
                        .color(primary_color)
                        .wrapping(Wrapping::None),
                    text(format!(" / {page_count}"))
                        .size(text_size)
                        .font(ui_font(text_weight))
                        .color(control_color)
                        .wrapping(Wrapping::None),
                ]
                .spacing(0)
                .align_y(iced::Alignment::Center),
            )
            .width(Length::Fixed(number_width + 28.0))
            .height(Length::Fixed(chevron_size))
            .center(Length::Fill),
        )
        .on_double_click(Message::StartPageInputEdit)
        .into()
    };

    let cluster = row![
        viewer_page_chevron_button(app.layout(), CHEVRON_LEFT_SVG, tokens)
            .on_press(Message::PreviousPage)
            .width(Length::Fixed(chevron_size))
            .height(Length::Fixed(chevron_size)),
        numerator,
        viewer_page_chevron_button(app.layout(), CHEVRON_RIGHT_SVG, tokens)
            .on_press(Message::NextPage)
            .width(Length::Fixed(chevron_size))
            .height(Length::Fixed(chevron_size)),
    ]
    .spacing(control_layout.spacing.unwrap_or(2.0))
    .align_y(iced::Alignment::Center)
    .padding([
        control_layout.padding_y(2.0),
        control_layout.padding_x(4.0),
    ]);

    container(cluster)
        .width(Length::Fixed(app.layout().viewer_page_control_width))
        .height(Length::Fixed(chevron_size + 4.0))
        .center_y(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::from_rgba(
                tokens.surface_raised.r,
                tokens.surface_raised.g,
                tokens.surface_raised.b,
                0.45,
            ))),
            border: Border {
                color: Color::from_rgba(tokens.border.r, tokens.border.g, tokens.border.b, 0.55),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// Transparent SVG chevron for previous/next page (no solid chrome).
fn viewer_page_chevron_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    icon: &'static [u8],
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let icon_size = layout.metric("ViewerToolbarChrome", "icon_size", 16.0);
    let icon_color = tokens.text_secondary;
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(icon_size)
        .height(icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });

    button(container(icon).center(Length::Fill)).padding(0.0).style(
        move |_, status| transparent_toolbar_icon_style(tokens, status, true),
    )
}

/// Shared transparent icon-button paint for toolbar glyphs (page, annotate, …).
pub(crate) fn transparent_toolbar_icon_style(
    tokens: ThemeTokens,
    status: iced::widget::button::Status,
    enabled: bool,
) -> iced::widget::button::Style {
    use iced::widget::button::Status;
    let (background, text_color) = if !enabled {
        (None, tokens.text_secondary)
    } else {
        match status {
            Status::Hovered => (
                Some(Background::Color(Color::from_rgba(
                    tokens.accent.r,
                    tokens.accent.g,
                    tokens.accent.b,
                    0.14,
                ))),
                tokens.text_primary,
            ),
            Status::Pressed => (
                Some(Background::Color(Color::from_rgba(
                    tokens.accent.r,
                    tokens.accent.g,
                    tokens.accent.b,
                    0.22,
                ))),
                tokens.accent,
            ),
            Status::Disabled => (None, tokens.text_secondary),
            Status::Active => (None, tokens.text_primary),
        }
    };
    iced::widget::button::Style {
        background,
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 6.0.into(),
        },
        shadow: Default::default(),
        snap: false,
    }
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

/// Overlay dialog for jumping to a typed page number (`Go` / Enter / Cancel).
///
/// Stacked by the root surface when the jump overlay is open; max page comes
/// from the loaded document’s page count.
pub(crate) fn view_jump_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let max_page = app.viewer.doc.as_ref().map_or(0, |doc| doc.page_count());
    let dialog = row![
        text("Go to page")
            .size(FontSize::CONTROL)
            .color(tokens.text_primary),
        text_input("Page", &app.viewer.jump_input)
            .on_input(Message::JumpInputChanged)
            .on_submit(Message::SubmitJump)
            .style(move |_, status| text_input_style(tokens, Class::ViewerFindInput, status))
            .width(app.layout().jump_input_width),
        text(format!("of {max_page}"))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        toolbar_button("Go", tokens).on_press(Message::SubmitJump),
        toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::MD)
    .align_y(iced::Alignment::Center);

    container(dialog)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::JumpOverlay))
        .into()
}
