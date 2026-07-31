//! # Find-in-document bar
//!
//! Floating find chrome under `components::viewer::find_bar`. Provides the
//! search field, match counter, previous/next buttons, compact option toggles
//! (highlight all / case / diacritics), and dismiss control while a document
//! is open.
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

/// Find bar body: query field, match fraction, prev/next, and compact toggles.
fn view_viewer_find_bar(app: &PDFolioApp, tokens: ThemeTokens, width: f32) -> Element<'_, Message> {
    let current = app.viewer.viewer_find.selected.map_or(0, |index| index + 1);
    let total = app.viewer.viewer_find.matches.len();
    let layers_pending = !app.viewer.pending_text_layers.is_empty();
    let fraction = if layers_pending && total == 0 && !app.viewer.viewer_find.query.is_empty() {
        String::from("…")
    } else if layers_pending {
        format!("{current}/{total}…")
    } else {
        format!("{current}/{total}")
    };

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
            160.0
        ),)),
        text(fraction)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "counter_width",
                56.0
            ),)),
        viewer_find_icon_button(app.layout(), CHEVRON_UP_SVG, "Previous match (Shift+F3)", tokens)
            .on_press(Message::ViewerFindPrevious),
        viewer_find_icon_button(app.layout(), CHEVRON_DOWN_SVG, "Next match (F3)", tokens)
            .on_press(Message::ViewerFindNext),
        find_option_toggle(
            "All",
            "Highlight all matches",
            app.viewer.viewer_find.highlight_all,
            tokens,
            Message::ViewerFindHighlightAllToggled(!app.viewer.viewer_find.highlight_all),
            app.layout(),
        ),
        find_option_toggle(
            "Aa",
            "Match case",
            app.viewer.viewer_find.match_case,
            tokens,
            Message::ViewerFindMatchCaseToggled(!app.viewer.viewer_find.match_case),
            app.layout(),
        ),
        find_option_toggle(
            "á",
            "Match diacritics",
            app.viewer.viewer_find.match_diacritics,
            tokens,
            Message::ViewerFindMatchDiacriticsToggled(!app.viewer.viewer_find.match_diacritics),
            app.layout(),
        ),
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

/// Compact labeled toggle used for find options (saves horizontal space vs checkboxes).
fn find_option_toggle<'a>(
    label: &'static str,
    tooltip_label: &'static str,
    active: bool,
    tokens: ThemeTokens,
    message: Message,
    layout: &crate::style::AppLayoutTokens,
) -> Element<'a, Message> {
    let size = layout.metric("ViewerFindBar", "button_size", 30.0);
    let content = container(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.accent
            } else {
                tokens.text_secondary
            })
            .wrapping(Wrapping::None),
    )
    .center(Length::Fill);

    button(
        tooltip(content, tooltip_label, tooltip::Position::Top)
            .style(move |_| container_style(tokens, Class::Tooltip)),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .padding(layout.metric("ViewerFindBar", "button_padding", 0.0))
    .on_press(message)
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::ViewerFindButton, status);
        if active {
            let active_style = tokens.class_styles[Class::ViewerFindButton.index()]
                .resolve(ComponentState::Pressed);
            style = style.with_visual_override(active_style);
        }
        style
    })
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
