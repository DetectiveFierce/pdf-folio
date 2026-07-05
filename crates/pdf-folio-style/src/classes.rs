//! Reusable semantic style classes.

use iced::widget::{button, container, pick_list, progress_bar, scrollable, slider, text_input};
use iced::{overlay, Background, Border, Color, Shadow as IcedShadow, Vector};

use crate::tokens::{BorderWidth, CornerRadius, Radius, ThemeTokens, VisualBorder, VisualStyle};

/// Semantic style classes used by UI widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Whole application shell.
    AppShell,
    /// Top toolbar.
    Toolbar,
    /// Application menu bar.
    MenuBar,
    /// Top-level menu button.
    MenuButton,
    /// Dropdown menu panel.
    MenuPanel,
    /// Dropdown menu row.
    MenuItem,
    /// Group of controls inside the toolbar.
    ToolbarGroup,
    /// Toolbar button.
    ToolbarButton,
    /// Sidebar surface.
    Sidebar,
    /// Sidebar section.
    SidebarSection,
    /// Sidebar row.
    SidebarRow,
    /// Library sidebar tab button.
    SidebarTab,
    /// Library file/tag tree body.
    FileTree,
    /// Library file tree fold/expand button.
    FileTreeFoldButton,
    /// Library sidebar expand/collapse button.
    SidebarToggleButton,
    /// Library sidebar details panel.
    SidebarDetailPanel,
    /// Library sidebar detail row.
    SidebarDetailRow,
    /// Library sidebar action button.
    SidebarActionButton,
    /// Selected-folder sidebar card.
    SidebarFolderCard,
    /// Selected-folder sidebar card title.
    SidebarFolderCardTitle,
    /// Selected-folder sidebar rename input.
    SidebarFolderTextInput,
    /// Selected-folder sidebar card action button.
    SidebarFolderActionButton,
    /// Table-of-contents entry.
    TocEntry,
    /// Library grid card.
    LibraryCard,
    /// Library folder card.
    LibraryFolderCard,
    /// Library list row.
    LibraryRow,
    /// Library search/sort/import control bar.
    LibraryControlBar,
    /// Library search input.
    LibrarySearchInput,
    /// Library sort dropdown.
    LibrarySortDropdown,
    /// Library grid/list view toggle.
    LibraryViewToggle,
    /// Library import-folder button.
    LibraryImportButton,
    /// Library masonry grid zoom slider.
    LibraryGridZoomSlider,
    /// Tag pill.
    TagPill,
    /// Search input.
    SearchInput,
    /// Progress bar.
    ProgressBar,
    /// Error banner.
    ErrorBanner,
    /// Viewer canvas.
    ViewerCanvas,
    /// Viewer toolbar surface.
    ViewerToolbar,
    /// Viewer toolbar button.
    ViewerToolbarButton,
    /// Viewer toolbar title.
    ViewerToolbarTitle,
    /// Viewer page navigation control.
    ViewerPageControl,
    /// Viewer zoom control.
    ViewerZoomControl,
    /// Viewer zoom dropdown panel.
    ViewerZoomMenu,
    /// Viewer zoom dropdown row.
    ViewerZoomMenuItem,
    /// Viewer sidebar surface.
    ViewerSidebar,
    /// Viewer sidebar tab.
    ViewerSidebarTab,
    /// Viewer outline entry.
    ViewerOutlineEntry,
    /// Viewer page thumbnail.
    ViewerThumbnail,
    /// Viewer find popup.
    ViewerFindBar,
    /// Viewer find input.
    ViewerFindInput,
    /// Viewer find button.
    ViewerFindButton,
    /// Viewer page placeholder.
    ViewerPagePlaceholder,
    /// Backward-compatible page placeholder alias.
    PagePlaceholder,
    /// Jump-to-page overlay.
    JumpOverlay,
    /// Tooltip overlay.
    Tooltip,
    /// Annotation toolbar.
    AnnotationToolbar,
    /// Annotation popover.
    AnnotationPopover,
    /// Viewer presentation overlay.
    ViewerPresentationOverlay,
    /// Backward-compatible presentation overlay alias.
    PresentationOverlay,
    /// Viewer minimap.
    Minimap,
    /// Empty-state panel.
    EmptyState,
    /// Library drag insertion marker.
    DragInsertionMarker,
    /// Library entry selection checkbox.
    SelectionCheckbox,
    /// Library toolbar master selection checkbox.
    MasterCheckbox,
    /// Multi-selection drag stack ghost.
    DragStackGhost,
    /// Active folder target for PDF drag/drop assignment.
    FolderDropTarget,
    /// Right-click contextual menu panel.
    ContextMenuPanel,
    /// Right-click contextual menu row.
    ContextMenuItem,
    /// Selection toolbar restore action.
    SelectionRestoreButton,
    /// Selection toolbar destructive text action.
    SelectionDangerButton,
    /// Selection toolbar destructive icon action.
    SelectionDangerIconButton,
}

/// Visual state shared by components that do not expose an iced status directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentState {
    /// Normal state.
    Normal,
    /// Hovered state.
    Hovered,
    /// Pressed state.
    Pressed,
    /// Focused state.
    Focused,
    /// Disabled state.
    Disabled,
    /// Selected state.
    Selected,
    /// Active state.
    Active,
    /// Error state.
    Error,
}

impl ComponentState {
    /// Number of component states represented in style files.
    pub const COUNT: usize = 8;

    /// Stable index for style arrays.
    pub const fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Hovered => 1,
            Self::Pressed => 2,
            Self::Focused => 3,
            Self::Disabled => 4,
            Self::Selected => 5,
            Self::Active => 6,
            Self::Error => 7,
        }
    }
}

impl Class {
    /// Number of semantic classes represented in style files.
    pub const COUNT: usize = 71;

    /// Stable index for style arrays.
    pub const fn index(self) -> usize {
        match self {
            Self::AppShell => 0,
            Self::Toolbar => 1,
            Self::MenuBar => 2,
            Self::MenuButton => 3,
            Self::MenuPanel => 4,
            Self::MenuItem => 5,
            Self::ToolbarGroup => 6,
            Self::ToolbarButton => 7,
            Self::Sidebar => 8,
            Self::SidebarSection => 9,
            Self::SidebarRow => 10,
            Self::SidebarTab => 11,
            Self::FileTree => 12,
            Self::SidebarToggleButton => 13,
            Self::SidebarDetailPanel => 14,
            Self::SidebarDetailRow => 15,
            Self::SidebarActionButton => 16,
            Self::SidebarFolderCard => 17,
            Self::SidebarFolderCardTitle => 18,
            Self::SidebarFolderTextInput => 19,
            Self::SidebarFolderActionButton => 20,
            Self::TocEntry => 21,
            Self::LibraryCard => 22,
            Self::LibraryFolderCard => 23,
            Self::LibraryRow => 24,
            Self::LibraryControlBar => 25,
            Self::LibrarySearchInput => 26,
            Self::LibrarySortDropdown => 27,
            Self::LibraryViewToggle => 28,
            Self::LibraryImportButton => 29,
            Self::LibraryGridZoomSlider => 30,
            Self::TagPill => 31,
            Self::SearchInput => 32,
            Self::ProgressBar => 33,
            Self::ErrorBanner => 34,
            Self::ViewerCanvas => 35,
            Self::ViewerToolbar => 36,
            Self::ViewerToolbarButton => 37,
            Self::ViewerToolbarTitle => 38,
            Self::ViewerPageControl => 39,
            Self::ViewerZoomControl => 40,
            Self::ViewerZoomMenu => 41,
            Self::ViewerZoomMenuItem => 42,
            Self::ViewerSidebar => 43,
            Self::ViewerSidebarTab => 44,
            Self::ViewerOutlineEntry => 45,
            Self::ViewerThumbnail => 46,
            Self::ViewerFindBar => 47,
            Self::ViewerFindInput => 48,
            Self::ViewerFindButton => 49,
            Self::ViewerPagePlaceholder => 50,
            Self::PagePlaceholder => 51,
            Self::JumpOverlay => 52,
            Self::Tooltip => 53,
            Self::AnnotationToolbar => 54,
            Self::AnnotationPopover => 55,
            Self::ViewerPresentationOverlay => 56,
            Self::PresentationOverlay => 57,
            Self::Minimap => 58,
            Self::EmptyState => 59,
            Self::DragInsertionMarker => 60,
            Self::FileTreeFoldButton => 61,
            Self::SelectionCheckbox => 62,
            Self::MasterCheckbox => 63,
            Self::DragStackGhost => 64,
            Self::FolderDropTarget => 65,
            Self::ContextMenuPanel => 66,
            Self::ContextMenuItem => 67,
            Self::SelectionRestoreButton => 68,
            Self::SelectionDangerButton => 69,
            Self::SelectionDangerIconButton => 70,
        }
    }
}

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

/// Applies a parsed KDL visual override to an iced widget style.
pub trait VisualOverride {
    fn with_visual_override(self, style: VisualStyle) -> Self;
}

fn apply_border_override(border: &mut Border, style: VisualStyle) {
    if let Some(border_color) = style.border_color {
        border.color = border_color;
    }
    if let Some(border_width) = style.border_width {
        border.width = border_width;
    }
    if style.border.is_some() {
        border.width = 0.0;
    }
}

/// Returns a custom border for class styles that provide side-aware border data.
pub fn side_border_for_style(style: VisualStyle) -> Option<VisualBorder> {
    style.border.filter(visual_border_has_visible_side)
}

fn visual_border_has_visible_side(border: &VisualBorder) -> bool {
    [border.top, border.right, border.bottom, border.left]
        .into_iter()
        .any(|side| {
            side.width.is_some_and(|width| width > 0.0)
                && side.color.is_some_and(|color| color.a > 0.0)
        })
}

/// Returns a custom per-side border for a class in a component state.
pub fn side_border_for_class(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
) -> Option<VisualBorder> {
    side_border_for_style(tokens.class_styles[class.index()].resolve(state))
}

impl VisualOverride for container::Style {
    fn with_visual_override(mut self, style: VisualStyle) -> Self {
        if let Some(background) = style.background {
            self.background = Some(Background::Color(background));
        }
        if let Some(text_color) = style.text_color {
            self.text_color = Some(text_color);
        }
        apply_border_override(&mut self.border, style);
        if let Some(radius) = style.radius {
            self.border.radius = radius.into();
        }
        if let Some(shadow) = style.shadow {
            self.shadow = shadow.into();
        }
        self
    }
}

impl VisualOverride for button::Style {
    fn with_visual_override(mut self, style: VisualStyle) -> Self {
        if let Some(background) = style.background {
            self.background = Some(Background::Color(background));
        }
        if let Some(text_color) = style.text_color {
            self.text_color = text_color;
        }
        apply_border_override(&mut self.border, style);
        if let Some(radius) = style.radius {
            self.border.radius = radius.into();
        }
        if let Some(shadow) = style.shadow {
            self.shadow = shadow.into();
        }
        self
    }
}

impl VisualOverride for pick_list::Style {
    fn with_visual_override(mut self, style: VisualStyle) -> Self {
        if let Some(background) = style.background {
            self.background = Background::Color(background);
        }
        if let Some(text_color) = style.text_color {
            self.text_color = text_color;
        }
        apply_border_override(&mut self.border, style);
        if let Some(radius) = style.radius {
            self.border.radius = radius.into();
        }
        self
    }
}

impl VisualOverride for text_input::Style {
    fn with_visual_override(mut self, style: VisualStyle) -> Self {
        if let Some(background) = style.background {
            self.background = Background::Color(background);
        }
        if let Some(text_color) = style.text_color {
            self.value = text_color;
        }
        apply_border_override(&mut self.border, style);
        if let Some(radius) = style.radius {
            self.border.radius = radius.into();
        }
        self
    }
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

/// Returns an iced container style for a semantic class.
pub fn container_style(tokens: ThemeTokens, class: Class) -> container::Style {
    let (background, text_color, border_color, border_width, radius) = match class {
        Class::AppShell => (
            tokens.background,
            tokens.text_primary,
            tokens.border,
            BorderWidth::NONE,
            Radius::NONE,
        ),
        Class::Toolbar
        | Class::ViewerToolbar
        | Class::MenuBar
        | Class::Sidebar
        | Class::ViewerSidebar
        | Class::SidebarSection
        | Class::LibraryControlBar => (
            tokens.surface,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::NONE,
        ),
        Class::MenuPanel | Class::ContextMenuPanel => (
            tokens.surface_raised,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::JumpOverlay
        | Class::Tooltip
        | Class::AnnotationToolbar
        | Class::AnnotationPopover
        | Class::ViewerPresentationOverlay
        | Class::PresentationOverlay
        | Class::Minimap => (
            tokens.surface,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::LibraryCard
        | Class::LibraryFolderCard
        | Class::LibraryRow
        | Class::SidebarDetailPanel
        | Class::SidebarDetailRow
        | Class::SidebarFolderCard
        | Class::EmptyState => (
            tokens.surface_raised,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::MD,
        ),
        Class::DragInsertionMarker => (
            tokens.focus,
            tokens.text_primary,
            tokens.focus,
            BorderWidth::NONE,
            Radius::SM,
        ),
        Class::SelectionCheckbox | Class::MasterCheckbox => (
            tokens.surface_raised,
            tokens.text_primary,
            tokens.accent,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::DragStackGhost | Class::FolderDropTarget => (
            mix_color(tokens.surface_raised, tokens.accent, 0.18),
            tokens.text_primary,
            tokens.focus,
            BorderWidth::HAIRLINE,
            Radius::MD,
        ),
        Class::TagPill => (
            mix_color(tokens.surface, tokens.accent, 0.12),
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::ErrorBanner => (
            mix_color(tokens.surface, tokens.error, 0.18),
            tokens.text_primary,
            tokens.error,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::PagePlaceholder | Class::ViewerPagePlaceholder => (
            tokens.placeholder,
            tokens.text_secondary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::ViewerFindBar
        | Class::ViewerZoomMenu
        | Class::ViewerThumbnail
        | Class::ViewerToolbarTitle
        | Class::ViewerPageControl
        | Class::ViewerZoomControl => (
            tokens.surface_raised,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::ViewerCanvas
        | Class::ToolbarGroup
        | Class::ToolbarButton
        | Class::ViewerToolbarButton
        | Class::ViewerFindButton
        | Class::ViewerZoomMenuItem
        | Class::LibrarySortDropdown
        | Class::LibraryViewToggle
        | Class::LibraryImportButton
        | Class::LibraryGridZoomSlider
        | Class::SidebarToggleButton
        | Class::SidebarActionButton
        | Class::SidebarFolderCardTitle
        | Class::SidebarFolderActionButton
        | Class::MenuButton
        | Class::MenuItem
        | Class::ContextMenuItem
        | Class::SelectionRestoreButton
        | Class::SelectionDangerButton
        | Class::SelectionDangerIconButton
        | Class::SidebarRow
        | Class::ViewerSidebarTab
        | Class::ViewerOutlineEntry
        | Class::SidebarTab
        | Class::FileTree
        | Class::FileTreeFoldButton
        | Class::TocEntry
        | Class::LibrarySearchInput
        | Class::ViewerFindInput
        | Class::SidebarFolderTextInput
        | Class::SearchInput
        | Class::ProgressBar => (
            tokens.background,
            tokens.text_primary,
            tokens.border,
            BorderWidth::NONE,
            Radius::NONE,
        ),
    };

    let override_style = tokens.class_styles[class.index()].resolve(ComponentState::Normal);
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text_color),
        border: Border {
            width: border_width,
            color: border_color,
            radius: radius.into(),
        },
        ..container::Style::default()
    }
    .with_visual_override(override_style)
}

/// Returns an iced button style for a semantic class.
pub fn button_style(tokens: ThemeTokens, class: Class, status: button::Status) -> button::Style {
    let base = match class {
        Class::LibraryCard | Class::LibraryFolderCard | Class::LibraryRow => tokens.surface_raised,
        Class::TagPill => mix_color(tokens.background, tokens.accent, 0.18),
        Class::ToolbarButton
        | Class::ViewerToolbarButton
        | Class::ViewerFindButton
        | Class::ViewerZoomMenuItem
        | Class::LibrarySortDropdown
        | Class::LibraryViewToggle
        | Class::LibraryImportButton
        | Class::SidebarActionButton
        | Class::MenuItem => tokens.surface_raised,
        Class::MenuButton
        | Class::SidebarRow
        | Class::ViewerSidebarTab
        | Class::ViewerOutlineEntry
        | Class::SidebarTab
        | Class::FileTree
        | Class::FileTreeFoldButton
        | Class::SidebarToggleButton
        | Class::TocEntry => tokens.surface,
        _ => tokens.surface,
    };

    let state = match status {
        button::Status::Active => ComponentState::Normal,
        button::Status::Hovered => ComponentState::Hovered,
        button::Status::Pressed => ComponentState::Pressed,
        button::Status::Disabled => ComponentState::Disabled,
    };

    let background = match state {
        ComponentState::Normal => base,
        ComponentState::Hovered | ComponentState::Focused => mix_color(base, tokens.accent, 0.14),
        ComponentState::Pressed | ComponentState::Selected | ComponentState::Active => {
            mix_color(base, tokens.accent, 0.24)
        }
        ComponentState::Disabled => tokens.background,
        ComponentState::Error => mix_color(base, tokens.error, 0.24),
    };
    let border_color = match state {
        ComponentState::Focused | ComponentState::Selected | ComponentState::Active => tokens.focus,
        ComponentState::Error => tokens.error,
        _ => tokens.border,
    };
    let text_color = if matches!(state, ComponentState::Disabled) {
        tokens.text_secondary
    } else {
        tokens.text_primary
    };

    let override_style = tokens.class_styles[class.index()].resolve(state);
    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: border_color,
            radius: if matches!(class, Class::LibraryCard | Class::LibraryFolderCard) {
                Radius::MD.into()
            } else {
                Radius::SM.into()
            },
        },
        ..button::Style::default()
    }
    .with_visual_override(override_style)
}

/// Returns an iced slider style for a semantic class.
pub fn slider_style(tokens: ThemeTokens, class: Class, status: slider::Status) -> slider::Style {
    let state = match status {
        slider::Status::Active => ComponentState::Normal,
        slider::Status::Hovered => ComponentState::Hovered,
        slider::Status::Dragged => ComponentState::Pressed,
    };
    let style = tokens.class_styles[class.index()].resolve(state);
    let rail_active = style.text_color.unwrap_or(tokens.accent);
    let rail_rest = style
        .background
        .unwrap_or_else(|| mix_color(tokens.surface_raised, tokens.background, 0.36));
    let border_color = style.border_color.unwrap_or(tokens.border);
    let radius = style
        .radius
        .unwrap_or_else(|| CornerRadius::uniform(999.0))
        .into();

    slider::Style {
        rail: slider::Rail {
            backgrounds: (rail_active.into(), rail_rest.into()),
            width: tokens.primitives.slider_rail_width,
            border: Border {
                radius,
                width: style.border_width.unwrap_or(BorderWidth::NONE),
                color: border_color,
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle {
                radius: tokens.primitives.slider_handle_radius,
            },
            background: rail_active.into(),
            border_color,
            border_width: style.border_width.unwrap_or(BorderWidth::HAIRLINE),
        },
    }
}

/// Returns an iced pick-list style for a semantic class.
pub fn pick_list_style(
    tokens: ThemeTokens,
    class: Class,
    status: pick_list::Status,
) -> pick_list::Style {
    let is_active = matches!(
        status,
        pick_list::Status::Hovered | pick_list::Status::Opened { .. }
    );
    let background = if is_active {
        mix_color(tokens.surface_raised, tokens.accent, 0.16)
    } else {
        tokens.surface_raised
    };

    let state = if is_active {
        ComponentState::Hovered
    } else {
        ComponentState::Normal
    };
    let override_style = tokens.class_styles[class.index()].resolve(state);
    pick_list::Style {
        text_color: tokens.text_primary,
        placeholder_color: tokens.text_secondary,
        handle_color: tokens.text_secondary,
        background: Background::Color(background),
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: if is_active {
                tokens.focus
            } else {
                tokens.border
            },
            radius: Radius::SM.into(),
        },
    }
    .with_visual_override(override_style)
}

/// Returns an iced dropdown menu style for themed popup menus.
pub fn menu_style(tokens: ThemeTokens) -> overlay::menu::Style {
    menu_style_for_class(tokens, Class::MenuPanel)
}

/// Returns an iced dropdown menu style for a semantic class.
pub fn menu_style_for_class(tokens: ThemeTokens, class: Class) -> overlay::menu::Style {
    let override_style = tokens.class_styles[class.index()].resolve(ComponentState::Normal);
    let style = overlay::menu::Style {
        background: Background::Color(tokens.surface_raised),
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: tokens.border,
            radius: Radius::SM.into(),
        },
        text_color: tokens.text_primary,
        selected_text_color: tokens.text_primary,
        selected_background: Background::Color(mix_color(
            tokens.surface_raised,
            tokens.accent,
            0.30,
        )),
        shadow: IcedShadow {
            color: tokens.shadow,
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
    };
    overlay::menu::Style {
        background: override_style
            .background
            .map_or(style.background, Background::Color),
        border: Border {
            width: override_style.border_width.unwrap_or(style.border.width),
            color: override_style.border_color.unwrap_or(style.border.color),
            radius: override_style
                .radius
                .map_or(style.border.radius, Into::into),
        },
        text_color: override_style.text_color.unwrap_or(style.text_color),
        selected_text_color: override_style
            .text_color
            .unwrap_or(style.selected_text_color),
        selected_background: style.selected_background,
        shadow: override_style.shadow.map_or(style.shadow, Into::into),
    }
}

/// Returns an iced text-input style for a semantic class.
pub fn text_input_style(
    tokens: ThemeTokens,
    class: Class,
    status: text_input::Status,
) -> text_input::Style {
    let is_focused = matches!(status, text_input::Status::Focused { .. });
    let is_hovered = matches!(
        status,
        text_input::Status::Hovered | text_input::Status::Focused { is_hovered: true }
    );
    let background = match (class, is_focused || is_hovered) {
        (
            Class::SearchInput
            | Class::LibrarySearchInput
            | Class::SidebarFolderTextInput
            | Class::ViewerFindInput,
            true,
        ) => mix_color(tokens.surface_raised, tokens.accent, 0.16),
        (
            Class::SearchInput
            | Class::LibrarySearchInput
            | Class::SidebarFolderTextInput
            | Class::ViewerFindInput,
            false,
        ) => tokens.surface_raised,
        (_, true) => mix_color(tokens.surface, tokens.accent, 0.16),
        (_, false) => tokens.surface,
    };

    let state = if is_focused {
        ComponentState::Focused
    } else if is_hovered {
        ComponentState::Hovered
    } else {
        ComponentState::Normal
    };
    let override_style = tokens.class_styles[class.index()].resolve(state);
    text_input::Style {
        background: Background::Color(background),
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: if is_focused {
                tokens.focus
            } else {
                tokens.border
            },
            radius: 18.0.into(),
        },
        icon: tokens.text_secondary,
        placeholder: mix_color(tokens.background, tokens.text_secondary, 0.48),
        value: tokens.text_primary,
        selection: mix_color(tokens.surface_raised, tokens.accent, 0.44),
    }
    .with_visual_override(override_style)
}

/// Returns an iced progress-bar style for a semantic class.
pub fn progress_bar_style(tokens: ThemeTokens, _class: Class) -> progress_bar::Style {
    let override_style =
        tokens.class_styles[Class::ProgressBar.index()].resolve(ComponentState::Normal);
    progress_bar::Style {
        background: Background::Color(override_style.background.unwrap_or(tokens.surface_raised)),
        bar: Background::Color(override_style.text_color.unwrap_or(tokens.accent)),
        border: Border {
            width: override_style.border_width.unwrap_or(BorderWidth::NONE),
            color: override_style.border_color.unwrap_or(tokens.border),
            radius: override_style
                .radius
                .unwrap_or_else(|| CornerRadius::uniform(2.0))
                .into(),
        },
    }
}

/// Returns an iced scrollable style for a semantic class.
pub fn scrollable_style(
    tokens: ThemeTokens,
    _class: Class,
    status: scrollable::Status,
) -> scrollable::Style {
    let base_scroller = match status {
        scrollable::Status::Active { .. } => tokens.border,
        scrollable::Status::Hovered { .. } => mix_color(tokens.border, tokens.focus, 0.42),
        scrollable::Status::Dragged { .. } => tokens.accent,
    };
    let rail = scrollable::Rail {
        background: Some(Background::Color(mix_color(
            tokens.background,
            tokens.surface,
            0.64,
        ))),
        border: Border {
            width: BorderWidth::NONE,
            color: tokens.border,
            radius: tokens.primitives.scrollbar_radius.into(),
        },
        scroller: scrollable::Scroller {
            background: Background::Color(base_scroller),
            border: Border {
                width: BorderWidth::NONE,
                color: base_scroller,
                radius: tokens.primitives.scrollbar_radius.into(),
            },
        },
    };

    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: Some(Background::Color(tokens.surface)),
        auto_scroll: scrollable::AutoScroll {
            background: Background::Color(mix_color(
                tokens.surface_raised,
                tokens.background,
                0.18,
            )),
            border: Border {
                width: BorderWidth::HAIRLINE,
                color: tokens.focus,
                radius: tokens.primitives.auto_scroll_radius.into(),
            },
            shadow: IcedShadow {
                color: tokens.shadow,
                offset: Vector::ZERO,
                blur_radius: tokens.primitives.auto_scroll_shadow_blur,
            },
            icon: tokens.text_primary,
        },
    }
}

/// Returns an iced scrollable style for sidebar scrollbars.
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

/// Blends two colors by the provided amount.
pub fn mix_color(base: Color, overlay: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color {
        r: base.r + (overlay.r - base.r) * amount,
        g: base.g + (overlay.g - base.g) * amount,
        b: base.b + (overlay.b - base.b) * amount,
        a: base.a + (overlay.a - base.a) * amount,
    }
}

#[cfg(test)]
#[path = "tests/style/classes.rs"]
mod tests;
