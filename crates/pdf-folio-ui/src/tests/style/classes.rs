use iced::widget::button;

use super::*;

fn tokens() -> ThemeTokens {
    crate::style::fallback_dark_tokens()
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

#[test]
fn visible_visual_borders_use_custom_border_path() {
    let border = VisualBorder::uniform(7.0, Color::BLACK);
    let style = VisualStyle {
        border: Some(border),
        ..VisualStyle::EMPTY
    };

    assert_eq!(side_border_for_style(style), Some(border));
}
