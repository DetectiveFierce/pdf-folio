//! Semantic design tokens shared by the style book and the UI crate.
//!
//! This module is the typed vocabulary of the design system. Values either
//! come from KDL (via [`crate::StyleBook`]) or from the constant tables below
//! (`Spacing`, `FontSize`, `Radius`, …) so views avoid magic numbers.
//!
//! # Groups
//!
//! | Type | Purpose |
//! | --- | --- |
//! | [`ThemeTokens`] | Full resolved palette + per-class styles + primitives |
//! | [`AppLayoutTokens`] | Window, sidebar, card, toolbar, viewer metrics |
//! | [`AppLabelTokens`] | Chrome strings loaded from KDL label sections |
//! | [`ClassStyle`] / [`VisualStyle`] | Per-state paint for a [`crate::Class`] |
//! | [`ComponentLayout`] / [`ComponentTextStyle`] | Size/padding/text from KDL |
//! | [`VisualBorder`] / [`BorderSide`] / [`BoxShadow`] | Side-aware chrome |
//! | [`Spacing`], [`FontSize`], [`Radius`], [`BorderWidth`] | Constant scales |
//!
//! Prefer [`ui_font`] and [`display_font`] when constructing iced text so the
//! bundled IBM Plex Sans / Vollkorn families stay consistent.

use iced::{font, Color, Font};
use std::collections::HashMap;

use crate::classes::{Class, ComponentState};

/// KDL-backed layout metrics for the app shell (window, sidebars, grids, menus).
///
/// Populated from `application.kdl`, component `layout { … }` blocks, and top-level
/// `layout { metric …; count … }` nodes. Extra keys land in [`Self::metrics`] /
/// [`Self::counts`] via `Component.property` names.
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
    /// Width of the File app menu button.
    pub app_menu_file_width: f32,
    /// Width of the Edit app menu button.
    pub app_menu_edit_width: f32,
    /// Width of the View app menu button.
    pub app_menu_view_width: f32,
    /// Width of the Document app menu button.
    pub app_menu_document_width: f32,
    /// Width of the Library app menu button.
    pub app_menu_library_width: f32,
    /// Width of the Tools app menu button.
    pub app_menu_tools_width: f32,
    /// Width of the Help app menu button.
    pub app_menu_help_width: f32,
    /// Selection context row height.
    pub selection_context_row_height: f32,
    /// Dropdown panel width.
    pub app_menu_panel_width: f32,
    /// Dropdown menu item height.
    pub app_menu_item_height: f32,
    /// Context menu panel width.
    pub context_menu_panel_width: f32,
    /// Context menu item height.
    pub context_menu_item_height: f32,
    /// Library sidebar tab button height.
    pub sidebar_tab_height: f32,
    /// Viewer toolbar title minimum width.
    pub viewer_toolbar_title_min_width: f32,
    /// Viewer toolbar title maximum width.
    pub viewer_toolbar_title_max_width: f32,
    /// Viewer toolbar selection/status width.
    pub viewer_toolbar_selection_width: f32,
    /// Viewer find popup width.
    pub viewer_find_bar_width: f32,
    /// Viewer find popup height.
    pub viewer_find_bar_height: f32,
    /// Viewer page number input width.
    pub viewer_page_number_width: f32,
    /// Viewer page control width.
    pub viewer_page_control_width: f32,
    /// Viewer page chevron button size.
    pub viewer_page_chevron_size: f32,
    /// Viewer thumbnail width in pixels.
    pub viewer_thumbnail_width_px: u16,
    /// Viewer page fade duration in milliseconds.
    pub viewer_page_fade_ms: u64,
    /// Viewer zoom control width.
    pub viewer_zoom_control_width: f32,
    /// Viewer zoom menu width.
    pub viewer_zoom_menu_width: f32,
    /// Viewer zoom menu row height.
    pub viewer_zoom_menu_row_height: f32,
    /// Additional KDL-backed component metrics keyed as `Component.property`.
    pub metrics: HashMap<String, f32>,
    /// Additional KDL-backed component counts keyed as `Component.property`.
    pub counts: HashMap<String, usize>,
}

impl AppLayoutTokens {
    /// Default window size as `[width, height]` for iced's application settings.
    ///
    /// Backed by `window_width` / `window_height` from `application.kdl` (or defaults).
    pub fn window_size(&self) -> [f32; 2] {
        [self.window_width, self.window_height]
    }

    /// Looks up an open-ended KDL metric keyed as `{component}.{property}`.
    ///
    /// Populated by `layout { metric … }` / component layout extras when the
    /// field is not a fixed struct member. Returns `fallback` when the key is
    /// absent so call sites can keep a hard-coded safety value.
    pub fn metric(&self, component: &str, property: &str, fallback: f32) -> f32 {
        self.metrics
            .get(&format!("{component}.{property}"))
            .copied()
            .unwrap_or(fallback)
    }

    /// Looks up an open-ended KDL integer count keyed as `{component}.{property}`.
    ///
    /// Same map as [`Self::metric`] but for discrete values (column counts, …).
    /// Returns `fallback` when the key is missing.
    pub fn count(&self, component: &str, property: &str, fallback: usize) -> usize {
        self.counts
            .get(&format!("{component}.{property}"))
            .copied()
            .unwrap_or(fallback)
    }

    /// Records a metric during style-book parse (`Component.property` → f32).
    pub fn set_metric(&mut self, component: &str, property: &str, value: f32) {
        self.metrics
            .insert(format!("{component}.{property}"), value);
    }

    /// Records a count during style-book parse (`Component.property` → usize).
    pub fn set_count(&mut self, component: &str, property: &str, value: usize) {
        self.counts.insert(format!("{component}.{property}"), value);
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
            toolbar_height: 46.0,
            library_overscan_rows: 4,
            card_grid_columns: 2,
            library_grid_card_width,
            library_grid_row_height: 394.0,
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
            library_card_info_height: 138.0,
            library_card_media_max_height: library_grid_card_width * 1.32,
            library_masonry_gap: 18.0,
            library_scrollbar_gutter: 10.0,
            library_row_title_width: 520.0,
            library_drag_preview_grid_x_offset: 20.0,
            library_drag_preview_grid_y_offset: 16.0,
            library_drag_preview_list_x_offset: 18.0,
            library_drag_preview_list_y_offset: 14.0,
            library_drag_placeholder_content_alpha: 0.42,
            bulk_tag_input_width: 150.0,
            bulk_tag_input_min_width: 90.0,
            selection_title_input_width: 260.0,
            selection_author_input_width: 190.0,
            selection_title_input_min_width: 120.0,
            selection_author_input_min_width: 96.0,
            app_menu_bar_height: 32.0,
            app_menu_file_width: 48.0,
            app_menu_edit_width: 48.0,
            app_menu_view_width: 56.0,
            app_menu_document_width: 88.0,
            app_menu_library_width: 68.0,
            app_menu_tools_width: 58.0,
            app_menu_help_width: 56.0,
            selection_context_row_height: 46.0,
            app_menu_panel_width: 270.0,
            app_menu_item_height: 30.0,
            context_menu_panel_width: 250.0,
            context_menu_item_height: 30.0,
            sidebar_tab_height: 30.0,
            viewer_toolbar_title_min_width: 28.0,
            viewer_toolbar_title_max_width: 360.0,
            viewer_toolbar_selection_width: 116.0,
            viewer_find_bar_width: 600.0,
            viewer_find_bar_height: 42.0,
            viewer_page_number_width: 42.0,
            viewer_page_control_width: 150.0,
            viewer_page_chevron_size: 28.0,
            viewer_thumbnail_width_px: 128,
            viewer_page_fade_ms: 50,
            viewer_zoom_control_width: 98.0,
            viewer_zoom_menu_width: 118.0,
            viewer_zoom_menu_row_height: 22.0,
            metrics: HashMap::new(),
            counts: HashMap::new(),
        }
    }
}

/// KDL-backed user-facing strings for menus, selection toolbar, and chrome copy.
///
/// Loaded from `labels { … }` nodes and component `labels` blocks so product
/// copy can change without recompiling Rust (when hot-reloading styles).
#[derive(Debug, Clone, Default)]
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
    /// Resolves a KDL label under `section`/`key`, or `fallback` when unset.
    ///
    /// Call sites pass a compile-time English fallback so missing KDL never
    /// blanks chrome (menus, selection toolbar, sidebar tabs, misc status).
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

/// Label map namespace passed to [`AppLabelTokens::get`] (mirrors KDL label sections).
#[derive(Debug, Clone, Copy)]
pub enum LabelSection {
    /// Top-level menu titles (`File`, `Edit`, …).
    AppMenu,
    /// Individual app-menu command captions.
    AppMenuAction,
    /// Multi-select / selection toolbar command captions.
    SelectionToolbarAction,
    /// Library sidebar tab titles (Tags, Folders, …).
    LibrarySidebarTab,
    /// Catch-all short status and chrome strings.
    Text,
}

/// Semantic horizontal text alignment (independent of LTR/RTL iced left/right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    /// Leading edge (maps to iced Left in LTR).
    Start,
    /// Horizontal center.
    Center,
    /// Trailing edge (maps to iced Right in LTR).
    End,
}

impl TextAlignment {
    /// iced horizontal alignment used by text widgets / factories.
    pub const fn horizontal(self) -> iced::alignment::Horizontal {
        match self {
            Self::Start => iced::alignment::Horizontal::Left,
            Self::Center => iced::alignment::Horizontal::Center,
            Self::End => iced::alignment::Horizontal::Right,
        }
    }
}

/// Semantic content alignment for containers and layout helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentAlignment {
    /// Leading / top edge depending on axis.
    Start,
    /// Centered on the chosen axis.
    Center,
    /// Trailing / bottom edge depending on axis.
    End,
}

impl ContentAlignment {
    /// Horizontal iced alignment (Start→Left, End→Right in LTR).
    pub const fn horizontal(self) -> iced::alignment::Horizontal {
        match self {
            Self::Start => iced::alignment::Horizontal::Left,
            Self::Center => iced::alignment::Horizontal::Center,
            Self::End => iced::alignment::Horizontal::Right,
        }
    }

    /// Vertical iced alignment (Start→Top, End→Bottom).
    pub const fn vertical(self) -> iced::alignment::Vertical {
        match self {
            Self::Start => iced::alignment::Vertical::Top,
            Self::Center => iced::alignment::Vertical::Center,
            Self::End => iced::alignment::Vertical::Bottom,
        }
    }
}

/// Optional paint overrides for one class state (from KDL `component` blocks).
///
/// Each field is `None` until KDL or a theme sets it. Merged with
/// [`VisualStyle::merged`] so `Hovered` / `Pressed` only specify deltas over
/// `Normal`. Consumed by `classes::*` when building iced stylesheets.
#[derive(Debug, Clone, Copy)]
pub struct VisualStyle {
    /// Fill color when the widget draws a solid background.
    pub background: Option<Color>,
    /// Foreground / label color for the widget contents.
    pub text_color: Option<Color>,
    /// Legacy uniform border color (ignored when [`Self::border`] is set).
    pub border_color: Option<Color>,
    /// Legacy uniform border width in logical pixels.
    pub border_width: Option<f32>,
    /// Side-aware border override (preferred over uniform border fields).
    pub border: Option<VisualBorder>,
    /// Corner radii applied to the widget chrome.
    pub radius: Option<CornerRadius>,
    /// Drop shadow drawn behind elevated surfaces.
    pub shadow: Option<BoxShadow>,
}

impl VisualStyle {
    /// No overrides — resolve falls through to theme defaults / other states.
    pub const EMPTY: Self = Self {
        background: None,
        text_color: None,
        border_color: None,
        border_width: None,
        border: None,
        radius: None,
        shadow: None,
    };

    /// Layer-merge: each `Some` field in `overlay` replaces the base field.
    ///
    /// Nested [`VisualBorder`] values merge side-by-side rather than wholesale replace.
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
            shadow: match overlay.shadow {
                Some(value) => Some(value),
                None => self.shadow,
            },
        }
    }
}

/// Drop-shadow parameters mapped onto iced's `Shadow` (KDL `shadow { … }`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    /// Horizontal offset in logical pixels (positive = right).
    pub offset_x: f32,
    /// Vertical offset in logical pixels (positive = down).
    pub offset_y: f32,
    /// Gaussian blur radius in logical pixels.
    pub blur_radius: f32,
    /// Shadow tint (usually a translucent token color).
    pub color: Color,
}

impl From<BoxShadow> for iced::Shadow {
    fn from(shadow: BoxShadow) -> Self {
        Self {
            color: shadow.color,
            offset: iced::Vector::new(shadow.offset_x, shadow.offset_y),
            blur_radius: shadow.blur_radius,
        }
    }
}

/// Optional width/color for a single edge of a [`VisualBorder`].
///
/// Used by the side-border widget path when iced’s uniform border is insufficient
/// (e.g. only a bottom hairline under a toolbar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderSide {
    /// Edge thickness in logical pixels (`None` = inherit / no override).
    pub width: Option<f32>,
    /// Edge color (`None` = inherit / no override).
    pub color: Option<Color>,
}

impl BorderSide {
    /// No side override (transparent to merge / side-border extractors).
    pub const EMPTY: Self = Self {
        width: None,
        color: None,
    };

    /// Fully specified side for KDL or programmatic uniform borders.
    pub const fn new(width: f32, color: Color) -> Self {
        Self {
            width: Some(width),
            color: Some(color),
        }
    }

    /// Field-wise merge; `overlay` `Some` values win.
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

/// Per-side border overrides from KDL (`border { top …; … }`) or code.
///
/// When sides differ, `classes` clear iced’s uniform border and the
/// `borders::side_border` wrapper paints each edge. When all four sides match,
/// [`Self::uniform_style`] can feed iced’s native border instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualBorder {
    /// Top edge.
    pub top: BorderSide,
    /// Right edge.
    pub right: BorderSide,
    /// Bottom edge.
    pub bottom: BorderSide,
    /// Left edge.
    pub left: BorderSide,
}

impl VisualBorder {
    /// All sides empty (no side-border paint from this override).
    pub const EMPTY: Self = Self {
        top: BorderSide::EMPTY,
        right: BorderSide::EMPTY,
        bottom: BorderSide::EMPTY,
        left: BorderSide::EMPTY,
    };

    /// Same width/color on every edge (typical KDL `border width=… color=…`).
    pub const fn uniform(width: f32, color: Color) -> Self {
        let side = BorderSide::new(width, color);
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    /// Builds a four-side border from legacy flat `border_width` / `border_color`.
    pub const fn from_legacy(width: Option<f32>, color: Option<Color>) -> Self {
        let side = BorderSide { width, color };
        Self {
            top: side,
            right: side,
            bottom: side,
            left: side,
        }
    }

    /// Side-wise merge for stacking Normal → Hovered state borders.
    pub const fn merged(self, overlay: Self) -> Self {
        Self {
            top: self.top.merged(overlay.top),
            right: self.right.merged(overlay.right),
            bottom: self.bottom.merged(overlay.bottom),
            left: self.left.merged(overlay.left),
        }
    }

    /// `(width, color)` when all four sides are fully specified and equal; else `None`.
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

/// Per-corner radii in logical pixels (KDL `radius` / iced `border::Radius`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadius {
    /// Top-left corner.
    pub top_left: f32,
    /// Top-right corner.
    pub top_right: f32,
    /// Bottom-right corner.
    pub bottom_right: f32,
    /// Bottom-left corner.
    pub bottom_left: f32,
}

impl CornerRadius {
    /// Same radius on every corner (most chrome); prefer [`Radius`] scale constants.
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

/// Full style payload for one [`Class`]: states, layout, and text from KDL.
///
/// Stored in `ThemeTokens.class_styles[class.index()]`. Widget factories read
/// `layout` / `text`; stylesheet helpers call [`ClassStyle::resolve`].
#[derive(Debug, Clone, Copy)]
pub struct ClassStyle {
    /// Paint overrides indexed by [`ComponentState::index`] (`Normal`, `Hovered`, …).
    pub states: [VisualStyle; ComponentState::COUNT],
    /// Size/padding/margin/spacing from the component’s `layout { … }` block.
    pub layout: ComponentLayout,
    /// Default font size/weight from the component’s `text { … }` block.
    pub text: ComponentTextStyle,
}

impl ClassStyle {
    /// Zeroed class: no paint, layout, or text overrides.
    pub const EMPTY: Self = Self {
        states: [VisualStyle::EMPTY; ComponentState::COUNT],
        layout: ComponentLayout::EMPTY,
        text: ComponentTextStyle::EMPTY,
    };

    /// `Normal` style merged with the requested state (state fields win).
    pub fn resolve(self, state: ComponentState) -> VisualStyle {
        self.states[ComponentState::Normal.index()].merged(self.states[state.index()])
    }
}

/// Size and box-model properties from a KDL `layout { … }` block on a component.
///
/// Widget factories (e.g. `toolbar_button`) call the `padding_*` / `margin_*`
/// helpers so missing sides fall back to axis values or a [`Spacing`] constant.
#[derive(Debug, Clone, Copy)]
pub struct ComponentLayout {
    /// Fixed width in logical pixels when set.
    pub width: Option<f32>,
    /// iced `Length::FillPortion` width when set (row/column children).
    pub width_portion: Option<u16>,
    /// Fixed height in logical pixels when set.
    pub height: Option<f32>,
    /// Inner padding (KDL `padding` shorthand or per-side).
    pub padding: BoxSpacing,
    /// Outer margin/gutter around the component.
    pub margin: BoxSpacing,
    /// Gap between children when the component is a layout container.
    pub spacing: Option<f32>,
}

impl ComponentLayout {
    /// No layout overrides — callers must supply their own fallbacks.
    pub const EMPTY: Self = Self {
        width: None,
        width_portion: None,
        height: None,
        padding: BoxSpacing::EMPTY,
        margin: BoxSpacing::EMPTY,
        spacing: None,
    };

    /// Field-wise merge for stacking theme defaults under component KDL.
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

    /// Horizontal padding: left/right if set, else `fallback` (typically a [`Spacing`] constant).
    ///
    /// Backs iced `.padding([y, x])` on buttons and inputs when KDL omits sides.
    pub fn padding_x(self, fallback: f32) -> f32 {
        self.padding.horizontal(fallback)
    }

    /// Vertical padding: top/bottom if set, else `fallback`.
    pub fn padding_y(self, fallback: f32) -> f32 {
        self.padding.vertical(fallback)
    }

    /// Left padding; falls back to [`Self::padding_x`] then `fallback`.
    pub fn padding_left(self, fallback: f32) -> f32 {
        self.padding
            .left
            .unwrap_or_else(|| self.padding_x(fallback))
    }

    /// Right padding; falls back to [`Self::padding_x`] then `fallback`.
    pub fn padding_right(self, fallback: f32) -> f32 {
        self.padding
            .right
            .unwrap_or_else(|| self.padding_x(fallback))
    }

    /// Top padding; falls back to [`Self::padding_y`] then `fallback`.
    pub fn padding_top(self, fallback: f32) -> f32 {
        self.padding.top.unwrap_or_else(|| self.padding_y(fallback))
    }

    /// Bottom padding; falls back to [`Self::padding_y`] then `fallback`.
    pub fn padding_bottom(self, fallback: f32) -> f32 {
        self.padding
            .bottom
            .unwrap_or_else(|| self.padding_y(fallback))
    }

    /// Horizontal margin: left/right if set, else `fallback`.
    pub fn margin_x(self, fallback: f32) -> f32 {
        self.margin.horizontal(fallback)
    }

    /// Vertical margin: top/bottom if set, else `fallback`.
    pub fn margin_y(self, fallback: f32) -> f32 {
        self.margin.vertical(fallback)
    }

    /// Left margin; falls back to [`Self::margin_x`] then `fallback`.
    pub fn margin_left(self, fallback: f32) -> f32 {
        self.margin.left.unwrap_or_else(|| self.margin_x(fallback))
    }

    /// Right margin; falls back to [`Self::margin_x`] then `fallback`.
    pub fn margin_right(self, fallback: f32) -> f32 {
        self.margin.right.unwrap_or_else(|| self.margin_x(fallback))
    }

    /// Top margin; falls back to [`Self::margin_y`] then `fallback`.
    pub fn margin_top(self, fallback: f32) -> f32 {
        self.margin.top.unwrap_or_else(|| self.margin_y(fallback))
    }

    /// Bottom margin; falls back to [`Self::margin_y`] then `fallback`.
    pub fn margin_bottom(self, fallback: f32) -> f32 {
        self.margin
            .bottom
            .unwrap_or_else(|| self.margin_y(fallback))
    }
}

/// CSS-like optional sides for KDL padding/margin shorthands.
///
/// Parsed from `padding 8`, `padding 4 12`, or `padding top=…` style nodes.
/// Unset sides stay `None` so [`ComponentLayout`] helpers can apply scale fallbacks.
#[derive(Debug, Clone, Copy)]
pub struct BoxSpacing {
    /// Top side when explicitly set in KDL.
    pub top: Option<f32>,
    /// Right side when explicitly set in KDL.
    pub right: Option<f32>,
    /// Bottom side when explicitly set in KDL.
    pub bottom: Option<f32>,
    /// Left side when explicitly set in KDL.
    pub left: Option<f32>,
}

impl BoxSpacing {
    /// All sides unset (inherit fallbacks from layout helpers).
    pub const EMPTY: Self = Self {
        top: None,
        right: None,
        bottom: None,
        left: None,
    };

    /// Same value on every side (KDL single-number padding/margin).
    pub const fn uniform(value: f32) -> Self {
        Self {
            top: Some(value),
            right: Some(value),
            bottom: Some(value),
            left: Some(value),
        }
    }

    /// Two-value CSS/iced shorthand: `vertical` top/bottom, `horizontal` left/right.
    pub const fn axes(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: Some(vertical),
            right: Some(horizontal),
            bottom: Some(vertical),
            left: Some(horizontal),
        }
    }

    /// Four-value CSS order: top, right, bottom, left.
    pub const fn sides(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top: Some(top),
            right: Some(right),
            bottom: Some(bottom),
            left: Some(left),
        }
    }

    /// Field-wise merge; overlay `Some` sides replace the base.
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

/// Optional font size/weight from a KDL `text { … }` block on a component.
///
/// Widget factories fall back to [`FontSize`] / [`FontWeight`] constants when unset.
#[derive(Debug, Clone, Copy)]
pub struct ComponentTextStyle {
    /// Font size in logical pixels when set by theme/component KDL.
    pub size: Option<u32>,
    /// iced font weight when set (`regular` / `medium` / `semibold` / `bold` in KDL).
    pub weight: Option<iced::font::Weight>,
}

impl ComponentTextStyle {
    /// No text overrides — factories use scale defaults.
    pub const EMPTY: Self = Self {
        size: None,
        weight: None,
    };

    /// Field-wise merge for theme → component stacking.
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

/// Drawing metrics that are not iced stylesheet fields (viewer, trees, chrome sizes).
///
/// Defaults live in Rust; theme/component KDL can override individual keys via the
/// style book. Stored on [`ThemeTokens::primitives`].
#[derive(Debug, Clone, Copy)]
pub struct PrimitiveTokens {
    /// PDF page drop-shadow X offset for the viewer canvas.
    pub page_shadow_offset_x: f32,
    /// PDF page drop-shadow Y offset for the viewer canvas.
    pub page_shadow_offset_y: f32,
    /// Unselected find-in-document highlight fill on the canvas.
    pub viewer_find_fill: Color,
    /// Active find match highlight fill on the canvas.
    pub viewer_find_selected_fill: Color,
    /// Mix toward accent when building text-selection fill.
    pub viewer_text_selection_mix: f32,
    /// Alpha applied to the text-selection overlay fill.
    pub viewer_text_selection_alpha: f32,
    /// Progress-bar thickness (girth) for library/import rails.
    pub progress_girth: f32,
    /// Slider track thickness (library grid zoom, etc.).
    pub slider_rail_width: f32,
    /// Slider thumb radius.
    pub slider_handle_radius: f32,
    /// Main document scrollable rail width.
    pub scrollbar_width: f32,
    /// Main document scrollbar thumb width.
    pub scrollbar_scroller_width: f32,
    /// Sidebar scrollable rail width (denser chrome).
    pub sidebar_scrollbar_width: f32,
    /// Sidebar scrollbar thumb width.
    pub sidebar_scrollbar_scroller_width: f32,
    /// Scrollbar thumb corner radius.
    pub scrollbar_radius: f32,
    /// Middle-click auto-scroll affordance corner radius.
    pub auto_scroll_radius: f32,
    /// Auto-scroll affordance drop-shadow blur.
    pub auto_scroll_shadow_blur: f32,
    /// Placeholder “document lines” spacing on generated previews.
    pub document_preview_line_spacing: f32,
    /// Minimum width of a generated preview body line.
    pub document_preview_min_line_width: f32,
    /// Height of the bold heading line in generated previews.
    pub document_preview_heading_line_height: f32,
    /// Height of body lines in generated previews.
    pub document_preview_body_line_height: f32,
    /// Corner radius of generated preview line stubs.
    pub document_preview_line_radius: f32,
    /// Mix amount for flush media card backgrounds toward surface.
    pub flush_media_background_mix: f32,
    /// Icon size for library grid/list view toggles.
    pub library_view_toggle_icon_size: f32,
    /// Width reserved for the grid-zoom percentage label.
    pub library_grid_zoom_label_width: f32,
    /// Width of the metadata-density pick list.
    pub library_metadata_picker_width: f32,
    /// Max height of the library sort dropdown panel.
    pub library_sort_menu_height: f32,
    /// “Up one folder” drop-target icon width.
    pub folder_parent_icon_width: f32,
    /// “Up one folder” drop-target icon height.
    pub folder_parent_icon_height: f32,
    /// Folder glyph size inside folder cards.
    pub folder_icon_size: f32,
    /// Capsule width behind the folder glyph.
    pub folder_icon_container_width: f32,
    /// Capsule height behind the folder glyph.
    pub folder_icon_container_height: f32,
    /// Accent mix for the folder glyph capsule background.
    pub folder_icon_background_mix: f32,
    /// Library switcher row icon size in the sidebar.
    pub library_switcher_sidebar_icon_size: f32,
    /// Hit target slot around the switcher icon.
    pub library_switcher_sidebar_icon_slot: f32,
    /// Library switcher row height.
    pub library_switcher_sidebar_button_height: f32,
    /// Label width beside the library switcher icon.
    pub library_switcher_sidebar_text_width: f32,
    /// Expand/collapse chevron glyph size in trees.
    pub sidebar_chevron_icon_size: f32,
    /// Expand/collapse chevron button size.
    pub sidebar_chevron_button_size: f32,
    /// Expand/collapse chevron button padding.
    pub sidebar_chevron_button_padding: f32,
    /// Extra left indent per depth level in the library file tree.
    pub file_tree_indent_width: f32,
    /// Cap on cumulative file-tree indentation.
    pub file_tree_max_indent: f32,
    /// Estimated width per metadata character in tree rows.
    pub file_tree_meta_char_width: f32,
    /// Minimum metadata column width in tree rows.
    pub file_tree_meta_min_width: f32,
    /// Maximum metadata column width in tree rows.
    pub file_tree_meta_max_width: f32,
    /// Vertical padding inside a file-tree row.
    pub file_tree_row_padding_y: f32,
    /// Indent per depth in the Raindrop import collection tree.
    pub raindrop_tree_indent_width: f32,
    /// Cap on Raindrop import tree indentation.
    pub raindrop_tree_max_indent: f32,
    /// Width of the fold control column in the Raindrop tree.
    pub raindrop_tree_fold_width: f32,
    /// Vertical padding inside a Raindrop tree row.
    pub raindrop_tree_row_padding_y: f32,
    /// “New folder” icon size in Raindrop import UI.
    pub raindrop_new_folder_icon_size: f32,
    /// Hairline height for app-menu separators.
    pub menu_separator_height: f32,
    /// Hairline height for context-menu separators.
    pub context_menu_separator_height: f32,
    /// Height of overflow/menu buttons in the selection toolbar.
    pub selection_menu_button_height: f32,
}

impl Default for PrimitiveTokens {
    fn default() -> Self {
        Self {
            page_shadow_offset_x: 2.0,
            page_shadow_offset_y: 2.0,
            viewer_find_fill: Color::from_rgba(1.0, 0.725, 0.133, 0.52),
            viewer_find_selected_fill: Color::from_rgba(0.871, 0.498, 0.0, 0.68),
            viewer_text_selection_mix: 0.72,
            viewer_text_selection_alpha: 0.42,
            progress_girth: 3.0,
            slider_rail_width: 4.0,
            slider_handle_radius: 7.0,
            scrollbar_width: 4.0,
            scrollbar_scroller_width: 2.0,
            sidebar_scrollbar_width: 4.0,
            sidebar_scrollbar_scroller_width: 2.0,
            scrollbar_radius: 6.0,
            auto_scroll_radius: 999.0,
            auto_scroll_shadow_blur: 4.0,
            document_preview_line_spacing: 7.0,
            document_preview_min_line_width: 12.0,
            document_preview_heading_line_height: 4.0,
            document_preview_body_line_height: 2.0,
            document_preview_line_radius: 1.0,
            flush_media_background_mix: 0.42,
            library_view_toggle_icon_size: 18.0,
            library_grid_zoom_label_width: 44.0,
            library_metadata_picker_width: 130.0,
            library_sort_menu_height: 360.0,
            folder_parent_icon_width: 26.0,
            folder_parent_icon_height: 22.0,
            folder_icon_size: 22.0,
            folder_icon_container_width: 38.0,
            folder_icon_container_height: 28.0,
            folder_icon_background_mix: 0.18,
            library_switcher_sidebar_icon_size: 22.0,
            library_switcher_sidebar_icon_slot: 30.0,
            library_switcher_sidebar_button_height: 34.0,
            library_switcher_sidebar_text_width: 170.0,
            sidebar_chevron_icon_size: 18.0,
            sidebar_chevron_button_size: 28.0,
            sidebar_chevron_button_padding: 0.0,
            file_tree_indent_width: 12.0,
            file_tree_max_indent: 72.0,
            file_tree_meta_char_width: 6.0,
            file_tree_meta_min_width: 52.0,
            file_tree_meta_max_width: 128.0,
            file_tree_row_padding_y: 3.0,
            raindrop_tree_indent_width: 14.0,
            raindrop_tree_max_indent: 70.0,
            raindrop_tree_fold_width: 20.0,
            raindrop_tree_row_padding_y: 2.0,
            raindrop_new_folder_icon_size: 18.0,
            menu_separator_height: 1.0,
            context_menu_separator_height: 1.0,
            selection_menu_button_height: 30.0,
        }
    }
}

/// Fully resolved palette, per-class styles, and drawing primitives for one theme.
///
/// Obtained from [`crate::StyleBook::tokens`] or [`crate::AppTheme::tokens`].
/// Views should treat this as an immutable snapshot for a single frame. Named
/// colors come from `themes/*.kdl`; class paint/layout from `components/**/*.kdl`.
#[derive(Debug, Clone, Copy)]
pub struct ThemeTokens {
    /// Root window / app-shell background (`background` theme token).
    pub background: Color,
    /// Default chrome surface for toolbars and sidebars (`surface`).
    pub surface: Color,
    /// Elevated panels, menus, and cards (`surface_raised`).
    pub surface_raised: Color,
    /// Primary body/control label color (`text_primary`).
    pub text_primary: Color,
    /// Secondary/muted labels and metadata (`text_secondary`).
    pub text_secondary: Color,
    /// Interactive accent for selection, links, and active thumbs (`accent`).
    pub accent: Color,
    /// Default hairline / rail color (`border`).
    pub border: Color,
    /// Destructive/error emphasis (`error`).
    pub error: Color,
    /// PDF viewer canvas backdrop behind pages (`canvas`).
    pub canvas: Color,
    /// Unrendered page placeholder fill (`placeholder`).
    pub placeholder: Color,
    /// Keyboard focus ring color (`focus`).
    pub focus: Color,
    /// Shared shadow tint for elevated chrome and page drop shadows (`shadow`).
    pub shadow: Color,
    /// Per-[`Class`] paint/layout/text loaded from component KDL.
    pub class_styles: [ClassStyle; Class::COUNT],
    /// Non-palette metrics (scrollbars, find highlights, tree indents, …).
    pub primitives: PrimitiveTokens,
}

/// Spacing scale in logical pixels (padding, gaps between controls).
///
/// Prefer these constants over raw numbers so density stays consistent. Larger
/// layout metrics (sidebar width, card size) live in [`AppLayoutTokens`].
pub struct Spacing;

impl Spacing {
    /// Extra-small space (tight icon padding).
    pub const XS: f32 = 4.0;
    /// Small space (compact control padding).
    pub const SM: f32 = 6.0;
    /// Medium space (default row/section gap).
    pub const MD: f32 = 9.0;
    /// Large space (toolbar group separation).
    pub const LG: f32 = 14.0;
    /// Extra-large space (panel-level separation).
    pub const XL: f32 = 24.0;
    /// Horizontal gutter around viewer pages.
    pub const PAGE_GUTTER: f32 = 32.0;
    /// Vertical gap between rendered PDF pages.
    pub const PAGE_GAP: f32 = 24.0;
}

/// Corner-radius scale in logical pixels.
pub struct Radius;

impl Radius {
    /// Sharp edge (no rounding).
    pub const NONE: f32 = 0.0;
    /// Small radius for compact controls and menus.
    pub const SM: f32 = 6.0;
    /// Medium radius for cards and raised surfaces.
    pub const MD: f32 = 10.0;
}

/// Border-width scale in logical pixels.
pub struct BorderWidth;

impl BorderWidth {
    /// No visible border.
    pub const NONE: f32 = 0.0;
    /// Single-pixel hairline for surfaces and controls.
    pub const HAIRLINE: f32 = 1.0;
}

/// Font-size scale in logical pixels for UI text roles.
pub struct FontSize;

impl FontSize {
    /// Small metadata / secondary labels.
    pub const SM: u32 = 12;
    /// Default body text.
    pub const MD: u32 = 13;
    /// Control labels (buttons, inputs).
    pub const CONTROL: u32 = 14;
    /// Section headings.
    pub const HEADING: u32 = 16;
}

/// Font-weight aliases for semantic text roles.
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

/// Primary UI font family name (IBM Plex Sans), registered from bundled bytes.
pub const UI_FONT_FAMILY: &str = "IBM Plex Sans";
/// Display / brand font family name (Vollkorn), registered from bundled bytes.
pub const DISPLAY_FONT_FAMILY: &str = "Vollkorn";

/// iced [`Font`] for IBM Plex Sans at `weight` (UI chrome, body, controls).
///
/// Prefer this over ad-hoc families so bundled font registration stays in sync.
pub fn ui_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(UI_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

/// iced [`Font`] for Vollkorn at `weight` (section headings, brand display).
pub fn display_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(DISPLAY_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

