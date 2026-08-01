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
//! Placement math lives in [`crate::viewer::annotation_layout`]; this module
//! only builds widgets. Related: composed inside
//! [`crate::viewer::view::document`]; highlight paint and click hit-testing
//! live in [`super::canvas`].

use crate::viewer::annotation_layout::{
    annotation_layer_metrics, CARD_WIDTH,
};
use crate::*;
use chrono::{DateTime, Utc};
use iced::widget::{button, column, row, stack, text_input, Space};
use iced::{Alignment, Background, Border, Color, Length, Padding, Shadow, Vector};

/// Ordinal badge diameter (mockup `::before` 22×22).
const BADGE_SIZE: f32 = 22.0;
/// How far the badge center sits past the card’s top-left corner.
///
/// Half the badge so the circle is centered on the corner (mockup ≈ −10/−10
/// with a 22px badge). The outer frame grows by this amount on top/left.
const BADGE_OVERHANG: f32 = BADGE_SIZE * 0.5;
/// Corner radius for comment cards (mockup `--radius: 12px`).
const CARD_RADIUS: f32 = 12.0;
/// Active card nudge toward the page (mockup `translateX(-4px)`).
const ACTIVE_NUDGE_X: f32 = 4.0;

/// Viewport-pinned chrome: compose form and empty-state hints only.
///
/// Anchored cards live in the scrollable content via
/// [`view_annotations_content_layer`].
pub(crate) fn view_annotations_viewport_chrome(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'_, Message>> {
    if let Some(compose) = &app.viewer.annotation_compose {
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
    // the card column, so pass base size from page rects for column placement.
    let base = app.viewer_base_content_size(app.viewer.viewer_viewport_width);
    let page_rects = app.viewer_page_rects_content(app.viewer.viewer_viewport_width);
    let metrics = annotation_layer_metrics(
        &app.viewer.annotations,
        &page_rects,
        &app.viewer.viewer_text_layers,
        app.viewer.annotation_editing_id.as_ref(),
        base,
    );
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
        let editing = app.viewer.annotation_editing_id.as_ref() == Some(&annotation.id);
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

fn view_anchored_card<'a>(
    app: &'a PDFolioApp,
    annotation: &'a pdf_folio_core::Annotation,
    index: usize,
    _total: usize,
    active: bool,
    editing: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let ordinal = index + 1;
    let badge = view_ordinal_badge(ordinal, active, tokens);

    if editing {
        let body_input = text_input("Annotation", &app.viewer.annotation_edit_body)
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
