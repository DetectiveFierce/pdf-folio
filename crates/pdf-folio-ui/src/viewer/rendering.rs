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

/// Logical page width (px) treated as 100% / “actual size” for percent math.
const ACTUAL_SIZE_WIDTH: u16 = 800;
/// Minimum allowed zoom page width in logical pixels.
pub(crate) const MIN_ZOOM_WIDTH: u16 = 240;
/// Maximum allowed zoom page width in logical pixels.
pub(crate) const MAX_ZOOM_WIDTH: u16 = 3200;
/// Multiplicative zoom step for buttons, shortcuts, and Ctrl+wheel (~12% per notch).
pub(crate) const ZOOM_STEP_RATIO: f32 = 1.12;
/// Fraction of available canvas width used by [`ZoomPreset::Automatic`] (~comfortable reading).
const READING_WIDTH_FILL: f32 = 0.86;
/// Multiplier on available canvas height when height-capping automatic zoom.
const READING_HEIGHT_MULTIPLIER: f32 = 1.75;

/// Next zoom width one step larger than `width` (multiplicative, clamped).
pub(crate) fn zoom_in_width(width: u16) -> u16 {
    let next = ((f32::from(width) * ZOOM_STEP_RATIO).round() as u16).max(width.saturating_add(1));
    next.clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH)
}

/// Next zoom width one step smaller than `width` (multiplicative, clamped).
pub(crate) fn zoom_out_width(width: u16) -> u16 {
    let next = ((f32::from(width) / ZOOM_STEP_RATIO).round() as u16).min(width.saturating_sub(1));
    next.clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH)
}

/// Named zoom presets offered by the viewer zoom menu.
///
/// Dimension-dependent variants ([`Self::Automatic`], [`Self::PageFit`],
/// [`Self::PageWidth`]) recompute via [`Self::width_for`] when the viewer
/// canvas resizes; [`Self::ActualSize`] and [`Self::Percent`] are absolute
/// widths relative to the 800px “actual size” baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomPreset {
    /// Comfortable reading width (~86% of canvas width, height-capped).
    /// Default when a document first opens.
    Automatic,
    /// Fixed 800 logical-pixel page width (100% / “actual size”).
    ActualSize,
    /// Fit the current page/spread entirely inside the canvas (width and height).
    PageFit,
    /// Stretch the current page/spread to the full available canvas width.
    PageWidth,
    /// Absolute zoom as a percent of actual size (e.g. `Percent(150)` → 150%).
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

    /// Resolves this preset to a clamped page width (logical px) for the current viewport/doc.
    ///
    /// Result is always within [`MIN_ZOOM_WIDTH`]..=[`MAX_ZOOM_WIDTH`].
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

    /// Whether this preset must recompute when the viewer canvas size changes.
    ///
    /// True for [`Self::Automatic`], [`Self::PageFit`], and [`Self::PageWidth`];
    /// false for absolute [`Self::ActualSize`] / [`Self::Percent`]. Callers
    /// (e.g. `apply_active_dimension_zoom`) skip work when this returns false.
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

/// Page width for automatic zoom: min of reading-width fill and height-capped fit.
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

/// Page width that fills available canvas width for the current spread's page count.
fn page_width_zoom(app: &PDFolioApp) -> u16 {
    page_width_for_group(
        available_page_width(app),
        current_spread_metrics(app).page_count,
    )
    .round() as u16
}

/// Page width that fits the current spread entirely inside the canvas (width and height).
fn page_fit_width(app: &PDFolioApp) -> u16 {
    let metrics = current_spread_metrics(app);
    let available_width = available_page_width(app);
    let available_height = available_page_height(app);
    page_width_for_group(available_width, metrics.page_count)
        .min(available_height * metrics.min_aspect_ratio)
        .round() as u16
}

/// Convert a percent-of-actual-size value to a logical page width in pixels.
fn percent_width(percent: u16) -> u16 {
    ((f32::from(ACTUAL_SIZE_WIDTH) * f32::from(percent)) / 100.0).round() as u16
}

/// Geometry of the current page/spread used when resolving dimension-dependent zoom.
#[derive(Debug, Clone, Copy)]
struct SpreadZoomMetrics {
    /// Number of pages in the current spread group (at least 1).
    page_count: usize,
    /// Smallest width/height aspect among spread pages (defaults to letter if unknown).
    min_aspect_ratio: f32,
}

/// Build [`SpreadZoomMetrics`] for the pages currently shown under the active spread mode.
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

/// Zero-based page indices for the spread containing the current page (odd/even/none).
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

/// Per-page width after dividing `total_width` across `page_count` pages with inter-page gaps.
fn page_width_for_group(total_width: f32, page_count: usize) -> f32 {
    let page_count = page_count.max(1);
    let gaps = Spacing::PAGE_GAP * page_count.saturating_sub(1) as f32;
    ((total_width - gaps) / page_count as f32).max(1.0)
}

/// Usable canvas width for pages (viewport minus left/right gutters), floored at min zoom.
fn available_page_width(app: &PDFolioApp) -> f32 {
    (app.viewer.viewer_viewport_width - Spacing::PAGE_GUTTER * 2.0).max(f32::from(MIN_ZOOM_WIDTH))
}

/// Usable canvas height for pages (viewport minus top/bottom gutters), floored at min zoom.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_are_multiplicative_and_clamped() {
        let mid = zoom_in_width(800);
        assert!(mid > 800);
        let down = zoom_out_width(mid);
        assert!(down < mid);
        // One step out from a step in should land near the original width.
        assert!((i32::from(down) - 800).unsigned_abs() <= 2);

        assert_eq!(zoom_in_width(MAX_ZOOM_WIDTH), MAX_ZOOM_WIDTH);
        assert_eq!(zoom_out_width(MIN_ZOOM_WIDTH), MIN_ZOOM_WIDTH);
        assert!(zoom_in_width(MIN_ZOOM_WIDTH) > MIN_ZOOM_WIDTH);
    }
}
