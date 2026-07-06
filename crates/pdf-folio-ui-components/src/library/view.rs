//! Reusable rendered library UI components.

use iced::widget::scrollable::{Direction, Scrollbar, Viewport};
use iced::widget::{
    button, column, container, pick_list, row, scrollable, slider, text, tooltip, Svg,
};
use iced::{Element, Length};
use pdf_folio_db::LibrarySortMode;
use pdf_folio_style::{
    button_style, container_style, menu_style_for_class, mix_color, pick_list_style, slider_style,
    tag_pill, ui_font, Class, ComponentState, FontSize, FontWeight, Spacing, ThemeTokens,
    VisualOverride,
};
use std::time::Duration;

use crate::library::state::LibraryMetadataDensity;

pub const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";

pub fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

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

pub fn breadcrumb_button<'a, Message: Clone + 'a>(
    label: String,
    active: bool,
    tokens: ThemeTokens,
    message: Message,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.accent
            })
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_, status| {
        if active {
            let active_style =
                tokens.class_styles[Class::SidebarRow.index()].resolve(ComponentState::Active);
            button_style(tokens, Class::SidebarRow, status).with_visual_override(active_style)
        } else {
            button_style(tokens, Class::SidebarRow, status)
        }
    })
    .on_press(message)
    .into()
}

pub fn library_scrollable<'a, Message: 'a>(
    content: iced::widget::Column<'a, Message>,
    tokens: ThemeTokens,
    scrollbar_gutter: f32,
    on_scroll: impl Fn(Viewport) -> Message + 'a,
) -> Element<'a, Message> {
    let scrollbar_width = tokens.primitives.scrollbar_width;
    let scrollbar_spacing = (scrollbar_gutter - scrollbar_width).max(0.0);
    scrollable(content)
        .id(iced::widget::Id::new(LIBRARY_SCROLLABLE_ID))
        .width(Length::Fill)
        .height(Length::Fill)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(scrollbar_width)
                .scroller_width(tokens.primitives.scrollbar_scroller_width)
                .spacing(scrollbar_spacing),
        ))
        .style(move |_, status| {
            pdf_folio_style::scrollable_style(tokens, Class::LibraryRow, status)
        })
        .on_scroll(on_scroll)
        .into()
}

pub fn library_layout_toggle_button<'a, Message: Clone + 'a>(
    compact_view_mode: bool,
    tokens: ThemeTokens,
    grid_icon: &'static [u8],
    list_icon: &'static [u8],
    message: Message,
) -> Element<'a, Message> {
    let (icon, tooltip_label) = if compact_view_mode {
        (grid_icon, "Switch to grid")
    } else {
        (list_icon, "Switch to list")
    };
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(tokens.primitives.library_view_toggle_icon_size)
        .height(tokens.primitives.library_view_toggle_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_primary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(
        tokens.class_styles[Class::LibraryViewToggle.index()]
            .layout
            .width
            .unwrap_or(34.0),
    )
    .height(
        tokens.class_styles[Class::LibraryViewToggle.index()]
            .layout
            .height
            .unwrap_or(34.0),
    )
    .padding(
        tokens.class_styles[Class::LibraryViewToggle.index()]
            .layout
            .padding_x(0.0),
    )
    .style(move |_, status| button_style(tokens, Class::LibraryViewToggle, status))
    .on_press(message);

    tooltip(
        button,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

pub fn library_grid_zoom_control<'a, Message: Clone + 'a>(
    min: f32,
    max: f32,
    value: f32,
    step: f32,
    label: String,
    tokens: ThemeTokens,
    on_change: impl Fn(f32) -> Message + 'a,
) -> Element<'a, Message> {
    let control = row![
        text("Grid")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
        slider(min..=max, value, on_change)
            .step(step)
            .width(
                tokens.class_styles[Class::LibraryGridZoomSlider.index()]
                    .layout
                    .width
                    .unwrap_or(150.0),
            )
            .style(move |_, status| slider_style(tokens, Class::LibraryGridZoomSlider, status)),
        text(label)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .width(tokens.primitives.library_grid_zoom_label_width),
    ]
    .spacing(Spacing::SM)
    .align_y(iced::Alignment::Center);

    tooltip(
        control,
        container(
            text("Grid zoom")
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

pub fn library_new_folder_button<'a, Message: 'a>(
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text("New folder")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary),
    )
    .padding([
        tokens.class_styles[Class::LibraryImportButton.index()]
            .layout
            .padding_y(Spacing::SM),
        tokens.class_styles[Class::LibraryImportButton.index()]
            .layout
            .padding_x(Spacing::LG),
    ])
    .style(move |_, status| button_style(tokens, Class::LibraryImportButton, status))
}

pub fn library_drop_zone_card<'a, Message: 'a>(
    card_width: f32,
    estimated_height: f32,
    font_size: u32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(font_size)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(card_width)
    .height(estimated_height)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

pub fn library_drop_zone_row<'a, Message: 'a>(
    row_height: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        text("Drop selected PDFs here")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(iced::widget::text::Wrapping::None),
    )
    .width(Length::Fill)
    .height(row_height)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::DragInsertionMarker))
    .into()
}

pub fn library_metadata_density_picker<'a, Message: Clone + 'a>(
    selected: LibraryMetadataDensity,
    options: &'static [LibraryMetadataDensity],
    tokens: ThemeTokens,
    on_select: impl Fn(LibraryMetadataDensity) -> Message + 'a,
) -> Element<'a, Message> {
    pick_list(options, Some(selected), on_select)
        .placeholder("Metadata")
        .width(tokens.primitives.library_metadata_picker_width)
        .padding([
            tokens.class_styles[Class::LibrarySortDropdown.index()]
                .layout
                .padding_y(Spacing::SM),
            tokens.class_styles[Class::LibrarySortDropdown.index()]
                .layout
                .padding_x(Spacing::MD),
        ])
        .text_size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
        .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown))
        .into()
}

pub fn library_sort_picker<'a, Message: Clone + 'a>(
    selected: LibrarySortMode,
    options: &'static [LibrarySortMode],
    tokens: ThemeTokens,
    on_select: impl Fn(LibrarySortMode) -> Message + 'a,
) -> Element<'a, Message> {
    pick_list(options, Some(selected), on_select)
        .placeholder("Sort")
        .width(
            tokens.class_styles[Class::LibrarySortDropdown.index()]
                .layout
                .width
                .unwrap_or(190.0),
        )
        .menu_height(tokens.primitives.library_sort_menu_height)
        .padding([
            tokens.class_styles[Class::LibrarySortDropdown.index()]
                .layout
                .padding_y(Spacing::SM),
            tokens.class_styles[Class::LibrarySortDropdown.index()]
                .layout
                .padding_x(Spacing::MD),
        ])
        .text_size(FontSize::MD)
        .font(ui_font(FontWeight::MEDIUM))
        .style(move |_, status| pick_list_style(tokens, Class::LibrarySortDropdown, status))
        .menu_style(move |_| menu_style_for_class(tokens, Class::LibrarySortDropdown))
        .into()
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
