# PDF-Folio Style System

PDF-Folio routes repeated visual decisions through `pdf-folio-ui/src/style/` and external KDL
style files in `pdf-folio-ui/styles/`. Views should describe structure and message flow; colors,
radii, borders, shadows, primitive drawing values, and reusable widget states should live in the
style system.

## Modules

- `book.rs` parses bundled and user KDL style files into a validated `StyleBook`.
- `tokens.rs` defines semantic values such as `ThemeTokens`, `VisualStyle`, `Spacing`, `Radius`,
  `FontSize`, icon sizes, page gutters, and common UI dimensions.
- `classes.rs` defines reusable UI roles with `Class` and maps those roles to iced styles.
- `components.rs` provides small styled constructors such as `toolbar_button(...)`,
  `sidebar_button(...)`, `tag_pill(...)`, `search_input(...)`, `empty_state(...)`,
  `progress_bar(...)`, and `error_banner(...)`.
- `layout.rs` stores shared layout dimensions such as sidebar widths, virtualized library row
  heights, window size, and overlay input widths.

## KDL Styles

Bundled style files are compiled in as fallbacks, but in a development checkout PDF-Folio reads
`crates/pdf-folio-ui/styles/` from disk at runtime so edits can hot reload without relaunching.
User overrides are layered from:

```text
$XDG_CONFIG_HOME/pdf-folio/styles/*.kdl
```

If `XDG_CONFIG_HOME` is unset, PDF-Folio uses `~/.config/pdf-folio/styles/*.kdl`.
User files are optional. Invalid reloads keep the last valid style book and report the error in the
library status area/logs.

Basic shape:

```kdl
theme "espresso" {
    color "background" "#1A1208"
    color "surface" "#0F0A04"
    color "accent" "#D4A853"
    primitive "progress_girth" 3
}

component "LibraryCard" {
    normal background="#251A0E" text="#DDD0BA" border="#C8B89A1A" border_width=1 radius=8
    hovered background="#2E2010" border="#C8B89A2E"
    selected background="#30230F" border="#D4A853"
}
```

Supported component states are `normal`, `hovered`, `pressed`, `focused`, `disabled`, `selected`,
`active`, and `error`. State nodes can include `theme="espresso"` or `theme="light"` to target one
theme; otherwise they apply to every loaded theme.

Supported component properties are `background`, `text`, `text_color`, `border`, `border_color`,
`border_width`, and `radius`. Colors use `#RRGGBB`, `#RRGGBBAA`, `rgba(r,g,b,a)`, token references
such as `$accent`, or simple blends such as `mix($surface, $accent, 0.16)`.

Runtime reload is available through **View → Reload Styles**, `Ctrl+Shift+R`, and filesystem
watchers for both bundled checkout styles and user override files.

## Token Naming

Token names should describe intent, not a temporary color or measurement. Prefer
`tokens.surface`, `Spacing::PAGE_GUTTER`, or `FontSize::CONTROL` over values like `dark_gray`,
`32.0`, or `15`.

Light and dark themes must expose the same token names. Adding a token means adding both theme
values at the same time.

The current dark theme follows an espresso/walnut document-library palette: app background near
`#1A1208`, surfaces near `#0F0A04`, raised surfaces near `#251A0E`, parchment text, and amber
active states.

Text and content alignment should also go through the style system. Use `TextAlignment` for text
widgets and `ContentAlignment` plus `align_content_x(...)` / `align_content_y(...)` for container
content placement instead of hard-coding alignment directly in views.

## Class Naming

Class names describe UI roles: `Toolbar`, `Sidebar`, `LibraryCard`, `TocEntry`, `JumpOverlay`.
Avoid names based on current appearance, such as `BlueButton` or `LargeGrayPanel`.

Create a new class when a visual pattern repeats, has interactive state, or belongs to a feature
surface that will evolve. Inline styling is acceptable for one-off geometry inside a canvas or for
a genuinely local value that would be misleading as a global token.

Unsupported browser concepts such as selector cascade, margins, flex layout, and inheritance are
intentionally not part of this language. Layout and behavior still belong to iced view code.

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
