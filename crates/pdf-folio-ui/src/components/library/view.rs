//! # Reusable library toolbar widgets
//!
//! Message-generic controls: layout toggle, grid zoom, sort/density pickers,
//! scrollable shell, and new-folder button styling. Domain views bind them to
//! concrete `Message` constructors.

use iced::widget::scrollable::{Direction, Scrollbar, Viewport};
use iced::widget::{button, container, pick_list, row, scrollable, slider, text, tooltip, Svg};
use iced::{Element, Length};
use pdf_folio_core::LibrarySortMode;
use pdf_folio_style::{
    button_style, container_style, menu_style_for_class, pick_list_style, slider_style, ui_font,
    Class, ComponentState, FontSize, FontWeight, Spacing, ThemeTokens, VisualOverride,
};
use std::time::Duration;

use crate::library::state::LibraryMetadataDensity;

/// LIBRARY SCROLLABLE ID constant used by this module.
pub const LIBRARY_SCROLLABLE_ID: &str = "library-scrollable";

/// Returns a copy with alpha applied.
pub fn with_alpha(mut color: iced::Color, alpha: f32) -> iced::Color {
    color.a *= alpha.clamp(0.0, 1.0);
    color
}

/// Breadcrumb button.
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

/// Library scrollable.
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

/// Library layout toggle button.
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

/// Library grid zoom control.
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

/// Library new folder button.
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

/// Library metadata density picker.
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

/// Library sort picker.
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
