# PDF-Folio Style System

PDF-Folio routes repeated visual decisions through `pdf-folio-ui/src/style/`.
Views should describe structure and message flow; colors, spacing, radii, and reusable widget
states should live in the style system.

## Modules

- `tokens.rs` defines semantic values such as `ThemeTokens`, `Spacing`, `Radius`, `FontSize`,
  icon sizes, page gutters, and common UI dimensions.
- `classes.rs` defines reusable UI roles with `Class` and maps those roles to iced styles.
- `components.rs` provides small styled constructors such as `toolbar_button(...)`,
  `sidebar_button(...)`, `tag_pill(...)`, `search_input(...)`, `empty_state(...)`,
  `progress_bar(...)`, and `error_banner(...)`.
- `layout.rs` stores shared layout dimensions such as sidebar widths, virtualized library row
  heights, window size, and overlay input widths.

## Token Naming

Token names should describe intent, not a temporary color or measurement. Prefer
`tokens.surface`, `Spacing::PAGE_GUTTER`, or `FontSize::CONTROL` over values like `dark_gray`,
`32.0`, or `15`.

Light and dark themes must expose the same token names. Adding a token means adding both theme
values at the same time.

The current dark theme intentionally follows a neutral gray document-library palette: app
background near `#181818`, surfaces near `#202020`, raised surfaces near `#282828`, hover/active
states mixed toward muted gray, and subdued gray text/borders. Keep new dark-theme values in that
family unless a feature needs a semantic status color.

Text and content alignment should also go through the style system. Use `TextAlignment` for text
widgets and `ContentAlignment` plus `align_content_x(...)` / `align_content_y(...)` for container
content placement instead of hard-coding alignment directly in views.

## Class Naming

Class names describe UI roles: `Toolbar`, `Sidebar`, `LibraryCard`, `TocEntry`, `JumpOverlay`.
Avoid names based on current appearance, such as `BlueButton` or `LargeGrayPanel`.

Create a new class when a visual pattern repeats, has interactive state, or belongs to a feature
surface that will evolve. Inline styling is acceptable for one-off geometry inside a canvas or for
a genuinely local value that would be misleading as a global token.

## Styled Helpers

Helpers should stay small and composable. They may accept labels, content, tokens, and messages via
iced callbacks, but they should not read application state, database state, document state, or
rendering state.

Preferred pattern:

```rust
toolbar_button("Open", tokens).on_press(Message::OpenFileDialog)
```

For aligned text:

```rust
aligned_text("Tags", tokens, FontSize::HEADING, TextAlignment::Start)
```

For more specialized UI, compose a helper with normal iced layout:

```rust
container(column![section_heading("Contents", tokens), body])
    .style(move |_| container_style(tokens, Class::Sidebar))
```

## Component States

Interactive components should use the shared state vocabulary from `ComponentState`: normal,
hovered, pressed, focused, disabled, selected, active, and error. If iced exposes a native status,
map it into this vocabulary before choosing colors or borders.

Focused, selected, and active states should use `tokens.focus` or `tokens.accent` strongly enough
to remain visible in both light and dark themes.

## Viewer And Overlays

Viewer canvas primitives use `viewer_primitives(tokens)` so the page background, placeholder, and
shadow can evolve without touching scroll or render math. Annotation toolbars, annotation
popovers, presentation overlays, and minimap controls should reuse `Class::AnnotationToolbar`,
`Class::AnnotationPopover`, `Class::PresentationOverlay`, and `Class::Minimap`.

## Anti-Patterns

- Do not add raw `Color::from_rgb8(...)` values to ordinary view code.
- Do not scatter repeated dimensions such as sidebar widths, card heights, or page gutters.
- Do not move app behavior into style helpers.
- Do not create a large component helper that hides message routing or document/library decisions.
- Do not add a new visual state for one component if it can map to the shared state vocabulary.
