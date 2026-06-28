//! Semantic design tokens for the UI crate.

use iced::Color;

/// Horizontal text alignment tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    /// Align text to the start edge.
    Start,
    /// Center text.
    Center,
    /// Align text to the end edge.
    End,
}

impl TextAlignment {
    /// Converts to iced's horizontal text alignment.
    pub const fn horizontal(self) -> iced::alignment::Horizontal {
        match self {
            Self::Start => iced::alignment::Horizontal::Left,
            Self::Center => iced::alignment::Horizontal::Center,
            Self::End => iced::alignment::Horizontal::Right,
        }
    }
}

/// Content alignment tokens for containers and layout helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlignment {
    /// Align content to the start edge.
    Start,
    /// Center content.
    Center,
    /// Align content to the end edge.
    End,
}

impl ContentAlignment {
    /// Converts to iced's horizontal content alignment.
    pub const fn horizontal(self) -> iced::alignment::Horizontal {
        match self {
            Self::Start => iced::alignment::Horizontal::Left,
            Self::Center => iced::alignment::Horizontal::Center,
            Self::End => iced::alignment::Horizontal::Right,
        }
    }

    /// Converts to iced's vertical content alignment.
    pub const fn vertical(self) -> iced::alignment::Vertical {
        match self {
            Self::Start => iced::alignment::Vertical::Top,
            Self::Center => iced::alignment::Vertical::Center,
            Self::End => iced::alignment::Vertical::Bottom,
        }
    }
}

/// Theme color tokens used by PDF-Folio views.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    /// Window background.
    pub background: Color,
    /// Toolbar and sidebar surface.
    pub surface: Color,
    /// Elevated surface color.
    pub surface_raised: Color,
    /// Primary text color.
    pub text_primary: Color,
    /// Secondary text color.
    pub text_secondary: Color,
    /// Accent color for active controls.
    pub accent: Color,
    /// Border color.
    pub border: Color,
    /// Error color.
    pub error: Color,
    /// Viewer canvas background.
    pub canvas: Color,
    /// Placeholder page fill.
    pub placeholder: Color,
    /// Focus outline color.
    pub focus: Color,
    /// Subtle shadow color.
    pub shadow: Color,
}

/// Spacing tokens in logical pixels.
pub struct Spacing;

impl Spacing {
    /// Extra-small space.
    pub const XS: f32 = 4.0;
    /// Small space.
    pub const SM: f32 = 6.0;
    /// Medium space.
    pub const MD: f32 = 10.0;
    /// Large space.
    pub const LG: f32 = 12.0;
    /// Extra-large space.
    pub const XL: f32 = 24.0;
    /// Viewer page gutter.
    pub const PAGE_GUTTER: f32 = 32.0;
    /// Vertical space between rendered pages.
    pub const PAGE_GAP: f32 = 24.0;
}

/// Border-radius tokens in logical pixels.
pub struct Radius;

impl Radius {
    /// Sharp edge.
    pub const NONE: f32 = 0.0;
    /// Small radius for compact controls.
    pub const SM: f32 = 6.0;
    /// Medium radius for repeated cards.
    pub const MD: f32 = 10.0;
}

/// Border-width tokens in logical pixels.
pub struct BorderWidth;

impl BorderWidth {
    /// No visible border.
    pub const NONE: f32 = 0.0;
    /// Hairline border used for normal controls and surfaces.
    pub const HAIRLINE: f32 = 1.0;
}

/// Font-size tokens in logical pixels.
pub struct FontSize;

impl FontSize {
    /// Small metadata text.
    pub const SM: u32 = 12;
    /// Body text.
    pub const MD: u32 = 14;
    /// Control label text.
    pub const CONTROL: u32 = 15;
    /// Section heading text.
    pub const HEADING: u32 = 16;
}

/// Font-weight tokens for semantic text roles.
pub struct FontWeight;

impl FontWeight {
    /// Normal body text weight.
    pub const REGULAR: iced::font::Weight = iced::font::Weight::Normal;
    /// Emphasized control and heading weight.
    pub const SEMIBOLD: iced::font::Weight = iced::font::Weight::Semibold;
}

/// Icon-size tokens in logical pixels.
pub struct IconSize;

impl IconSize {
    /// Compact icon size.
    pub const SM: f32 = 16.0;
    /// Default toolbar icon size.
    pub const MD: f32 = 20.0;
}
