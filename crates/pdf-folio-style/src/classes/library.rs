//! Library-surface class styles: sidebar scrollbars and related chrome.
//!
//! Library cards/rows mostly go through [`super::core::button_style`]. This
//! module holds library-specific scrollable styling so sidebar rails can use a
//! denser thumb treatment than document scroll areas.

use iced::widget::{container, scrollable};
use iced::{Background, Border, Color, Shadow as IcedShadow};

use crate::tokens::{BorderWidth, ThemeTokens};

use super::mix_color;

/// iced scrollable stylesheet tuned for library/viewer sidebars (denser thumbs).
pub fn sidebar_scrollable_style(
    tokens: ThemeTokens,
    status: scrollable::Status,
) -> scrollable::Style {
    let thumb = match status {
        scrollable::Status::Active { .. } => mix_color(tokens.border, tokens.text_secondary, 0.35),
        scrollable::Status::Hovered { .. } => mix_color(tokens.border, tokens.focus, 0.52),
        scrollable::Status::Dragged { .. } => tokens.accent,
    };
    let rail = scrollable::Rail {
        background: None,
        border: Border {
            width: BorderWidth::NONE,
            color: Color::TRANSPARENT,
            radius: 0.0.into(),
        },
        scroller: scrollable::Scroller {
            background: Background::Color(thumb),
            border: Border {
                width: BorderWidth::NONE,
                color: thumb,
                radius: tokens.primitives.scrollbar_radius.into(),
            },
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(Color::TRANSPARENT),
            border: Border {
                width: BorderWidth::NONE,
                color: Color::TRANSPARENT,
                radius: 0.0.into(),
            },
            shadow: IcedShadow::default(),
            icon: Color::TRANSPARENT,
        },
    }
}
