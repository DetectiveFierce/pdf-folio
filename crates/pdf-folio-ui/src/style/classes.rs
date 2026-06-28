//! Reusable semantic style classes.

use iced::widget::{button, container, progress_bar, text_input};
use iced::{Background, Border, Color};

use super::tokens::{BorderWidth, Radius, ThemeTokens};

/// Semantic style classes used by UI widgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Whole application shell.
    AppShell,
    /// Top toolbar.
    Toolbar,
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
    /// Table-of-contents entry.
    TocEntry,
    /// Library grid card.
    LibraryCard,
    /// Library list row.
    LibraryRow,
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
    /// Page placeholder.
    PagePlaceholder,
    /// Jump-to-page overlay.
    JumpOverlay,
    /// Annotation toolbar.
    AnnotationToolbar,
    /// Annotation popover.
    AnnotationPopover,
    /// Presentation overlay.
    PresentationOverlay,
    /// Viewer minimap.
    Minimap,
    /// Empty-state panel.
    EmptyState,
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
}

/// Returns viewer canvas drawing primitives for the active theme.
pub fn viewer_primitives(tokens: ThemeTokens) -> ViewerPrimitiveStyle {
    ViewerPrimitiveStyle {
        canvas: tokens.canvas,
        placeholder: tokens.placeholder,
        page_shadow: Shadow {
            offset_x: 2.0,
            offset_y: 2.0,
            color: tokens.shadow,
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
        Class::Toolbar | Class::Sidebar | Class::SidebarSection => (
            tokens.surface,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::NONE,
        ),
        Class::JumpOverlay
        | Class::AnnotationToolbar
        | Class::AnnotationPopover
        | Class::PresentationOverlay
        | Class::Minimap => (
            tokens.surface,
            tokens.text_primary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::LibraryCard | Class::LibraryRow | Class::EmptyState => (
            tokens.surface,
            tokens.text_primary,
            tokens.border,
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
        Class::PagePlaceholder => (
            tokens.placeholder,
            tokens.text_secondary,
            tokens.border,
            BorderWidth::HAIRLINE,
            Radius::SM,
        ),
        Class::ViewerCanvas
        | Class::ToolbarGroup
        | Class::ToolbarButton
        | Class::SidebarRow
        | Class::TocEntry
        | Class::SearchInput
        | Class::ProgressBar => (
            tokens.background,
            tokens.text_primary,
            tokens.border,
            BorderWidth::NONE,
            Radius::NONE,
        ),
    };

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
}

/// Returns an iced button style for a semantic class.
pub fn button_style(tokens: ThemeTokens, class: Class, status: button::Status) -> button::Style {
    let base = match class {
        Class::LibraryCard | Class::LibraryRow => tokens.surface,
        Class::TagPill => mix_color(tokens.background, tokens.accent, 0.18),
        Class::ToolbarButton | Class::SidebarRow | Class::TocEntry => tokens.surface_raised,
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
        ComponentState::Hovered | ComponentState::Focused => mix_color(base, tokens.accent, 0.18),
        ComponentState::Pressed | ComponentState::Selected | ComponentState::Active => {
            mix_color(base, tokens.accent, 0.30)
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

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: border_color,
            radius: Radius::SM.into(),
        },
        ..button::Style::default()
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
        (Class::SearchInput, true) => mix_color(tokens.surface_raised, tokens.accent, 0.16),
        (Class::SearchInput, false) => tokens.surface_raised,
        (_, true) => mix_color(tokens.surface, tokens.accent, 0.16),
        (_, false) => tokens.surface,
    };

    text_input::Style {
        background: Background::Color(background),
        border: Border {
            width: BorderWidth::HAIRLINE,
            color: if is_focused {
                tokens.focus
            } else {
                tokens.border
            },
            radius: 20.0.into(),
        },
        icon: tokens.text_secondary,
        placeholder: mix_color(tokens.background, tokens.text_secondary, 0.48),
        value: tokens.text_primary,
        selection: mix_color(tokens.surface_raised, tokens.accent, 0.44),
    }
}

/// Returns an iced progress-bar style for a semantic class.
pub fn progress_bar_style(tokens: ThemeTokens, _class: Class) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(tokens.surface_raised),
        bar: Background::Color(tokens.accent),
        border: Border {
            width: BorderWidth::NONE,
            color: tokens.border,
            radius: 2.0.into(),
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
mod tests {
    use iced::widget::button;

    use super::*;

    fn tokens() -> ThemeTokens {
        ThemeTokens {
            background: Color::from_rgb(0.0, 0.0, 0.0),
            surface: Color::from_rgb(0.1, 0.1, 0.1),
            surface_raised: Color::from_rgb(0.2, 0.2, 0.2),
            text_primary: Color::WHITE,
            text_secondary: Color::from_rgb(0.7, 0.7, 0.7),
            accent: Color::from_rgb(0.2, 0.4, 0.8),
            border: Color::from_rgb(0.3, 0.3, 0.3),
            error: Color::from_rgb(0.8, 0.1, 0.1),
            canvas: Color::from_rgb(0.05, 0.05, 0.05),
            placeholder: Color::from_rgb(0.4, 0.4, 0.4),
            focus: Color::from_rgb(0.2, 0.6, 1.0),
            shadow: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
        }
    }

    #[test]
    fn container_classes_produce_semantic_surfaces() {
        let tokens = tokens();
        let shell = container_style(tokens, Class::AppShell);
        let toolbar = container_style(tokens, Class::Toolbar);
        let error = container_style(tokens, Class::ErrorBanner);

        assert_eq!(shell.background, Some(Background::Color(tokens.background)));
        assert_eq!(toolbar.background, Some(Background::Color(tokens.surface)));
        assert_eq!(error.border.color, tokens.error);
    }

    #[test]
    fn button_states_are_visually_distinct() {
        let tokens = tokens();
        let active = button_style(tokens, Class::ToolbarButton, button::Status::Active);
        let hovered = button_style(tokens, Class::ToolbarButton, button::Status::Hovered);
        let pressed = button_style(tokens, Class::ToolbarButton, button::Status::Pressed);

        assert_ne!(active.background, hovered.background);
        assert_ne!(hovered.background, pressed.background);
    }
}
