//! # Find-in-document bar
//!
//! Floating find chrome under `components::viewer::find_bar`. Provides the
//! search field, match counter, previous/next buttons, highlight-all and
//! case/diacritic toggles, and dismiss control while a document is open.
//!
//! ## Ownership
//!
//! Reads `app.viewer.viewer_find` query and match state; emits
//! `ViewerFind*` / `CloseViewerFind` messages handled by viewer update.
//! Positioning is a bottom-right anchor over the viewer surface; the host
//! domain view decides when the bar is shown.
//!
//! Related: text selection and canvas hit-testing in [`super::canvas`];
//! find is also reachable from [`super::toolbar`] and context menus.

use crate::*;
use iced::widget::{row, Svg};

/// Bottom-right anchor that hosts the find bar over the viewer content area.
///
/// `width` is the bar’s fixed layout width from the host; the bar itself is
/// only built when the domain view includes this element.
pub(crate) fn viewer_find_anchor(
    app: &PDFolioApp,
    tokens: ThemeTokens,
    width: f32,
) -> Element<'_, Message> {
    container(view_viewer_find_bar(app, tokens, width))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// Find bar body: query field, match fraction, prev/next, and option toggles.
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

/// Icon button with tooltip used for find-bar prev/next and option controls.
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
