//! # Document-anchored annotation layer
//!
//! Comment cards sit in the **scrollable document content** (not a fixed
//! viewport carousel). Each card is vertically aligned to its highlight
//! anchor; overlapping cards are collision-resolved into compact stacks so
//! sparse notes stay next to their marks and dense clusters stack cleanly.
//!
//! Behavior follows the anchored-document mockup:
//! - Natural document scroll carries cards with the page
//! - Click a card or highlight to select and bring the pair into view
//! - ↑ / ↓ move to the previous / next comment
//!
//! Related: composed inside [`crate::viewer::view::document`]; highlight paint
//! and click hit-testing live in [`super::canvas`].

use crate::*;
use chrono::{DateTime, Utc};
use iced::widget::{button, column, row, stack, text_input, Space};
use iced::{Alignment, Background, Border, Color, Length, Padding, Shadow, Vector};
use pdf_folio_core::Annotation;

/// Fixed card column width (mockup `.annotation-layer` width ≈ 292).
pub(crate) const CARD_WIDTH: f32 = 292.0;
/// Ordinal badge diameter (mockup `::before` 22×22).
const BADGE_SIZE: f32 = 22.0;
/// How far the badge center sits past the card’s top-left corner.
///
/// Half the badge so the circle is centered on the corner (mockup ≈ −10/−10
/// with a 22px badge). The outer frame grows by this amount on top/left.
const BADGE_OVERHANG: f32 = BADGE_SIZE * 0.5;
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
/// Corner radius for comment cards (mockup `--radius: 12px`).
const CARD_RADIUS: f32 = 12.0;
/// Active card nudge toward the page (mockup `translateX(-4px)`).
const ACTIVE_NUDGE_X: f32 = 4.0;

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

/// Viewport-pinned chrome: compose form and empty-state hints only.
///
/// Anchored cards live in the scrollable content via
/// [`view_annotations_content_layer`].
pub(crate) fn view_annotations_viewport_chrome(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'_, Message>> {
    if let Some(compose) = app.viewer.compose() {
        let card = view_compose_card(compose, tokens);
        let x = (app.viewer.viewer_viewport_width - CARD_WIDTH - 16.0).max(Spacing::SM);
        return Some(pin(card).x(x).y(Spacing::MD).into());
    }

    if !app.can_annotate() {
        let card = hint_card(
            "Open this PDF from the library to add annotations.",
            tokens,
        );
        let x = (app.viewer.viewer_viewport_width - CARD_WIDTH - 16.0).max(Spacing::SM);
        return Some(pin(card).x(x).y(Spacing::MD).into());
    }

    if app.viewer.annotations.is_empty() {
        let card = hint_card("Select text and add an annotation.", tokens);
        let x = (app.viewer.viewer_viewport_width - CARD_WIDTH - 16.0).max(Spacing::SM);
        return Some(pin(card).x(x).y(Spacing::MD).into());
    }

    None
}

/// Scrollable content layer: document-anchored comment cards.
pub(crate) fn view_annotations_content_layer<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    content_size: Size,
) -> Element<'a, Message> {
    if app.viewer.annotations.is_empty() {
        return Space::new()
            .width(content_size.width)
            .height(content_size.height)
            .into();
    }

    // Layout against page geometry; `content_size` is already expanded to fit
    // the card column, so pass it through for width/height extents only.
    let metrics = annotation_layer_metrics(app, content_size);
    let selected = app.viewer.selected_annotation_id.as_ref();
    let total = app.viewer.annotations.len();

    let mut layers = stack![]
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height))
        .clip(false);

    // Important: iced `pin` positions its *child* at (x, y) inside the pin’s own
    // bounds, then clips draw to those bounds. Each pin must therefore span the
    // full document content, not just the card box — otherwise cards with a
    // large document Y never paint (only ones near the top remain visible).
    for placement in &metrics.placements {
        let Some(annotation) = app.viewer.annotations.get(placement.index) else {
            continue;
        };
        let active = selected == Some(&annotation.id);
        let editing = app.viewer.editing_id() == Some(&annotation.id);
        // Placement is the card’s top-left; the frame grows by BADGE_OVERHANG
        // up/left so the ordinal circle can sit on the corner without clipping.
        let mut x = placement.x - BADGE_OVERHANG;
        if active {
            x -= ACTIVE_NUDGE_X;
        }
        let y = placement.top - BADGE_OVERHANG;
        let card = view_anchored_card(
            app,
            annotation,
            placement.index,
            total,
            active,
            editing,
            tokens,
        );
        layers = layers.push(
            pin(card)
                .x(x.max(0.0))
                .y(y.max(0.0))
                .width(Length::Fixed(content_size.width))
                .height(Length::Fixed(content_size.height)),
        );
    }

    layers.into()
}

/// Computes card placements and the content size needed to host them.
pub(crate) fn annotation_layer_metrics(
    app: &PDFolioApp,
    base_content: Size,
) -> AnnotationLayerMetrics {
    let annotations = &app.viewer.annotations;
    if annotations.is_empty() {
        return AnnotationLayerMetrics {
            placements: Vec::new(),
            content_width: base_content.width,
            content_height: base_content.height,
        };
    }

    let page_rects = app.viewer_page_rects_content(app.viewer.viewer_viewport_width);
    let mut anchors: Vec<(usize, f32 /*center_y*/, f32 /*height*/, f32 /*page_right*/)> =
        Vec::with_capacity(annotations.len());

    for (index, annotation) in annotations.iter().enumerate() {
        let height = estimate_card_height(
            annotation,
            app.viewer.editing_id() == Some(&annotation.id),
        );
        let (center_y, page_right) =
            annotation_anchor_center(app, annotation, &page_rects).unwrap_or_else(|| {
                // Fallback: top of first page or content top.
                let fallback = page_rects
                    .first()
                    .map(|(_, r)| (r.y + 40.0, r.x + r.width))
                    .unwrap_or((LAYER_MIN_TOP + height * 0.5, base_content.width * 0.55));
                fallback
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
pub(crate) fn layout_anchored_tops(
    items: &[(f32, f32)],
    min_top: f32,
    gap: f32,
) -> Vec<f32> {
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

/// Content-space vertical center of the annotation’s start anchor, plus the
/// right edge of its page (for column placement).
fn annotation_anchor_center(
    app: &PDFolioApp,
    annotation: &Annotation,
    page_rects: &[(u16, Rectangle)],
) -> Option<(f32, f32)> {
    let page_rect = page_rects
        .iter()
        .find(|(page, _)| *page == annotation.start_page)
        .map(|(_, rect)| *rect)?;

    if let Some(layer) = app.viewer.viewer_text_layers.get(&annotation.start_page) {
        if let Some(character) = layer.chars.get(annotation.start_char) {
            let y = page_rect.y + character.bounds.y * page_rect.height;
            let h = character.bounds.height * page_rect.height;
            let center = y + h * 0.5;
            return Some((center, page_rect.x + page_rect.width));
        }
        // Prefer midpoint of the full range on this page when start char is missing.
        if let Some(range) =
            PDFolioApp::annotation_char_range_for_page(annotation, annotation.start_page, layer.chars.len())
        {
            let mut min_y = f32::MAX;
            let mut max_y = f32::MIN;
            for idx in range {
                if let Some(character) = layer.chars.get(idx) {
                    let y = page_rect.y + character.bounds.y * page_rect.height;
                    let h = character.bounds.height * page_rect.height;
                    min_y = min_y.min(y);
                    max_y = max_y.max(y + h);
                }
            }
            if min_y < max_y {
                return Some(((min_y + max_y) * 0.5, page_rect.x + page_rect.width));
            }
        }
    }

    // Text layer not ready: aim near the upper third of the page.
    Some((
        page_rect.y + page_rect.height * 0.2,
        page_rect.x + page_rect.width,
    ))
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

fn view_anchored_card<'a>(
    app: &'a PDFolioApp,
    annotation: &'a Annotation,
    index: usize,
    _total: usize,
    active: bool,
    editing: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let ordinal = index + 1;
    let badge = view_ordinal_badge(ordinal, active, tokens);

    if editing {
        let body_input = text_input("Annotation", app.viewer.edit_body())
            .on_input(Message::AnnotationEditBodyChanged)
            .on_submit(Message::AnnotationEditSubmitted)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status));
        let actions = row![
            toolbar_button("Cancel", tokens).on_press(Message::AnnotationEditCancelled),
            toolbar_button("Save", tokens).on_press(Message::AnnotationEditSubmitted),
        ]
        .spacing(Spacing::XS);

        let card = card_shell(
            column![
                text(truncate_preview(&annotation.quote, 72))
                    .size(FontSize::SM)
                    .color(tokens.text_secondary),
                body_input,
                actions,
            ]
            .spacing(Spacing::XS)
            .width(Length::Fill),
            active,
            tokens,
            None,
        );
        return with_corner_badge(card, badge);
    }

    let author_color = if active {
        tokens.accent
    } else {
        tokens.text_primary
    };
    let body_color = if active {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };

    let meta = row![
        text("You")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(author_color),
        Space::new().width(Length::Fill),
        text(format_relative_time(annotation.created_at))
            .size(10)
            .color(with_alpha(tokens.text_secondary, 0.75)),
    ]
    .spacing(Spacing::XS)
    .align_y(Alignment::Center);

    let mut body = column![
        meta,
        text(truncate_preview(&annotation.body, 280))
            .size(FontSize::MD)
            .color(body_color),
    ]
    .spacing(Spacing::SM)
    .width(Length::Fill);

    if active {
        let actions = row![
            button(text("Edit").size(FontSize::SM).color(tokens.text_secondary))
                .padding([Spacing::XS, Spacing::SM])
                .style(move |_, status| button_style(tokens, Class::ViewerSidebarTab, status))
                .on_press(Message::AnnotationEditStarted(annotation.id.clone())),
            button(
                text("Delete")
                    .size(FontSize::SM)
                    .color(tokens.text_secondary),
            )
            .padding([Spacing::XS, Spacing::SM])
            .style(move |_, status| button_style(tokens, Class::ViewerSidebarTab, status))
            .on_press(Message::AnnotationDeleteRequested(annotation.id.clone())),
        ]
        .spacing(Spacing::XS);
        body = body.push(actions);
    }

    let card = card_shell(
        body,
        active,
        tokens,
        Some(Message::AnnotationSelected(annotation.id.clone())),
    );
    with_corner_badge(card, badge)
}

/// Places the ordinal badge centered on the card’s top-left corner.
///
/// Layout (mockup `::before { left:-10px; top:-10px }` for a 22px circle):
/// ```text
///   frame (CARD_WIDTH + overhang)
///   ┌────── badge (centered on card corner)
///   │  ┌──────────────────────── card
///   │  │  author ··· time
///   │  │  body
/// ```
///
/// Uses stack overlay + padding instead of a small `pin`, so the badge cannot
/// be clipped by pin bounds and stays aligned as card height changes.
fn with_corner_badge<'a>(
    card: Element<'a, Message>,
    badge: Element<'a, Message>,
) -> Element<'a, Message> {
    let frame_w = CARD_WIDTH + BADGE_OVERHANG;
    // Card is inset so its top-left lands at (overhang, overhang) — the badge
    // center, since the badge is BADGE_SIZE and starts at the frame origin.
    let inset_card = container(card)
        .padding(Padding {
            top: BADGE_OVERHANG,
            right: 0.0,
            bottom: 0.0,
            left: BADGE_OVERHANG,
        })
        .width(Length::Fixed(frame_w))
        .style(move |_| transparent_container());

    stack![inset_card, badge]
        .width(Length::Fixed(frame_w))
        .clip(false)
        .into()
}

fn view_ordinal_badge(ordinal: usize, active: bool, tokens: ThemeTokens) -> Element<'static, Message> {
    let bg = if active {
        tokens.accent
    } else {
        tokens.surface_raised
    };
    let fg = if active {
        if luminance(tokens.accent) > 0.55 {
            Color::from_rgb(0.12, 0.12, 0.12)
        } else {
            Color::from_rgb(0.98, 0.98, 0.96)
        }
    } else {
        tokens.text_secondary
    };
    let border = if active {
        tokens.accent
    } else {
        tokens.border
    };
    let radius = BADGE_SIZE * 0.5;

    container(
        text(format!("{ordinal}"))
            .size(10)
            .font(ui_font(FontWeight::MEDIUM))
            .color(fg),
    )
    .width(BADGE_SIZE)
    .height(BADGE_SIZE)
    .center_x(BADGE_SIZE)
    .center_y(BADGE_SIZE)
    .style(move |_| iced::widget::container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            color: border,
            width: 1.0,
            radius: radius.into(),
        },
        text_color: Some(fg),
        shadow: Shadow {
            color: Color {
                a: 0.22,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        snap: false,
    })
    .into()
}

fn transparent_container() -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: None,
        border: Border::default(),
        text_color: None,
        shadow: Default::default(),
        snap: false,
    }
}

fn card_shell<'a>(
    content: impl Into<Element<'a, Message>>,
    active: bool,
    tokens: ThemeTokens,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let surface = if active {
        // Soft accent wash (mockup `--accent-soft` on light; mixed on espresso).
        mix_color(tokens.surface_raised, tokens.accent, 0.12)
    } else {
        tokens.surface_raised
    };
    let border_color = if active {
        tokens.accent
    } else {
        tokens.border
    };
    let shadow = Shadow {
        color: Color {
            a: if active { 0.28 } else { 0.18 },
            ..Color::BLACK
        },
        offset: Vector::new(0.0, 8.0),
        blur_radius: if active { 22.0 } else { 18.0 },
    };

    let styled = container(content.into())
        .padding(Padding {
            top: 14.0,
            right: 15.0,
            bottom: 15.0,
            left: 15.0,
        })
        .width(Length::Fixed(CARD_WIDTH))
        .style(move |_| iced::widget::container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                color: border_color,
                width: if active { 1.5 } else { 1.0 },
                radius: CARD_RADIUS.into(),
            },
            text_color: Some(tokens.text_primary),
            shadow,
            snap: false,
        });

    if let Some(message) = on_press {
        button(styled)
            .padding(0)
            .style(move |_, _| iced::widget::button::Style {
                background: None,
                text_color: tokens.text_primary,
                border: Border::default(),
                shadow: Default::default(),
                snap: false,
            })
            .on_press(message)
            .into()
    } else {
        styled.into()
    }
}

fn view_compose_card(
    compose: &crate::viewer::document::AnnotationComposeState,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let quote = truncate_preview(&compose.quote, 120);
    let body_input = text_input("Add an annotation…", &compose.body)
        .on_input(Message::AnnotationComposeBodyChanged)
        .on_submit(Message::AnnotationCreateSubmitted)
        .style(move |_, status| text_input_style(tokens, Class::SearchInput, status));

    let actions = row![
        toolbar_button("Cancel", tokens).on_press(Message::AnnotationComposeCancelled),
        toolbar_button("Save", tokens).on_press(Message::AnnotationCreateSubmitted),
    ]
    .spacing(Spacing::XS);

    card_shell(
        column![
            text("New annotation")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary),
            text(quote)
                .size(FontSize::SM)
                .color(tokens.text_secondary),
            body_input,
            actions,
        ]
        .spacing(Spacing::XS)
        .width(Length::Fill),
        true,
        tokens,
        None,
    )
}

fn hint_card(message: &str, tokens: ThemeTokens) -> Element<'_, Message> {
    card_shell(
        text(message)
            .size(FontSize::SM)
            .color(tokens.text_secondary),
        false,
        tokens,
        None,
    )
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0.0, 1.0),
        ..color
    }
}

fn mix_color(base: Color, tint: Color, amount: f32) -> Color {
    let t = amount.clamp(0.0, 1.0);
    Color {
        r: base.r + (tint.r - base.r) * t,
        g: base.g + (tint.g - base.g) * t,
        b: base.b + (tint.b - base.b) * t,
        a: base.a + (tint.a - base.a) * t,
    }
}

fn luminance(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

fn truncate_preview(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_owned();
    }
    let mut out = trimmed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

/// Relative timestamp labels (“6 days ago”, “yesterday”, …).
pub(crate) fn format_relative_time(when: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(when);
    let secs = delta.num_seconds().max(0);
    if secs < 60 {
        return "just now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        return if mins == 1 {
            "1 minute ago".into()
        } else {
            format!("{mins} minutes ago")
        };
    }
    let hours = mins / 60;
    if hours < 24 {
        return if hours == 1 {
            "1 hour ago".into()
        } else {
            format!("{hours} hours ago")
        };
    }
    let days = hours / 24;
    if days == 1 {
        "yesterday".into()
    } else if days < 14 {
        format!("{days} days ago")
    } else if days < 45 {
        let weeks = days / 7;
        if weeks == 1 {
            "1 week ago".into()
        } else {
            format!("{weeks} weeks ago")
        }
    } else {
        when.format("%b %e").to_string().replace("  ", " ")
    }
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
