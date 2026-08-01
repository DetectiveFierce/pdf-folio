//! Pure annotation placement and mark geometry for the document viewer.
//!
//! Owns content-space layout for anchored comment cards and a single fallback
//! policy for mapping annotation character ranges onto page rectangles. Presentational
//! widgets in [`crate::components::viewer::annotations`] call into this module;
//! scroll/content-size helpers on [`crate::PDFolioApp`] do the same so domain
//! never depends on components for geometry.
//!
//! # Fallback policy (anchors + mark bounds)
//!
//! 1. Start character bounds when present in the text layer
//! 2. Midpoint / union of the annotation’s char range on the start page
//! 3. Page upper-third synthetic mark when the text layer is missing or empty

use std::collections::HashMap;
use std::sync::Arc;

use iced::{Point, Rectangle, Size};
use pdf_folio_core::{Annotation, AnnotationId, PageTextLayer, TextRect};

use crate::PDFolioApp;

/// Fixed card column width (mockup `.annotation-layer` width ≈ 292).
pub(crate) const CARD_WIDTH: f32 = 292.0;
/// Gap between the page’s right edge and the card column.
const PAGE_CARD_GAP: f32 = 18.0;
/// Horizontal margin after the card column (extends content width).
const LAYER_RIGHT_MARGIN: f32 = 24.0;
/// Vertical gap between stacked cards (mockup `gap = 12`).
const CARD_STACK_GAP: f32 = 12.0;
/// Minimum top inset for the first card in content space (mockup `minTop = 36`).
const LAYER_MIN_TOP: f32 = 36.0;
/// Extra content height past the last card bottom (mockup `+ 52`).
const LAYER_BOTTOM_PAD: f32 = 52.0;

/// Resolved layout for one annotation card in document content coordinates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AnchoredCardPlacement {
    /// Index into `viewer.annotations` (document order).
    pub index: usize,
    /// Content-space top of the card.
    pub top: f32,
    /// Estimated card height used for collision layout.
    pub height: f32,
    /// Content-space left of the card column.
    pub x: f32,
}

/// Full metrics for the annotation layer (placements + content extents).
#[derive(Debug, Clone)]
pub(crate) struct AnnotationLayerMetrics {
    /// Per-card placements in document order (same length as annotations that
    /// could be laid out; missing text layers still get a page fallback).
    pub placements: Vec<AnchoredCardPlacement>,
    /// Required content width so the card column is not clipped.
    pub content_width: f32,
    /// Required content height so the last card is not clipped.
    pub content_height: f32,
}

/// Map normalized character bounds into content/page-rect space.
///
/// `bounds` are unit-square fractions relative to the PDF page (origin top-left).
pub(crate) fn character_content_rect(page_rect: Rectangle, bounds: &TextRect) -> Rectangle {
    Rectangle::new(
        Point::new(
            page_rect.x + bounds.x * page_rect.width,
            page_rect.y + bounds.y * page_rect.height,
        ),
        Size::new(
            bounds.width * page_rect.width,
            bounds.height * page_rect.height,
        ),
    )
}

/// Content-space bounds of an annotation mark with the shared fallback policy.
///
/// Returns `None` only when the start page has no page rect (page not laid out).
/// Otherwise always yields a rect so cards and scroll agree:
/// start char → range union → page upper-third synthetic mark.
pub(crate) fn annotation_mark_content_bounds(
    annotation: &Annotation,
    page_rects: &[(u16, Rectangle)],
    text_layers: &HashMap<u16, Arc<PageTextLayer>>,
) -> Option<Rectangle> {
    let page_rect = page_rects
        .iter()
        .find(|(page, _)| *page == annotation.start_page)
        .map(|(_, rect)| *rect)?;

    let layer = text_layers.get(&annotation.start_page).map(Arc::as_ref);
    Some(annotation_mark_on_page(annotation, page_rect, layer))
}

/// Content-space vertical center of the annotation’s mark plus the page right edge.
///
/// Used for card column placement. Same geometry policy as
/// [`annotation_mark_content_bounds`].
pub(crate) fn annotation_anchor_center(
    annotation: &Annotation,
    page_rects: &[(u16, Rectangle)],
    text_layers: &HashMap<u16, Arc<PageTextLayer>>,
) -> Option<(f32, f32)> {
    let page_rect = page_rects
        .iter()
        .find(|(page, _)| *page == annotation.start_page)
        .map(|(_, rect)| *rect)?;
    let layer = text_layers.get(&annotation.start_page).map(Arc::as_ref);
    let mark = annotation_mark_on_page(annotation, page_rect, layer);
    let center_y = mark.y + mark.height * 0.5;
    Some((center_y, page_rect.x + page_rect.width))
}

/// Mark geometry on a known page rect (shared by bounds + anchor helpers).
fn annotation_mark_on_page(
    annotation: &Annotation,
    page_rect: Rectangle,
    layer: Option<&PageTextLayer>,
) -> Rectangle {
    if let Some(layer) = layer {
        if let Some(character) = layer.chars.get(annotation.start_char) {
            return clamp_mark_size(character_content_rect(page_rect, &character.bounds));
        }
        if let Some(range) =
            PDFolioApp::annotation_char_range_for_page(annotation, annotation.start_page, layer.chars.len())
        {
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            let mut any = false;
            for idx in range {
                if let Some(character) = layer.chars.get(idx) {
                    let rect = character_content_rect(page_rect, &character.bounds);
                    min_x = min_x.min(rect.x);
                    min_y = min_y.min(rect.y);
                    max_x = max_x.max(rect.x + rect.width);
                    max_y = max_y.max(rect.y + rect.height);
                    any = true;
                }
            }
            if any && min_x < max_x && min_y < max_y {
                return clamp_mark_size(Rectangle::new(
                    Point::new(min_x, min_y),
                    Size::new(max_x - min_x, max_y - min_y),
                ));
            }
        }
    }

    // Text layer not ready / missing chars: synthetic mark near the upper third
    // (center matches the previous card-path fallback at `y + height * 0.2`).
    let height = (page_rect.height * 0.05).max(8.0);
    let y = page_rect.y + page_rect.height * 0.2 - height * 0.5;
    Rectangle::new(
        Point::new(page_rect.x, y),
        Size::new(page_rect.width.max(4.0), height),
    )
}

fn clamp_mark_size(rect: Rectangle) -> Rectangle {
    Rectangle::new(
        rect.position(),
        Size::new(rect.width.max(4.0), rect.height.max(8.0)),
    )
}

/// Computes card placements and the content size needed to host them.
///
/// Pure-ish: takes page rects, annotations, text layers, and the editing id
/// rather than a full app borrow.
pub(crate) fn annotation_layer_metrics(
    annotations: &[Annotation],
    page_rects: &[(u16, Rectangle)],
    text_layers: &HashMap<u16, Arc<PageTextLayer>>,
    editing_id: Option<&AnnotationId>,
    base_content: Size,
) -> AnnotationLayerMetrics {
    if annotations.is_empty() {
        return AnnotationLayerMetrics {
            placements: Vec::new(),
            content_width: base_content.width,
            content_height: base_content.height,
        };
    }

    let mut anchors: Vec<(usize, f32 /*center_y*/, f32 /*height*/, f32 /*page_right*/)> =
        Vec::with_capacity(annotations.len());

    for (index, annotation) in annotations.iter().enumerate() {
        let height = estimate_card_height(annotation, editing_id == Some(&annotation.id));
        let (center_y, page_right) =
            annotation_anchor_center(annotation, page_rects, text_layers).unwrap_or_else(|| {
                // Fallback: top of first page or content top.
                page_rects
                    .first()
                    .map(|(_, r)| (r.y + 40.0, r.x + r.width))
                    .unwrap_or((LAYER_MIN_TOP + height * 0.5, base_content.width * 0.55))
            });
        anchors.push((index, center_y, height, page_right));
    }

    // Shared column x: right of the rightmost page edge we need, with a floor so
    // sparse docs still leave a readable gutter next to the page block.
    let max_page_right = anchors
        .iter()
        .map(|(_, _, _, right)| *right)
        .fold(0.0_f32, f32::max);
    let column_x = (max_page_right + PAGE_CARD_GAP)
        .max(base_content.width - CARD_WIDTH - LAYER_RIGHT_MARGIN)
        .max(0.0);

    let layout_items: Vec<(f32, f32)> = anchors
        .iter()
        .map(|(_, center, height, _)| (*center, *height))
        .collect();
    let tops = layout_anchored_tops(&layout_items, LAYER_MIN_TOP, CARD_STACK_GAP);

    let mut placements = Vec::with_capacity(annotations.len());
    let mut max_bottom = 0.0_f32;
    for ((index, _center, height, _), top) in anchors.iter().zip(tops.iter()) {
        let top = *top;
        max_bottom = max_bottom.max(top + height);
        placements.push(AnchoredCardPlacement {
            index: *index,
            top,
            height: *height,
            x: column_x,
        });
    }

    AnnotationLayerMetrics {
        placements,
        content_width: base_content
            .width
            .max(column_x + CARD_WIDTH + LAYER_RIGHT_MARGIN)
            .max(1.0),
        content_height: base_content
            .height
            .max(max_bottom + LAYER_BOTTOM_PAD)
            .max(1.0),
    }
}

/// Collision-resolving vertical layout (mockup `layoutAnchored`).
///
/// `items` are `(anchor_center_y, height)` in document order. Returns the
/// resolved top for each item in the **same order**.
pub(crate) fn layout_anchored_tops(items: &[(f32, f32)], min_top: f32, gap: f32) -> Vec<f32> {
    if items.is_empty() {
        return Vec::new();
    }

    struct Work {
        order: usize,
        height: f32,
        anchor_center: f32,
        desired_top: f32,
        top: f32,
    }

    let mut work: Vec<Work> = items
        .iter()
        .enumerate()
        .map(|(order, &(center, height))| Work {
            order,
            height,
            anchor_center: center,
            desired_top: center - height * 0.5,
            top: 0.0,
        })
        .collect();
    work.sort_by(|a, b| {
        a.desired_top
            .partial_cmp(&b.desired_top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Group items that would overlap at their desired tops.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, item) in work.iter().enumerate() {
        if let Some(prev_group) = groups.last_mut() {
            let prev = &work[*prev_group.last().expect("group non-empty")];
            if item.desired_top < prev.desired_top + prev.height + gap {
                prev_group.push(i);
                continue;
            }
        }
        groups.push(vec![i]);
    }

    let mut previous_bottom = min_top - gap;
    for group in &groups {
        let total_height: f32 = group.iter().map(|&i| work[i].height).sum::<f32>()
            + gap * (group.len().saturating_sub(1) as f32);
        let average_anchor: f32 =
            group.iter().map(|&i| work[i].anchor_center).sum::<f32>() / group.len() as f32;
        let mut group_top = min_top
            .max(average_anchor - total_height * 0.5)
            .max(previous_bottom + gap);
        for &i in group {
            work[i].top = group_top;
            group_top += work[i].height + gap;
        }
        previous_bottom = group_top - gap;
    }

    let mut tops = vec![0.0; items.len()];
    for item in work {
        tops[item.order] = item.top;
    }
    tops
}

fn estimate_card_height(annotation: &Annotation, editing: bool) -> f32 {
    if editing {
        return 148.0;
    }
    // padding 14+15 + meta ~20 + body lines
    let body_chars = annotation.body.chars().count().max(1);
    let chars_per_line = 36_usize;
    let lines = ((body_chars + chars_per_line - 1) / chars_per_line).clamp(1, 10);
    14.0 + 15.0 + 20.0 + 7.0 + lines as f32 * 18.75 + 4.0
}

#[cfg(test)]
mod tests {
    use super::layout_anchored_tops;

    #[test]
    fn sparse_anchors_keep_independent_desired_tops() {
        // Far-apart anchors should not push each other.
        let items = [(100.0, 40.0), (400.0, 40.0), (800.0, 40.0)];
        let tops = layout_anchored_tops(&items, 36.0, 12.0);
        assert!((tops[0] - (100.0 - 20.0)).abs() < 0.5);
        assert!((tops[1] - (400.0 - 20.0)).abs() < 0.5);
        assert!((tops[2] - (800.0 - 20.0)).abs() < 0.5);
    }

    #[test]
    fn dense_anchors_stack_without_overlap() {
        let items = [(100.0, 50.0), (105.0, 50.0), (110.0, 50.0)];
        let tops = layout_anchored_tops(&items, 36.0, 12.0);
        assert!(tops[1] >= tops[0] + 50.0 + 12.0 - 0.1);
        assert!(tops[2] >= tops[1] + 50.0 + 12.0 - 0.1);
        // Group should start at least at min_top.
        assert!(tops[0] >= 36.0 - 0.1);
    }

    #[test]
    fn later_group_stays_below_previous_bottom() {
        let items = [(50.0, 80.0), (60.0, 80.0), (500.0, 40.0)];
        let tops = layout_anchored_tops(&items, 36.0, 12.0);
        let first_group_bottom = tops[1] + 80.0;
        assert!(tops[2] >= first_group_bottom + 12.0 - 0.1);
    }
}
