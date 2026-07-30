//! Viewer zoom presets and page-relative rendering math.
//!
//! Converts between percent labels, logical page widths, and viewport-fit
//! presets used by the toolbar zoom control. Dimension-dependent presets
//! (`Automatic`, `PageFit`, `PageWidth`) recompute when the viewer canvas size
//! changes; percent and actual-size presets are absolute.
//!
//! Related: [`super::navigation::PDFolioApp::zoom_to_width`] applies a width,
//! [`ZoomRenderPolicy`] chooses immediate vs debounced re-render,
//! [`crate::components::viewer::zoom`] renders the control UI.

use std::fmt;

use crate::style::Spacing;
use crate::viewer::state::ViewerSpreadMode;
use crate::PDFolioApp;

/// iced widget id for the editable zoom percent field.
pub(crate) const ZOOM_INPUT_ID: &str = "viewer-zoom-input";

const ACTUAL_SIZE_WIDTH: u16 = 800;
/// Minimum allowed zoom page width in logical pixels.
pub(crate) const MIN_ZOOM_WIDTH: u16 = 240;
/// Maximum allowed zoom page width in logical pixels.
pub(crate) const MAX_ZOOM_WIDTH: u16 = 3200;
const READING_WIDTH_FILL: f32 = 0.86;
const READING_HEIGHT_MULTIPLIER: f32 = 1.75;

/// Named zoom presets offered by the viewer zoom menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomPreset {
    Automatic,
    ActualSize,
    PageFit,
    PageWidth,
    Percent(u16),
}

impl ZoomPreset {
    /// All presets in menu order (named modes, then common percents).
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

    /// Resolves this preset to a clamped page width for the current viewport/doc.
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

    /// Returns whether dimension dependent.
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

/// Formats `width` as a percent string relative to actual-size (800px).
pub(crate) fn zoom_percent_label(width: u16) -> String {
    format!("{}%", zoom_percent(width))
}

/// Converts a page width to a rounded percent of actual size.
pub(crate) fn zoom_percent(width: u16) -> u16 {
    ((f32::from(width) / f32::from(ACTUAL_SIZE_WIDTH)) * 100.0).round() as u16
}

/// Parses a user-typed percent (optional `%` suffix) into a clamped page width.
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
        .filter_map(|&page| {
            app.viewer
                .page_aspect_ratios
                .get(usize::from(page))
                .copied()
        })
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
        .viewer
        .doc
        .as_ref()
        .map_or(app.viewer.page_aspect_ratios.len() as u16, |doc| {
            doc.page_count()
        });
    if page_count == 0 {
        return vec![0];
    }

    let page = app.current_page().min(page_count.saturating_sub(1));
    match app.viewer.viewer_spread_mode {
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
    (app.viewer.viewer_viewport_width - Spacing::PAGE_GUTTER * 2.0).max(f32::from(MIN_ZOOM_WIDTH))
}

fn available_page_height(app: &PDFolioApp) -> f32 {
    (app.viewer.viewer_viewport_height - Spacing::PAGE_GUTTER * 2.0).max(f32::from(MIN_ZOOM_WIDTH))
}

/// When to schedule page re-renders after a zoom width change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoomRenderPolicy {
    /// Request tiles immediately (toolbar buttons, presets, shortcuts).
    Immediate,
    /// Wait for wheel gesture idle before re-rasterizing (smooth live zoom).
    Debounced,
}
