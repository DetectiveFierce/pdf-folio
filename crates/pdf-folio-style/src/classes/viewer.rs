use iced::Color;

use crate::tokens::ThemeTokens;

use super::mix_color;

/// Canvas shadow primitive.
#[derive(Debug, Clone, Copy)]
pub struct Shadow {
    /// Horizontal shadow offset.
    pub offset_x: f32,
    /// Vertical shadow offset.
    pub offset_y: f32,
    /// Shadow color.
    pub color: Color,
}

/// Canvas drawing colors used by the viewer.
#[derive(Debug, Clone, Copy)]
pub struct ViewerPrimitiveStyle {
    /// Canvas background color.
    pub canvas: Color,
    /// Placeholder fill color.
    pub placeholder: Color,
    /// Page shadow.
    pub page_shadow: Shadow,
    /// Unselected find result fill.
    pub find_fill: Color,
    /// Selected find result fill.
    pub find_selected_fill: Color,
    /// Text selection fill.
    pub text_selection_fill: Color,
}

/// Returns viewer canvas drawing primitives for the active theme.
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
