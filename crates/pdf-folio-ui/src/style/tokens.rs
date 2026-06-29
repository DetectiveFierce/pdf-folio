//! Semantic design tokens for the UI crate.

use iced::{font, Color, Font};

use super::classes::{Class, ComponentState};

/// KDL-backed layout, sizing, and spacing values used by the app shell.
#[derive(Debug, Clone)]
pub struct AppLayoutTokens {
    /// Default application window size.
    pub window_width: f32,
    /// Default application window height.
    pub window_height: f32,
    /// Sidebar width for the viewer table of contents.
    pub viewer_sidebar_width: f32,
    /// Initial width for library tag filters.
    pub library_sidebar_width: f32,
    /// Minimum width for the resizable library tag sidebar.
    pub library_sidebar_min_width: f32,
    /// Maximum width for the resizable library tag sidebar.
    pub library_sidebar_max_width: f32,
    /// Width of the draggable sidebar resize handle.
    pub sidebar_resize_handle_width: f32,
    /// Visible width of the sidebar resize handle when idle.
    pub sidebar_resize_handle_visual_width: f32,
    /// Toolbar height used as a sizing token for future settings persistence.
    pub toolbar_height: f32,
    /// Overscan rows rendered above and below the visible library window.
    pub library_overscan_rows: usize,
    /// Minimum number of columns in the masonry library view.
    pub card_grid_columns: usize,
    /// Fixed visual width for PDF cards in masonry mode.
    pub library_grid_card_width: f32,
    /// Library card row height in grid mode.
    pub library_grid_row_height: f32,
    /// Folder card row height in grid mode.
    pub library_folder_grid_row_height: f32,
    /// Library row height in list mode.
    pub library_list_row_height: f32,
    /// Folder row height in list mode.
    pub library_folder_list_row_height: f32,
    /// Default thumbnail width in grid cards.
    pub library_card_thumbnail_width: f32,
    /// Default thumbnail width in list rows.
    pub library_row_thumbnail_width: f32,
    /// Width of the progress area in compact library rows.
    pub library_row_progress_width: f32,
    /// Logical pixels per wheel line.
    pub line_scroll_pixels: f32,
    /// Default jump overlay input width.
    pub jump_input_width: f32,
    /// Inner text/content width of a grid card.
    pub library_card_content_width: f32,
    /// Width used for truncating grid card titles.
    pub library_card_title_width: f32,
    /// Fixed info panel height inside grid cards.
    pub library_card_info_height: f32,
    /// Maximum media area height inside grid cards.
    pub library_card_media_max_height: f32,
    /// Horizontal and vertical masonry gap.
    pub library_masonry_gap: f32,
    /// Reserved gutter for the library scrollbar.
    pub library_scrollbar_gutter: f32,
    /// Width used for truncating list row titles.
    pub library_row_title_width: f32,
    /// Floating drag preview offset in grid mode.
    pub library_drag_preview_grid_x_offset: f32,
    /// Floating drag preview offset in grid mode.
    pub library_drag_preview_grid_y_offset: f32,
    /// Floating drag preview offset in list mode.
    pub library_drag_preview_list_x_offset: f32,
    /// Floating drag preview offset in list mode.
    pub library_drag_preview_list_y_offset: f32,
    /// Alpha used for the drag placeholder content.
    pub library_drag_placeholder_content_alpha: f32,
    /// Bulk tag input preferred width.
    pub bulk_tag_input_width: f32,
    /// Bulk tag input minimum width.
    pub bulk_tag_input_min_width: f32,
    /// Single-selection title input preferred width.
    pub selection_title_input_width: f32,
    /// Single-selection author input preferred width.
    pub selection_author_input_width: f32,
    /// Single-selection title input minimum width.
    pub selection_title_input_min_width: f32,
    /// Single-selection author input minimum width.
    pub selection_author_input_min_width: f32,
    /// Top app menu bar height.
    pub app_menu_bar_height: f32,
    /// Selection context row height.
    pub selection_context_row_height: f32,
    /// Dropdown panel width.
    pub app_menu_panel_width: f32,
    /// Dropdown menu item height.
    pub app_menu_item_height: f32,
    /// Library sidebar tab button height.
    pub sidebar_tab_height: f32,
}

impl AppLayoutTokens {
    /// Returns the default app window size as expected by iced.
    pub fn window_size(&self) -> [f32; 2] {
        [self.window_width, self.window_height]
    }
}

impl Default for AppLayoutTokens {
    fn default() -> Self {
        let library_grid_card_width = 210.0;
        Self {
            window_width: 960.0,
            window_height: 1080.0,
            viewer_sidebar_width: 228.0,
            library_sidebar_width: 270.0,
            library_sidebar_min_width: 210.0,
            library_sidebar_max_width: 340.0,
            sidebar_resize_handle_width: 8.0,
            sidebar_resize_handle_visual_width: 2.0,
            toolbar_height: 58.0,
            library_overscan_rows: 4,
            card_grid_columns: 2,
            library_grid_card_width,
            library_grid_row_height: 376.0,
            library_folder_grid_row_height: 86.0,
            library_list_row_height: 78.0,
            library_folder_list_row_height: 50.0,
            library_card_thumbnail_width: 128.0,
            library_row_thumbnail_width: 46.0,
            library_row_progress_width: 120.0,
            line_scroll_pixels: 48.0,
            jump_input_width: 90.0,
            library_card_content_width: library_grid_card_width - 14.0 * 2.0,
            library_card_title_width: library_grid_card_width - 14.0 * 2.0,
            library_card_info_height: 120.0,
            library_card_media_max_height: library_grid_card_width * 1.32,
            library_masonry_gap: 18.0,
            library_scrollbar_gutter: 28.0,
            library_row_title_width: 520.0,
            library_drag_preview_grid_x_offset: 32.0,
            library_drag_preview_grid_y_offset: 28.0,
            library_drag_preview_list_x_offset: 28.0,
            library_drag_preview_list_y_offset: 24.0,
            library_drag_placeholder_content_alpha: 0.42,
            bulk_tag_input_width: 150.0,
            bulk_tag_input_min_width: 90.0,
            selection_title_input_width: 260.0,
            selection_author_input_width: 190.0,
            selection_title_input_min_width: 120.0,
            selection_author_input_min_width: 96.0,
            app_menu_bar_height: 32.0,
            selection_context_row_height: 46.0,
            app_menu_panel_width: 270.0,
            app_menu_item_height: 30.0,
            sidebar_tab_height: 30.0,
        }
    }
}

/// KDL-backed user-facing labels for app chrome and command surfaces.
#[derive(Debug, Clone)]
pub struct AppLabelTokens {
    /// App menu names.
    pub app_menu: std::collections::HashMap<String, String>,
    /// App menu command labels.
    pub app_menu_action: std::collections::HashMap<String, String>,
    /// Selection toolbar labels.
    pub selection_toolbar_action: std::collections::HashMap<String, String>,
    /// Library sidebar tab labels.
    pub library_sidebar_tab: std::collections::HashMap<String, String>,
    /// Other short labels and status copy.
    pub text: std::collections::HashMap<String, String>,
}

impl AppLabelTokens {
    /// Returns a configured label or the supplied fallback.
    pub fn get<'a>(&'a self, section: LabelSection, key: &str, fallback: &'a str) -> &'a str {
        let source = match section {
            LabelSection::AppMenu => &self.app_menu,
            LabelSection::AppMenuAction => &self.app_menu_action,
            LabelSection::SelectionToolbarAction => &self.selection_toolbar_action,
            LabelSection::LibrarySidebarTab => &self.library_sidebar_tab,
            LabelSection::Text => &self.text,
        };
        source.get(key).map_or(fallback, String::as_str)
    }
}

impl Default for AppLabelTokens {
    fn default() -> Self {
        Self {
            app_menu: std::collections::HashMap::new(),
            app_menu_action: std::collections::HashMap::new(),
            selection_toolbar_action: std::collections::HashMap::new(),
            library_sidebar_tab: std::collections::HashMap::new(),
            text: std::collections::HashMap::new(),
        }
    }
}

/// Label namespaces accepted by `AppLabelTokens`.
#[derive(Debug, Clone, Copy)]
pub enum LabelSection {
    /// App menu names.
    AppMenu,
    /// App menu command labels.
    AppMenuAction,
    /// Selection toolbar command labels.
    SelectionToolbarAction,
    /// Library sidebar tab labels.
    LibrarySidebarTab,
    /// Miscellaneous text.
    Text,
}

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
    /// Per-side border overrides.
    pub border: Option<VisualBorder>,
    /// Radius override.
    pub radius: Option<CornerRadius>,
}

impl VisualStyle {
    /// Empty style override.
    pub const EMPTY: Self = Self {
        background: None,
        text_color: None,
        border_color: None,
        border_width: None,
        border: None,
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
            border: match (self.border, overlay.border) {
                (Some(base), Some(overlay)) => Some(base.merged(overlay)),
                (None, Some(overlay)) => Some(overlay),
                (Some(base), None) => Some(base),
                (None, None) => None,
            },
            radius: match overlay.radius {
                Some(value) => Some(value),
                None => self.radius,
            },
        }
    }
}

/// Border styling for one side of a component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSide {
    /// Side width in logical pixels.
    pub width: Option<f32>,
    /// Side color.
    pub color: Option<Color>,
}

impl BorderSide {
    /// Empty side override.
    pub const EMPTY: Self = Self {
        width: None,
        color: None,
    };

    /// Creates a side with both width and color set.
    pub const fn new(width: f32, color: Color) -> Self {
        Self {
            width: Some(width),
            color: Some(color),
        }
    }

    /// Merges another side override over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            width: match overlay.width {
                Some(value) => Some(value),
                None => self.width,
            },
            color: match overlay.color {
                Some(value) => Some(value),
                None => self.color,
            },
        }
    }
}

/// Border styling for each side of a component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualBorder {
    /// Top side.
    pub top: BorderSide,
    /// Right side.
    pub right: BorderSide,
    /// Bottom side.
    pub bottom: BorderSide,
    /// Left side.
    pub left: BorderSide,
}

impl VisualBorder {
    /// Empty border override.
    pub const EMPTY: Self = Self {
        top: BorderSide::EMPTY,
        right: BorderSide::EMPTY,
        bottom: BorderSide::EMPTY,
        left: BorderSide::EMPTY,
    };

    /// Creates a border with the same style on each side.
    pub const fn uniform(width: f32, color: Color) -> Self {
        let side = BorderSide::new(width, color);
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    /// Creates a partial border from legacy uniform fields.
    pub const fn from_legacy(width: Option<f32>, color: Option<Color>) -> Self {
        let side = BorderSide { width, color };
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    /// Merges another border override over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            top: self.top.merged(overlay.top),
            right: self.right.merged(overlay.right),
            bottom: self.bottom.merged(overlay.bottom),
            left: self.left.merged(overlay.left),
        }
    }

    /// Returns the border as a native iced border when all sides match.
    pub fn uniform_style(self) -> Option<(f32, Color)> {
        let width = self.top.width?;
        let color = self.top.color?;
        let side = BorderSide::new(width, color);
        if self.right == side && self.bottom == side && self.left == side {
            Some((width, color))
        } else {
            None
        }
    }
}

/// Border radius values for each corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadius {
    /// Top-left corner radius.
    pub top_left: f32,
    /// Top-right corner radius.
    pub top_right: f32,
    /// Bottom-right corner radius.
    pub bottom_right: f32,
    /// Bottom-left corner radius.
    pub bottom_left: f32,
}

impl CornerRadius {
    /// Creates a radius with the same value for each corner.
    pub const fn uniform(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

impl From<CornerRadius> for iced::border::Radius {
    fn from(radius: CornerRadius) -> Self {
        Self {
            top_left: radius.top_left,
            top_right: radius.top_right,
            bottom_right: radius.bottom_right,
            bottom_left: radius.bottom_left,
        }
    }
}

/// Per-state style overrides for one semantic class.
#[derive(Debug, Clone, Copy)]
pub struct ClassStyle {
    /// State overrides ordered by `ComponentState::index`.
    pub states: [VisualStyle; ComponentState::COUNT],
    /// Layout overrides for this component.
    pub layout: ComponentLayout,
    /// Text styling overrides for this component.
    pub text: ComponentTextStyle,
}

impl ClassStyle {
    /// Empty class style.
    pub const EMPTY: Self = Self {
        states: [VisualStyle::EMPTY; ComponentState::COUNT],
        layout: ComponentLayout::EMPTY,
        text: ComponentTextStyle::EMPTY,
    };

    /// Returns the resolved style for a component state.
    pub fn resolve(self, state: ComponentState) -> VisualStyle {
        self.states[ComponentState::Normal.index()].merged(self.states[state.index()])
    }
}

/// Layout properties that can be attached to a styled component in KDL.
#[derive(Debug, Clone, Copy)]
pub struct ComponentLayout {
    /// Fixed width in logical pixels.
    pub width: Option<f32>,
    /// Fill-portion width for row/column layouts.
    pub width_portion: Option<u16>,
    /// Fixed height in logical pixels.
    pub height: Option<f32>,
    /// Component padding.
    pub padding: BoxSpacing,
    /// External component margin/gutter.
    pub margin: BoxSpacing,
    /// Child spacing.
    pub spacing: Option<f32>,
}

impl ComponentLayout {
    /// Empty component layout.
    pub const EMPTY: Self = Self {
        width: None,
        width_portion: None,
        height: None,
        padding: BoxSpacing::EMPTY,
        margin: BoxSpacing::EMPTY,
        spacing: None,
    };

    /// Merges another layout over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            width: match overlay.width {
                Some(value) => Some(value),
                None => self.width,
            },
            width_portion: match overlay.width_portion {
                Some(value) => Some(value),
                None => self.width_portion,
            },
            height: match overlay.height {
                Some(value) => Some(value),
                None => self.height,
            },
            padding: self.padding.merged(overlay.padding),
            margin: self.margin.merged(overlay.margin),
            spacing: match overlay.spacing {
                Some(value) => Some(value),
                None => self.spacing,
            },
        }
    }

    /// Returns horizontal padding, falling back to uniform padding.
    pub fn padding_x(self, fallback: f32) -> f32 {
        self.padding.horizontal(fallback)
    }

    /// Returns vertical padding, falling back to uniform padding.
    pub fn padding_y(self, fallback: f32) -> f32 {
        self.padding.vertical(fallback)
    }

    /// Returns left padding.
    pub fn padding_left(self, fallback: f32) -> f32 {
        self.padding
            .left
            .unwrap_or_else(|| self.padding_x(fallback))
    }

    /// Returns right padding.
    pub fn padding_right(self, fallback: f32) -> f32 {
        self.padding
            .right
            .unwrap_or_else(|| self.padding_x(fallback))
    }

    /// Returns top padding.
    pub fn padding_top(self, fallback: f32) -> f32 {
        self.padding.top.unwrap_or_else(|| self.padding_y(fallback))
    }

    /// Returns bottom padding.
    pub fn padding_bottom(self, fallback: f32) -> f32 {
        self.padding
            .bottom
            .unwrap_or_else(|| self.padding_y(fallback))
    }

    /// Returns horizontal margin.
    pub fn margin_x(self, fallback: f32) -> f32 {
        self.margin.horizontal(fallback)
    }

    /// Returns vertical margin.
    pub fn margin_y(self, fallback: f32) -> f32 {
        self.margin.vertical(fallback)
    }

    /// Returns left margin.
    pub fn margin_left(self, fallback: f32) -> f32 {
        self.margin.left.unwrap_or_else(|| self.margin_x(fallback))
    }

    /// Returns right margin.
    pub fn margin_right(self, fallback: f32) -> f32 {
        self.margin.right.unwrap_or_else(|| self.margin_x(fallback))
    }

    /// Returns top margin.
    pub fn margin_top(self, fallback: f32) -> f32 {
        self.margin.top.unwrap_or_else(|| self.margin_y(fallback))
    }

    /// Returns bottom margin.
    pub fn margin_bottom(self, fallback: f32) -> f32 {
        self.margin
            .bottom
            .unwrap_or_else(|| self.margin_y(fallback))
    }
}

/// CSS-like box spacing values for padding and margin.
#[derive(Debug, Clone, Copy)]
pub struct BoxSpacing {
    /// Top spacing.
    pub top: Option<f32>,
    /// Right spacing.
    pub right: Option<f32>,
    /// Bottom spacing.
    pub bottom: Option<f32>,
    /// Left spacing.
    pub left: Option<f32>,
}

impl BoxSpacing {
    /// Empty spacing.
    pub const EMPTY: Self = Self {
        top: None,
        right: None,
        bottom: None,
        left: None,
    };

    /// Uniform spacing.
    pub const fn uniform(value: f32) -> Self {
        Self {
            top: Some(value),
            right: Some(value),
            bottom: Some(value),
            left: Some(value),
        }
    }

    /// Axis spacing, ordered like iced/CSS shorthand: vertical, horizontal.
    pub const fn axes(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: Some(vertical),
            right: Some(horizontal),
            bottom: Some(vertical),
            left: Some(horizontal),
        }
    }

    /// Four-value spacing, ordered top, right, bottom, left.
    pub const fn sides(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top: Some(top),
            right: Some(right),
            bottom: Some(bottom),
            left: Some(left),
        }
    }

    /// Merges another spacing value over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            top: match overlay.top {
                Some(value) => Some(value),
                None => self.top,
            },
            right: match overlay.right {
                Some(value) => Some(value),
                None => self.right,
            },
            bottom: match overlay.bottom {
                Some(value) => Some(value),
                None => self.bottom,
            },
            left: match overlay.left {
                Some(value) => Some(value),
                None => self.left,
            },
        }
    }

    fn horizontal(self, fallback: f32) -> f32 {
        self.left.or(self.right).unwrap_or(fallback)
    }

    fn vertical(self, fallback: f32) -> f32 {
        self.top.or(self.bottom).unwrap_or(fallback)
    }
}

/// Text properties that can be attached to a styled component in KDL.
#[derive(Debug, Clone, Copy)]
pub struct ComponentTextStyle {
    /// Font size in logical pixels.
    pub size: Option<u32>,
    /// Font weight.
    pub weight: Option<iced::font::Weight>,
}

impl ComponentTextStyle {
    /// Empty component text style.
    pub const EMPTY: Self = Self {
        size: None,
        weight: None,
    };

    /// Merges another text style over this one.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            size: match overlay.size {
                Some(value) => Some(value),
                None => self.size,
            },
            weight: match overlay.weight {
                Some(value) => Some(value),
                None => self.weight,
            },
        }
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
