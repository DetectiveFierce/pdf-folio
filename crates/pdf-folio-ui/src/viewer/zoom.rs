//! Viewer zoom control, presets, and page-relative zoom math.

use std::fmt;

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, mouse_area, row, text, text_input, Svg};
use iced::{alignment, Alignment, Background, Element, Length};

use crate::messages::Message;
use crate::style::{
    button_style, menu_style_for_class, mix_color, text_input_style, ui_font, Class,
    ComponentState, FontSize, FontWeight, Spacing, ThemeTokens, VisualOverride,
};
use crate::viewer::state::ViewerSpreadMode;
use crate::PDFolioApp;

const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;

pub(crate) const ZOOM_INPUT_ID: &str = "viewer-zoom-input";
pub(crate) const ZOOM_CONTROL_WIDTH: f32 = 98.0;
pub(crate) const ZOOM_MENU_WIDTH: f32 = 118.0;

const ACTUAL_SIZE_WIDTH: u16 = 800;
pub(crate) const MIN_ZOOM_WIDTH: u16 = 240;
pub(crate) const MAX_ZOOM_WIDTH: u16 = 3200;
const ZOOM_MENU_ROW_HEIGHT: f32 = 22.0;
const READING_WIDTH_FILL: f32 = 0.86;
const READING_HEIGHT_MULTIPLIER: f32 = 1.75;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomPreset {
    Automatic,
    ActualSize,
    PageFit,
    PageWidth,
    Percent(u16),
}

impl ZoomPreset {
    pub(crate) const ALL: [Self; 12] = [
        Self::Automatic,
        Self::ActualSize,
        Self::PageFit,
        Self::PageWidth,
        Self::Percent(50),
        Self::Percent(75),
        Self::Percent(100),
        Self::Percent(125),
        Self::Percent(150),
        Self::Percent(200),
        Self::Percent(300),
        Self::Percent(400),
    ];

    pub(crate) fn width_for(self, app: &PDFolioApp) -> u16 {
        match self {
            Self::Automatic => automatic_zoom_width(app),
            Self::ActualSize => ACTUAL_SIZE_WIDTH,
            Self::PageFit => page_fit_width(app),
            Self::PageWidth => page_width_zoom(app),
            Self::Percent(percent) => percent_width(percent),
        }
        .clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH)
    }

    pub(crate) fn is_dimension_dependent(self) -> bool {
        matches!(self, Self::Automatic | Self::PageFit | Self::PageWidth)
    }
}

impl fmt::Display for ZoomPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Automatic => f.write_str("Automatic Zoom"),
            Self::ActualSize => f.write_str("Actual Size"),
            Self::PageFit => f.write_str("Page Fit"),
            Self::PageWidth => f.write_str("Page Width"),
            Self::Percent(percent) => write!(f, "{percent}%"),
        }
    }
}

pub(crate) fn zoom_percent_label(width: u16) -> String {
    format!("{}%", zoom_percent(width))
}

pub(crate) fn zoom_percent(width: u16) -> u16 {
    ((f32::from(width) / f32::from(ACTUAL_SIZE_WIDTH)) * 100.0).round() as u16
}

pub(crate) fn width_from_percent_input(input: &str) -> Option<u16> {
    let normalized = input.trim().trim_end_matches('%').trim();
    if normalized.is_empty() {
        return None;
    }
    let percent = normalized.parse::<f32>().ok()?;
    percent
        .is_finite()
        .then(|| ((percent / 100.0) * f32::from(ACTUAL_SIZE_WIDTH)).round())
        .map(|width| width.clamp(f32::from(MIN_ZOOM_WIDTH), f32::from(MAX_ZOOM_WIDTH)) as u16)
}

pub(crate) fn zoom_control<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let value: Element<'a, Message> = if app.zoom_editing {
        text_input("", &app.zoom_input)
            .id(iced::widget::Id::new(ZOOM_INPUT_ID))
            .on_input(Message::ZoomInputChanged)
            .on_submit(Message::SubmitZoomInput)
            .padding([Spacing::XS, Spacing::SM])
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .width(Length::Fixed(58.0))
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .into()
    } else {
        mouse_area(
            container(
                text(zoom_percent_label(app.zoom_width))
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_secondary)
                    .wrapping(Wrapping::None),
            )
            .width(Length::Fixed(58.0))
            .height(Length::Fixed(28.0))
            .center(Length::Fill),
        )
        .on_double_click(Message::StartZoomInputEdit)
        .into()
    };

    row![
        value,
        zoom_chevron_button(tokens)
            .on_press(Message::ToggleZoomMenu)
            .width(Length::Fixed(28.0))
            .height(Length::Fixed(28.0))
    ]
    .spacing(0)
    .align_y(Alignment::Center)
    .width(Length::Fixed(ZOOM_CONTROL_WIDTH))
    .into()
}

pub(crate) fn zoom_menu<'a>(app: &'a PDFolioApp, tokens: ThemeTokens) -> Element<'a, Message> {
    let mut options = column![].spacing(0).padding(0);

    for (index, preset) in ZoomPreset::ALL.into_iter().enumerate() {
        let active = preset_matches_current(preset, app);
        options = options.push(zoom_menu_row(preset, index % 2 == 1, active, tokens));
    }

    container(options)
        .width(Length::Fixed(ZOOM_MENU_WIDTH))
        .style(move |_| {
            let menu = menu_style_for_class(tokens, Class::MenuPanel);
            container::Style {
                background: Some(menu.background),
                border: menu.border,
                shadow: menu.shadow,
                ..container::Style::default()
            }
        })
        .into()
}

fn zoom_chevron_button<'a>(tokens: ThemeTokens) -> iced::widget::Button<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(CHEVRON_DOWN_SVG))
        .width(16.0)
        .height(16.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });

    button(container(icon).center(Length::Fill))
        .padding(0)
        .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

fn zoom_menu_row<'a>(
    preset: ZoomPreset,
    striped: bool,
    active: bool,
    tokens: ThemeTokens,
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
            .height(Length::Fixed(ZOOM_MENU_ROW_HEIGHT))
            .padding([0.0, Spacing::SM])
            .align_y(alignment::Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ZOOM_MENU_ROW_HEIGHT))
    .padding(0)
    .on_press(Message::ZoomPresetSelected(preset))
    .style(move |_, status| {
        let mut style = button_style(tokens, Class::MenuItem, status);
        let row_base = if striped {
            mix_color(tokens.surface_raised, tokens.accent, 0.08)
        } else {
            mix_color(tokens.surface_raised, tokens.background, 0.10)
        };
        let row_background = match status {
            iced::widget::button::Status::Hovered => mix_color(row_base, tokens.accent, 0.18),
            iced::widget::button::Status::Pressed => mix_color(row_base, tokens.accent, 0.28),
            iced::widget::button::Status::Disabled | iced::widget::button::Status::Active => {
                row_base
            }
        };
        style.background = Some(Background::Color(row_background));
        if active {
            let active_style =
                tokens.class_styles[Class::MenuItem.index()].resolve(ComponentState::Active);
            style = style.with_visual_override(active_style);
            style.background = Some(Background::Color(mix_color(
                row_background,
                tokens.accent,
                0.18,
            )));
        }
        style.border.radius = 0.0.into();
        style.border.width = 0.0;
        style
    })
    .into()
}

fn preset_matches_current(preset: ZoomPreset, app: &PDFolioApp) -> bool {
    match preset {
        ZoomPreset::Percent(percent) => zoom_percent(app.zoom_width) == percent,
        _ => preset.width_for(app) == app.zoom_width,
    }
}

fn automatic_zoom_width(app: &PDFolioApp) -> u16 {
    let metrics = current_spread_metrics(app);
    let width_target = page_width_for_group(
        available_page_width(app) * READING_WIDTH_FILL,
        metrics.page_count,
    );
    let height_target =
        available_page_height(app) * READING_HEIGHT_MULTIPLIER * metrics.min_aspect_ratio;

    width_target.min(height_target).round() as u16
}

fn page_width_zoom(app: &PDFolioApp) -> u16 {
    page_width_for_group(
        available_page_width(app),
        current_spread_metrics(app).page_count,
    )
    .round() as u16
}

fn page_fit_width(app: &PDFolioApp) -> u16 {
    let metrics = current_spread_metrics(app);
    let available_width = available_page_width(app);
    let available_height = available_page_height(app);
    page_width_for_group(available_width, metrics.page_count)
        .min(available_height * metrics.min_aspect_ratio)
        .round() as u16
}

fn percent_width(percent: u16) -> u16 {
    ((f32::from(ACTUAL_SIZE_WIDTH) * f32::from(percent)) / 100.0).round() as u16
}

#[derive(Debug, Clone, Copy)]
struct SpreadZoomMetrics {
    page_count: usize,
    min_aspect_ratio: f32,
}

fn current_spread_metrics(app: &PDFolioApp) -> SpreadZoomMetrics {
    let pages = current_spread_pages(app);
    let min_aspect_ratio = pages
        .iter()
        .filter_map(|&page| app.page_aspect_ratios.get(usize::from(page)).copied())
        .fold(f32::INFINITY, f32::min);

    SpreadZoomMetrics {
        page_count: pages.len().max(1),
        min_aspect_ratio: if min_aspect_ratio.is_finite() {
            min_aspect_ratio.max(0.01)
        } else {
            (8.5_f32 / 11.0).max(0.01)
        },
    }
}

fn current_spread_pages(app: &PDFolioApp) -> Vec<u16> {
    let page_count = app
        .doc
        .as_ref()
        .map_or(app.page_aspect_ratios.len() as u16, |doc| doc.page_count());
    if page_count == 0 {
        return vec![0];
    }

    let page = app.current_page().min(page_count.saturating_sub(1));
    match app.viewer_spread_mode {
        ViewerSpreadMode::None => vec![page],
        ViewerSpreadMode::Odd => {
            let left = page - (page % 2);
            if left + 1 < page_count {
                vec![left, left + 1]
            } else {
                vec![left]
            }
        }
        ViewerSpreadMode::Even => {
            if page == 0 {
                vec![0]
            } else {
                let left = page - ((page - 1) % 2);
                if left + 1 < page_count {
                    vec![left, left + 1]
                } else {
                    vec![left]
                }
            }
        }
    }
}

fn page_width_for_group(total_width: f32, page_count: usize) -> f32 {
    let page_count = page_count.max(1);
    let gaps = Spacing::PAGE_GAP * page_count.saturating_sub(1) as f32;
    ((total_width - gaps) / page_count as f32).max(1.0)
}

fn available_page_width(app: &PDFolioApp) -> f32 {
    (app.viewer_viewport_width - Spacing::PAGE_GUTTER * 2.0).max(f32::from(MIN_ZOOM_WIDTH))
}

fn available_page_height(app: &PDFolioApp) -> f32 {
    (app.viewer_viewport_height - Spacing::PAGE_GUTTER * 2.0).max(f32::from(MIN_ZOOM_WIDTH))
}
