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

/// Compact prev / page-number / next control for the viewer toolbar.
///
/// Double-clicking the page number starts inline edit (`page_input_editing`);
/// otherwise the label shows the 1-based `current_page` against `page_count`.
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

/// Compact SVG chevron button for previous/next page on the viewer toolbar.
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
