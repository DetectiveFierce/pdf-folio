//! Semantic design tokens for the UI crate.

use iced::{font, Color, Font};

use super::classes::{Class, ComponentState};

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
pub struct VisualStyle {
    /// Background color override.
    pub background: Option<Color>,
    /// Text color override.
    pub text_color: Option<Color>,
    /// Border color override.
    pub border_color: Option<Color>,
    /// Border width override.
    pub border_width: Option<f32>,
    /// Radius override.
    pub radius: Option<f32>,
}

impl VisualStyle {
    /// Empty style override.
    pub const EMPTY: Self = Self {
        background: None,
        text_color: None,
        border_color: None,
        border_width: None,
        radius: None,
    };

    /// Merges another style over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            background: match overlay.background {
                Some(value) => Some(value),
                None => self.background,
            },
            text_color: match overlay.text_color {
                Some(value) => Some(value),
                None => self.text_color,
            },
            border_color: match overlay.border_color {
                Some(value) => Some(value),
                None => self.border_color,
            },
            border_width: match overlay.border_width {
                Some(value) => Some(value),
                None => self.border_width,
            },
            radius: match overlay.radius {
                Some(value) => Some(value),
                None => self.radius,
            },
        }
    }
}

/// Per-state style overrides for one semantic class.
#[derive(Debug, Clone, Copy)]
pub struct ClassStyle {
    /// State overrides ordered by `ComponentState::index`.
    pub states: [VisualStyle; ComponentState::COUNT],
}

impl ClassStyle {
    /// Empty class style.
    pub const EMPTY: Self = Self {
        states: [VisualStyle::EMPTY; ComponentState::COUNT],
    };

    /// Returns the resolved style for a component state.
    pub fn resolve(self, state: ComponentState) -> VisualStyle {
        self.states[ComponentState::Normal.index()].merged(self.states[state.index()])
    }
}

/// Runtime style values that are not represented by iced's widget styles.
#[derive(Debug, Clone, Copy)]
pub struct PrimitiveTokens {
    /// Viewer page shadow x offset.
    pub page_shadow_offset_x: f32,
    /// Viewer page shadow y offset.
    pub page_shadow_offset_y: f32,
    /// Progress bar girth.
    pub progress_girth: f32,
}

impl Default for PrimitiveTokens {
    fn default() -> Self {
        Self {
            page_shadow_offset_x: 2.0,
            page_shadow_offset_y: 2.0,
            progress_girth: 3.0,
        }
    }
}

/// Resolved theme color and component tokens used by PDF-Folio views.
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
    /// Per-class style overrides loaded from KDL.
    pub class_styles: [ClassStyle; Class::COUNT],
    /// Primitive drawing and sizing tokens loaded from KDL.
    pub primitives: PrimitiveTokens,
}

/// Spacing tokens in logical pixels.
pub struct Spacing;

impl Spacing {
    /// Extra-small space.
    pub const XS: f32 = 4.0;
    /// Small space.
    pub const SM: f32 = 6.0;
    /// Medium space.
    pub const MD: f32 = 9.0;
    /// Large space.
    pub const LG: f32 = 14.0;
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
    pub const SM: u32 = 11;
    /// Body text.
    pub const MD: u32 = 13;
    /// Control label text.
    pub const CONTROL: u32 = 14;
    /// Section heading text.
    pub const HEADING: u32 = 15;
}

/// Font-weight tokens for semantic text roles.
pub struct FontWeight;

impl FontWeight {
    /// Normal body text weight.
    pub const REGULAR: iced::font::Weight = iced::font::Weight::Normal;
    /// Medium weight for controls and dense labels.
    pub const MEDIUM: iced::font::Weight = iced::font::Weight::Medium;
    /// Emphasized control and heading weight.
    pub const SEMIBOLD: iced::font::Weight = iced::font::Weight::Semibold;
    /// Strong heading weight.
    pub const BOLD: iced::font::Weight = iced::font::Weight::Bold;
}

/// Primary application font family.
pub const UI_FONT_FAMILY: &str = "Inter";
/// Display font family for bookish titles and brand marks.
pub const DISPLAY_FONT_FAMILY: &str = "Noto Serif";

/// Returns the primary UI font with a semantic weight.
pub fn ui_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(UI_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

/// Returns the display font with a semantic weight.
pub fn display_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(DISPLAY_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

/// Icon-size tokens in logical pixels.
pub struct IconSize;

impl IconSize {
    /// Compact icon size.
    pub const SM: f32 = 16.0;
    /// Default toolbar icon size.
    pub const MD: f32 = 20.0;
}
