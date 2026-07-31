//! Viewer canvas drawing primitives derived from theme tokens.
//!
//! Toolbar/find-bar chrome uses the shared `*_style` helpers with viewer
//! [`Class`](super::Class) variants. This module focuses on **non-widget** paint
//! used by the page canvas: background, placeholders, page shadow, and find /
//! text-selection fills.

use iced::Color;

use crate::tokens::ThemeTokens;

use super::mix_color;

/// Page drop-shadow offsets/color drawn under rendered PDF pages on the canvas.
#[derive(Debug, Clone, Copy)]
pub struct Shadow {
    /// Horizontal offset in logical pixels (from `primitives.page_shadow_offset_x`).
    pub offset_x: f32,
    /// Vertical offset in logical pixels (from `primitives.page_shadow_offset_y`).
    pub offset_y: f32,
    /// Shadow tint (theme `shadow` token).
    pub color: Color,
}

/// Non-widget paint bundle for the PDF page canvas (not iced stylesheets).
///
/// Built by [`viewer_primitives`] from palette + `PrimitiveTokens` so the
/// canvas renderer stays theme-aware without depending on widget classes.
#[derive(Debug, Clone, Copy)]
pub struct ViewerPrimitiveStyle {
    /// Area behind pages (`tokens.canvas`).
    pub canvas: Color,
    /// Fill for not-yet-rendered page placeholders (`tokens.placeholder`).
    pub placeholder: Color,
    /// Drop shadow under each page raster.
    pub page_shadow: Shadow,
    /// Unselected find-in-document highlight (`primitives.viewer_find_fill`).
    pub find_fill: Color,
    /// Active find match highlight (`primitives.viewer_find_selected_fill`).
    pub find_selected_fill: Color,
    /// Soft text-annotation highlight (`primitives.viewer_annotation_fill`).
    pub annotation_fill: Color,
    /// Active text-annotation highlight (`primitives.viewer_annotation_selected_fill`).
    pub annotation_selected_fill: Color,
    /// Text-selection overlay (accent mixed with theme alpha/mix primitives).
    pub text_selection_fill: Color,
}

/// Collects canvas colors/shadows for the active theme snapshot.
pub fn viewer_primitives(tokens: ThemeTokens) -> ViewerPrimitiveStyle {
    ViewerPrimitiveStyle {
        canvas: tokens.canvas,
        placeholder: tokens.placeholder,
        page_shadow: Shadow {
            offset_x: tokens.primitives.page_shadow_offset_x,
            offset_y: tokens.primitives.page_shadow_offset_y,
            color: tokens.shadow,
        },
        find_fill: tokens.primitives.viewer_find_fill,
        find_selected_fill: tokens.primitives.viewer_find_selected_fill,
        annotation_fill: tokens.primitives.viewer_annotation_fill,
        annotation_selected_fill: tokens.primitives.viewer_annotation_selected_fill,
        text_selection_fill: Color {
            a: tokens.primitives.viewer_text_selection_alpha,
            ..mix_color(
                tokens.canvas,
                tokens.accent,
                tokens.primitives.viewer_text_selection_mix,
            )
        },
    }
}
