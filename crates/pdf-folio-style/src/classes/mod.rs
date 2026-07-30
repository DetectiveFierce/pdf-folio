//! Semantic style classes and iced stylesheet closures.
//!
//! A [`Class`] names a **UI role** (`LibraryCard`, `ViewerFindBar`, `Sidebar`),
//! not a paint description (`BlueButton`). Each class has an array of
//! [`ComponentState`] styles inside [`ThemeTokens`](crate::ThemeTokens), filled
//! from KDL `component "…"` blocks.
//!
//! # Mapping to iced
//!
//! | Helper | iced widget |
//! | --- | --- |
//! | [`button_style`] | `button` |
//! | [`container_style`] | `container` |
//! | [`text_input_style`] | `text_input` |
//! | [`scrollable_style`] / [`sidebar_scrollable_style`] | `scrollable` |
//! | [`slider_style`] | `slider` |
//! | [`pick_list_style`] / [`menu_style`] | pick list / overlay menu |
//! | [`progress_bar_style`] | `progress_bar` |
//!
//! Submodules split the surface area: [`core`] (shell chrome), [`library`]
//! (sidebar scrollbars), [`viewer`] (canvas primitives).
//!
//! Class names in KDL must match the PascalCase enum variants (e.g.
//! `component "ToolbarButton" { … }`).

use crate::tokens::VisualStyle;

/// Semantic style class identifying a UI role for theming.
///
/// Indices from [`Class::index`] address `ThemeTokens.class_styles`. Keep
/// [`Class::COUNT`] in sync when adding variants.
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

/// Interaction / selection state for a styled component.
///
/// Mirrors KDL state children under `component "…"` (`normal`, `hovered`, …).
/// iced widgets map their native status into these states inside the `*_style`
/// helpers; custom chrome can pass a state explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentState {
    /// Default resting appearance.
    Normal,
    /// Pointer is over the control.
    Hovered,
    /// Pointer is pressed on the control.
    Pressed,
    /// Keyboard / focus ring active.
    Focused,
    /// Control is not interactive.
    Disabled,
    /// Item is selected in a list/grid.
    Selected,
    /// Toggle or mode is active (e.g. current sidebar tab).
    Active,
    /// Validation or operational error emphasis.
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

pub mod core;
pub mod library;
pub mod viewer;

/// Applies a parsed KDL [`VisualStyle`] on top of an iced widget style.
///
/// Implemented for iced `container`, `button`, and similar style structs so
/// stylesheet helpers can start from a baseline and layer book overrides.
pub trait VisualOverride {
    /// Merges non-empty fields from `style` into `self`.
    fn with_visual_override(self, style: VisualStyle) -> Self;
}

pub use core::{
    button_style, container_style, menu_style, menu_style_for_class, mix_color, pick_list_style,
    progress_bar_style, scrollable_style, side_border_for_class, side_border_for_style,
    slider_style, text_input_style,
};
pub use library::sidebar_scrollable_style;
pub use viewer::{viewer_primitives, Shadow, ViewerPrimitiveStyle};

#[cfg(test)]
mod tests;
