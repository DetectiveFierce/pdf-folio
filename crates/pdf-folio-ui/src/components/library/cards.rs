//! Reusable library card and row preview components.

use iced::widget::{column, container, row, text};
use iced::{Element, Length};
use pdf_folio_style::{
    container_style, mix_color, tag_pill, ui_font, Class, FontSize, FontWeight, Spacing,
    ThemeTokens,
};

use crate::components::library::view::with_alpha;

pub fn document_preview_lines<'a, Message: 'a>(
    width: f32,
    height: f32,
    tokens: ThemeTokens,
    alpha: f32,
) -> Element<'a, Message> {
    let line_widths = [0.68, 0.98, 0.78, 0.92, 0.54, 0.74, 0.98, 0.62];
    let layout = tokens.class_styles[Class::PagePlaceholder.index()].layout;
    let mut lines = column![].spacing(tokens.primitives.document_preview_line_spacing);
    for (index, fraction) in line_widths.into_iter().enumerate() {
        let color = if index == 0 {
            with_alpha(tokens.accent, alpha * 0.78)
        } else {
            with_alpha(tokens.text_secondary, alpha * 0.68)
        };
        lines = lines.push(
            container("")
                .width((width * fraction).max(tokens.primitives.document_preview_min_line_width))
                .height(if index == 0 {
                    tokens.primitives.document_preview_heading_line_height
                } else {
                    tokens.primitives.document_preview_body_line_height
                })
                .style(move |_| iced::widget::container::Style {
                    background: Some(iced::Background::Color(color)),
                    border: iced::Border {
                        radius: tokens.primitives.document_preview_line_radius.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::container::Style::default()
                }),
        );
    }

    container(lines)
        .padding([layout.padding_y(14.0), layout.padding_x(14.0)])
        .width(width)
        .height(height)
        .into()
}

pub fn flush_media_style(tokens: ThemeTokens, alpha: f32) -> iced::widget::container::Style {
    let mut style = container_style(tokens, Class::PagePlaceholder);
    let mut background = style
        .background
        .and_then(|background| match background {
            iced::Background::Color(color) => Some(color),
            _ => None,
        })
        .unwrap_or_else(|| {
            mix_color(
                tokens.background,
                tokens.surface_raised,
                tokens.primitives.flush_media_background_mix,
            )
        });
    background.a *= alpha.clamp(0.0, 1.0);

    style.background = Some(iced::Background::Color(background));
    style.text_color = Some(with_alpha(
        style.text_color.unwrap_or(tokens.text_secondary),
        alpha,
    ));
    style.border.color = with_alpha(style.border.color, alpha);
    style
}

pub fn library_drop_zone_card<'a, Message: 'a>(
    card_width: f32,
    estimated_height: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container("")
        .width(card_width)
        .height(estimated_height)
        .center(Length::Fill)
        .style(move |_| translucent_drop_zone_style(tokens))
        .into()
}

pub fn library_drop_zone_row<'a, Message: 'a>(
    row_height: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container("")
        .width(Length::Fill)
        .height(row_height)
        .center(Length::Fill)
        .style(move |_| translucent_drop_zone_style(tokens))
        .into()
}

fn translucent_drop_zone_style(tokens: ThemeTokens) -> iced::widget::container::Style {
    let mut style = container_style(tokens, Class::DragInsertionMarker);
    if let Some(iced::Background::Color(background)) = style.background {
        style.background = Some(iced::Background::Color(with_alpha(background, 0.34)));
    }
    style.border.color = with_alpha(style.border.color, 0.52);
    style
}

pub fn tags_row<'a, Message, OnTag>(
    tags: Vec<String>,
    tokens: ThemeTokens,
    on_tag: OnTag,
    start_tag_message: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
    OnTag: Fn(String) -> Message + Copy + 'a,
{
    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    for tag in tags {
        row = row.push(tag_pill(tag.clone(), tokens).on_press(on_tag(tag)));
    }
    row.push(tag_pill("+ tag", tokens).on_press(start_tag_message))
        .into()
}

pub fn ghost_tags_row<'a, Message: 'a>(
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
    row.width(Length::Shrink).into()
}
