//! Viewer page geometry: ranges, offsets, spread layout helpers.
//!
//! Pure functions used by navigation, canvas drawing, and prefetch. Spread
//! grouping pairs pages for odd/even two-page modes; prefetch order expands a
//! visible page range into a priority list for tile renders; group width/height
//! helpers feed content-size and scroll clamping.
//!
//! Related: [`super::navigation`] consumes rects and offsets,
//! [`super::rendering`] supplies zoom widths,
//! [`super::state`] hosts the higher-level page-rect builders on `PDFolioApp`.

use crate::*;

/// Groups page indices into single-page or two-page spreads for `spread_mode`.
pub(crate) fn viewer_spread_groups(
    page_count: u16,
    spread_mode: ViewerSpreadMode,
) -> Vec<Vec<u16>> {
    match spread_mode {
        ViewerSpreadMode::None => (0..page_count).map(|page| vec![page]).collect(),
        ViewerSpreadMode::Odd => {
            let mut groups = Vec::new();
            let mut page = 0;
            while page < page_count {
                let mut group = vec![page];
                if page + 1 < page_count {
                    group.push(page + 1);
                }
                groups.push(group);
                page = page.saturating_add(2);
            }
            groups
        }
        ViewerSpreadMode::Even => {
            let mut groups = Vec::new();
            if page_count > 0 {
                groups.push(vec![0]);
            }
            let mut page = 1;
            while page < page_count {
                let mut group = vec![page];
                if page + 1 < page_count {
                    group.push(page + 1);
                }
                groups.push(group);
                page = page.saturating_add(2);
            }
            groups
        }
    }
}

/// Ordered page indices to render: visible range first, then neighbors ahead/behind.
pub(crate) fn prefetch_page_order_for_range(
    visible: std::ops::Range<u16>,
    page_count: u16,
    scrolling_forward: bool,
) -> Vec<u16> {
    if page_count == 0 || visible.start >= page_count {
        return Vec::new();
    }

    let start = visible.start.min(page_count);
    let end = visible
        .end
        .min(page_count)
        .max(start.saturating_add(1).min(page_count));
    let mut pages = Vec::new();

    for page in start..end {
        push_unique_page(&mut pages, page, page_count);
    }

    if start > 0 {
        push_unique_page(&mut pages, start - 1, page_count);
    }
    push_unique_page(&mut pages, end, page_count);

    if scrolling_forward {
        push_unique_page(&mut pages, end.saturating_add(1), page_count);
        push_unique_page(&mut pages, end.saturating_add(2), page_count);
    } else {
        if start > 1 {
            push_unique_page(&mut pages, start - 2, page_count);
        }
        if start > 2 {
            push_unique_page(&mut pages, start - 3, page_count);
        }
    }

    pages
}

/// Appends `page` to `pages` if in range and not already present.
pub(crate) fn push_unique_page(pages: &mut Vec<u16>, page: u16, page_count: u16) {
    if page < page_count && !pages.contains(&page) {
        pages.push(page);
    }
}

/// Picks the best available tile key for `target` page, preferring exact width.
///
/// During debounced zoom, `preview_width_px` keeps showing the previous
/// resolution until the new render arrives. When no exact match exists,
/// returns the closest width for that page.
pub(crate) fn selected_render_key<'a>(
    keys: impl Iterator<Item = &'a TileKey>,
    target: TileKey,
    preview_width_px: Option<u16>,
    include_exact: bool,
) -> Option<TileKey> {
    let keys = keys
        .filter(|candidate| candidate.page == target.page)
        .copied()
        .collect::<Vec<_>>();

    if include_exact && keys.contains(&target) {
        return Some(target);
    }

    if let Some(width_px) = preview_width_px {
        let preview = TileKey { width_px, ..target };
        if preview != target && keys.contains(&preview) {
            return Some(preview);
        }
    }

    keys.into_iter()
        .filter(|candidate| include_exact || *candidate != target)
        .min_by_key(|candidate| candidate.width_px.abs_diff(target.width_px))
}

/// Layout width of one spread group at the current zoom width.
pub(crate) fn viewer_group_width(app: &PDFolioApp, group: &[u16]) -> f32 {
    if group.is_empty() {
        return 0.0;
    }

    f32::from(app.viewer.zoom_width) * group.len() as f32
        + Spacing::PAGE_GAP * group.len().saturating_sub(1) as f32
}

/// Layout height of one spread group (tallest page in the group).
pub(crate) fn viewer_group_height(app: &PDFolioApp, group: &[u16]) -> f32 {
    group
        .iter()
        .map(|&page| app.page_height(page))
        .fold(0.0, f32::max)
}

/// Max group width plus horizontal gutters (content width for vertical layout).
pub(crate) fn viewer_groups_max_width(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    groups
        .iter()
        .map(|group| viewer_group_width(app, group))
        .fold(0.0, f32::max)
        + Spacing::PAGE_GUTTER * 2.0
}

/// Max group height across spreads (content height for horizontal layout).
pub(crate) fn viewer_groups_max_height(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    groups
        .iter()
        .map(|group| viewer_group_height(app, group))
        .fold(0.0, f32::max)
}

/// Total inline width of all groups laid out left-to-right with gaps.
pub(crate) fn viewer_groups_inline_width(app: &PDFolioApp, groups: &[Vec<u16>]) -> f32 {
    if groups.is_empty() {
        return app.viewer.viewer_viewport_width.max(1.0);
    }

    let groups_width: f32 = groups
        .iter()
        .map(|group| viewer_group_width(app, group))
        .sum();
    groups_width
        + Spacing::PAGE_GAP * groups.len().saturating_sub(1) as f32
        + Spacing::PAGE_GUTTER * 2.0
}

/// Axis-aligned rectangle intersection test for visibility culling.
pub(crate) fn rects_intersect(a: Rectangle, b: Rectangle) -> bool {
    a.x <= b.x + b.width && a.x + a.width >= b.x && a.y <= b.y + b.height && a.y + a.height >= b.y
}
