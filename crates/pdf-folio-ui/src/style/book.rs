//! External KDL-backed style book.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use iced::Color;
use kdl::{KdlDocument, KdlNode, KdlValue};

use super::classes::{mix_color, Class, ComponentState};
use super::tokens::{ClassStyle, PrimitiveTokens, ThemeTokens, VisualStyle};

const BUNDLED_STYLE_FILES: [(&str, &str); 4] = [
    (
        "styles/themes/espresso.kdl",
        include_str!("../../styles/themes/espresso.kdl"),
    ),
    (
        "styles/themes/light.kdl",
        include_str!("../../styles/themes/light.kdl"),
    ),
    (
        "styles/components/core.kdl",
        include_str!("../../styles/components/core.kdl"),
    ),
    (
        "styles/components/library.kdl",
        include_str!("../../styles/components/library.kdl"),
    ),
];

/// Parsed and validated style data.
#[derive(Debug, Clone)]
pub struct StyleBook {
    themes: HashMap<String, ThemeTokens>,
    style_dirs: Vec<PathBuf>,
}

impl StyleBook {
    /// Loads bundled styles plus any user overrides found in the XDG config directory.
    pub fn load() -> Result<Arc<Self>, String> {
        let mut sources = bundled_style_sources()?;
        let user_style_dir = user_style_dir();

        if let Some(dir) = &user_style_dir {
            for file in user_style_files(dir) {
                let source = std::fs::read_to_string(&file)
                    .map_err(|error| format!("{}: {error}", file.display()))?;
                sources.push((file.display().to_string(), source));
            }
        }

        Self::from_sources(sources, style_source_dirs()).map(Arc::new)
    }

    /// Loads only bundled styles.
    pub fn bundled() -> Arc<Self> {
        Self::from_sources(
            BUNDLED_STYLE_FILES
                .iter()
                .map(|(name, source)| ((*name).to_owned(), (*source).to_owned()))
                .collect(),
            style_source_dirs(),
        )
        .unwrap_or_else(|error| panic!("bundled PDF-Folio styles must be valid: {error}"))
        .into()
    }

    /// Builds a style book from named KDL sources.
    pub fn from_sources(
        sources: Vec<(String, String)>,
        style_dirs: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let mut raw = RawStyleBook::default();
        for (name, source) in sources {
            raw.apply_source(&name, &source)?;
        }
        Ok(Self {
            themes: raw.compile()?,
            style_dirs,
        })
    }

    /// Returns the tokens for a theme id, falling back to `espresso`.
    pub fn tokens(&self, theme: &str) -> ThemeTokens {
        self.themes
            .get(theme)
            .copied()
            .or_else(|| self.themes.get("espresso").copied())
            .unwrap_or_else(fallback_dark_tokens)
    }

    /// Directories watched for style changes.
    pub fn style_dirs(&self) -> &[PathBuf] {
        &self.style_dirs
    }
}

#[derive(Debug, Default)]
struct RawStyleBook {
    themes: HashMap<String, RawTheme>,
}

#[derive(Debug, Clone)]
struct RawTheme {
    tokens: ThemeTokens,
}

impl RawStyleBook {
    fn apply_source(&mut self, name: &str, source: &str) -> Result<(), String> {
        let document = KdlDocument::from_str(source)
            .map_err(|error| format!("{name}: failed to parse KDL: {error}"))?;
        for node in document.nodes() {
            match node.name().value() {
                "theme" => self.apply_theme_node(name, node)?,
                "component" => self.apply_component_node(name, node)?,
                "primitive" => self.apply_primitive_node(name, node)?,
                other => {
                    return Err(format!(
                        "{name}: unsupported top-level style node `{other}`"
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply_theme_node(&mut self, name: &str, node: &KdlNode) -> Result<(), String> {
        let theme_name = node_string_arg(name, node, 0)?;
        let mut tokens = match theme_name {
            "light" => fallback_light_tokens(),
            "espresso" | "dark" => fallback_dark_tokens(),
            other => {
                return Err(format!("{name}: unknown theme `{other}`"));
            }
        };
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: theme `{theme_name}` must have children"))?;
        for child in children.nodes() {
            let key = child.name().value();
            match key {
                "color" => {
                    let token = node_string_arg(name, child, 0)?;
                    let value = parse_color_literal(node_string_arg(name, child, 1)?)
                        .map_err(|error| format!("{name}: color `{token}`: {error}"))?;
                    set_theme_color(&mut tokens, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                "primitive" => {
                    let token = node_string_arg(name, child, 0)?;
                    let value = node_f32_arg(name, child, 1)?;
                    set_primitive(&mut tokens.primitives, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                other => {
                    return Err(format!(
                        "{name}: unsupported theme property `{other}` in `{theme_name}`"
                    ));
                }
            }
        }
        self.themes
            .insert(theme_name.to_owned(), RawTheme { tokens });
        Ok(())
    }

    fn apply_component_node(&mut self, name: &str, node: &KdlNode) -> Result<(), String> {
        let class = parse_class(node_string_arg(name, node, 0)?)
            .ok_or_else(|| format!("{name}: unknown style class"))?;
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: component `{class:?}` must have state children"))?;

        for child in children.nodes() {
            let state = parse_state(child.name().value()).ok_or_else(|| {
                format!(
                    "{name}: unknown component state `{}` for `{class:?}`",
                    child.name().value()
                )
            })?;
            let target_themes = child
                .get("theme")
                .and_then(KdlValue::as_string)
                .map(|theme| vec![theme.to_owned()])
                .unwrap_or_else(|| self.themes.keys().cloned().collect());
            for theme in target_themes {
                let Some(raw_theme) = self.themes.get_mut(&theme) else {
                    return Err(format!(
                        "{name}: component `{class:?}` references unknown theme `{theme}`"
                    ));
                };
                let style = parse_visual_style(name, child, &raw_theme.tokens)?;
                raw_theme.tokens.class_styles[class.index()].states[state.index()] =
                    raw_theme.tokens.class_styles[class.index()].states[state.index()]
                        .merged(style);
            }
        }
        Ok(())
    }

    fn apply_primitive_node(&mut self, name: &str, node: &KdlNode) -> Result<(), String> {
        let primitive = node_string_arg(name, node, 0)?;
        let value = node_f32_arg(name, node, 1)?;
        for raw_theme in self.themes.values_mut() {
            set_primitive(&mut raw_theme.tokens.primitives, primitive, value)
                .map_err(|error| format!("{name}: {error}"))?;
        }
        Ok(())
    }

    fn compile(self) -> Result<HashMap<String, ThemeTokens>, String> {
        if !self.themes.contains_key("espresso") {
            return Err(String::from("missing required `espresso` theme"));
        }
        if !self.themes.contains_key("light") {
            return Err(String::from("missing required `light` theme"));
        }
        Ok(self
            .themes
            .into_iter()
            .map(|(name, raw)| (name, raw.tokens))
            .collect())
    }
}

fn parse_visual_style(
    name: &str,
    node: &KdlNode,
    tokens: &ThemeTokens,
) -> Result<VisualStyle, String> {
    let mut style = VisualStyle::EMPTY;
    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        match property {
            "background" => {
                style.background = Some(parse_color_value(name, entry.value(), tokens)?)
            }
            "text" | "text_color" => {
                style.text_color = Some(parse_color_value(name, entry.value(), tokens)?)
            }
            "border" | "border_color" => {
                style.border_color = Some(parse_color_value(name, entry.value(), tokens)?)
            }
            "border_width" => {
                style.border_width = Some(value_as_f32(name, entry.value())?);
            }
            "radius" => {
                style.radius = Some(value_as_f32(name, entry.value())?);
            }
            "theme" => {}
            other => {
                return Err(format!("{name}: unsupported component property `{other}`"));
            }
        }
    }
    Ok(style)
}

fn parse_color_value(name: &str, value: &KdlValue, tokens: &ThemeTokens) -> Result<Color, String> {
    let source = value
        .as_string()
        .ok_or_else(|| format!("{name}: expected color string"))?;
    parse_color_expression(source, tokens).map_err(|error| format!("{name}: {error}"))
}

fn parse_color_expression(source: &str, tokens: &ThemeTokens) -> Result<Color, String> {
    let source = source.trim();
    if let Some(token) = source.strip_prefix('$') {
        return theme_color(tokens, token).ok_or_else(|| format!("unknown color token `{source}`"));
    }
    if let Some(args) = source
        .strip_prefix("mix(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let parts = args.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(format!("invalid mix expression `{source}`"));
        }
        let base = parse_color_expression(parts[0], tokens)?;
        let overlay = parse_color_expression(parts[1], tokens)?;
        let amount = parts[2]
            .parse::<f32>()
            .map_err(|_| format!("invalid mix amount in `{source}`"))?;
        return Ok(mix_color(base, overlay, amount));
    }
    parse_color_literal(source)
}

fn parse_color_literal(source: &str) -> Result<Color, String> {
    let source = source.trim();
    if let Some(args) = source
        .strip_prefix("rgba(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let values = args
            .split(',')
            .map(|part| part.trim().parse::<f32>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| format!("invalid rgba color `{source}`"))?;
        if values.len() != 4 {
            return Err(format!("invalid rgba color `{source}`"));
        }
        return Ok(Color::from_rgba8(
            values[0].clamp(0.0, 255.0) as u8,
            values[1].clamp(0.0, 255.0) as u8,
            values[2].clamp(0.0, 255.0) as u8,
            values[3].clamp(0.0, 1.0),
        ));
    }
    let hex = source
        .strip_prefix('#')
        .ok_or_else(|| format!("invalid color `{source}`"))?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(format!("invalid color `{source}`"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("invalid color `{source}`"))?;
    let a = if hex.len() == 8 {
        f32::from(
            u8::from_str_radix(&hex[6..8], 16).map_err(|_| format!("invalid color `{source}`"))?,
        ) / 255.0
    } else {
        1.0
    };
    Ok(Color::from_rgba8(r, g, b, a))
}

fn theme_color(tokens: &ThemeTokens, token: &str) -> Option<Color> {
    Some(match token {
        "background" => tokens.background,
        "surface" => tokens.surface,
        "surface_raised" => tokens.surface_raised,
        "text_primary" => tokens.text_primary,
        "text_secondary" => tokens.text_secondary,
        "accent" => tokens.accent,
        "border" => tokens.border,
        "error" => tokens.error,
        "canvas" => tokens.canvas,
        "placeholder" => tokens.placeholder,
        "focus" => tokens.focus,
        "shadow" => tokens.shadow,
        _ => return None,
    })
}

fn node_string_arg<'a>(name: &str, node: &'a KdlNode, index: usize) -> Result<&'a str, String> {
    node.get(index)
        .and_then(KdlValue::as_string)
        .ok_or_else(|| {
            format!(
                "{name}: node `{}` missing string argument {index}",
                node.name().value()
            )
        })
}

fn node_f32_arg(name: &str, node: &KdlNode, index: usize) -> Result<f32, String> {
    node.get(index)
        .map(|value| value_as_f32(name, value))
        .transpose()?
        .ok_or_else(|| {
            format!(
                "{name}: node `{}` missing numeric argument {index}",
                node.name().value()
            )
        })
}

fn value_as_f32(name: &str, value: &KdlValue) -> Result<f32, String> {
    let number = match value {
        KdlValue::Integer(value) => *value as f32,
        KdlValue::Float(value) => *value as f32,
        _ => return Err(format!("{name}: expected numeric value")),
    };
    if !number.is_finite() || number < 0.0 {
        return Err(format!("{name}: expected finite non-negative number"));
    }
    Ok(number)
}

fn parse_class(value: &str) -> Option<Class> {
    Some(match value {
        "AppShell" => Class::AppShell,
        "Toolbar" => Class::Toolbar,
        "MenuBar" => Class::MenuBar,
        "MenuButton" => Class::MenuButton,
        "MenuPanel" => Class::MenuPanel,
        "MenuItem" => Class::MenuItem,
        "ToolbarGroup" => Class::ToolbarGroup,
        "ToolbarButton" => Class::ToolbarButton,
        "Sidebar" => Class::Sidebar,
        "SidebarSection" => Class::SidebarSection,
        "SidebarRow" => Class::SidebarRow,
        "TocEntry" => Class::TocEntry,
        "LibraryCard" => Class::LibraryCard,
        "LibraryFolderCard" => Class::LibraryFolderCard,
        "LibraryRow" => Class::LibraryRow,
        "TagPill" => Class::TagPill,
        "SearchInput" => Class::SearchInput,
        "ProgressBar" => Class::ProgressBar,
        "ErrorBanner" => Class::ErrorBanner,
        "ViewerCanvas" => Class::ViewerCanvas,
        "PagePlaceholder" => Class::PagePlaceholder,
        "JumpOverlay" => Class::JumpOverlay,
        "Tooltip" => Class::Tooltip,
        "AnnotationToolbar" => Class::AnnotationToolbar,
        "AnnotationPopover" => Class::AnnotationPopover,
        "PresentationOverlay" => Class::PresentationOverlay,
        "Minimap" => Class::Minimap,
        "EmptyState" => Class::EmptyState,
        "DragInsertionMarker" => Class::DragInsertionMarker,
        _ => return None,
    })
}

fn parse_state(value: &str) -> Option<ComponentState> {
    Some(match value {
        "normal" => ComponentState::Normal,
        "hovered" | "hover" => ComponentState::Hovered,
        "pressed" => ComponentState::Pressed,
        "focused" | "focus" => ComponentState::Focused,
        "disabled" => ComponentState::Disabled,
        "selected" => ComponentState::Selected,
        "active" => ComponentState::Active,
        "error" => ComponentState::Error,
        _ => return None,
    })
}

fn set_theme_color(tokens: &mut ThemeTokens, token: &str, color: Color) -> Result<(), String> {
    match token {
        "background" => tokens.background = color,
        "surface" => tokens.surface = color,
        "surface_raised" => tokens.surface_raised = color,
        "text_primary" => tokens.text_primary = color,
        "text_secondary" => tokens.text_secondary = color,
        "accent" => tokens.accent = color,
        "border" => tokens.border = color,
        "error" => tokens.error = color,
        "canvas" => tokens.canvas = color,
        "placeholder" => tokens.placeholder = color,
        "focus" => tokens.focus = color,
        "shadow" => tokens.shadow = color,
        other => return Err(format!("unknown theme color `{other}`")),
    }
    Ok(())
}

fn set_primitive(tokens: &mut PrimitiveTokens, token: &str, value: f32) -> Result<(), String> {
    match token {
        "page_shadow_offset_x" => tokens.page_shadow_offset_x = value,
        "page_shadow_offset_y" => tokens.page_shadow_offset_y = value,
        "progress_girth" => tokens.progress_girth = value,
        other => return Err(format!("unknown primitive `{other}`")),
    }
    Ok(())
}

fn user_style_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("pdf-folio").join("styles"))
}

fn bundled_style_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("styles")
}

fn style_source_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let bundled = bundled_style_dir();
    if bundled.exists() {
        dirs.push(bundled);
    }
    if let Some(user) = user_style_dir().filter(|path| path.exists()) {
        dirs.push(user);
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

fn bundled_style_sources() -> Result<Vec<(String, String)>, String> {
    let bundled_dir = bundled_style_dir();
    BUNDLED_STYLE_FILES
        .iter()
        .map(|(relative, fallback)| {
            let path = bundled_dir.join(relative.strip_prefix("styles/").unwrap_or(relative));
            if path.exists() {
                let source = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{}: {error}", path.display()))?;
                Ok((path.display().to_string(), source))
            } else {
                Ok(((*relative).to_owned(), (*fallback).to_owned()))
            }
        })
        .collect()
}

fn user_style_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "kdl"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Built-in dark fallback used when style loading fails before app startup.
pub fn fallback_dark_tokens() -> ThemeTokens {
    let mut tokens = ThemeTokens {
        background: Color::from_rgb8(26, 18, 8),
        surface: Color::from_rgb8(15, 10, 4),
        surface_raised: Color::from_rgb8(37, 26, 14),
        text_primary: Color::from_rgb8(221, 208, 186),
        text_secondary: Color::from_rgb8(139, 110, 82),
        accent: Color::from_rgb8(212, 168, 83),
        border: Color::from_rgba8(200, 184, 154, 0.18),
        error: Color::from_rgb8(217, 64, 64),
        canvas: Color::from_rgb8(24, 15, 5),
        placeholder: Color::from_rgb8(46, 32, 16),
        focus: Color::from_rgb8(212, 168, 83),
        shadow: Color::from_rgba8(0, 0, 0, 0.62),
        class_styles: [ClassStyle::EMPTY; Class::COUNT],
        primitives: PrimitiveTokens::default(),
    };
    apply_fallback_class_styles(&mut tokens);
    tokens
}

/// Built-in light fallback used when style loading fails before app startup.
pub fn fallback_light_tokens() -> ThemeTokens {
    let mut tokens = ThemeTokens {
        background: Color::from_rgb8(241, 239, 233),
        surface: Color::from_rgb8(252, 250, 245),
        surface_raised: Color::from_rgb8(246, 242, 234),
        text_primary: Color::from_rgb8(38, 30, 20),
        text_secondary: Color::from_rgb8(116, 95, 70),
        accent: Color::from_rgb8(156, 115, 43),
        border: Color::from_rgb8(216, 206, 188),
        error: Color::from_rgb8(176, 48, 64),
        canvas: Color::from_rgb8(232, 226, 216),
        placeholder: Color::from_rgb8(224, 216, 202),
        focus: Color::from_rgb8(156, 115, 43),
        shadow: Color::from_rgba8(0, 0, 0, 0.16),
        class_styles: [ClassStyle::EMPTY; Class::COUNT],
        primitives: PrimitiveTokens::default(),
    };
    apply_fallback_class_styles(&mut tokens);
    tokens
}

fn apply_fallback_class_styles(tokens: &mut ThemeTokens) {
    for class in [
        Class::AppShell,
        Class::Toolbar,
        Class::MenuBar,
        Class::Sidebar,
        Class::SidebarSection,
    ] {
        set_class_state(
            tokens,
            class,
            ComponentState::Normal,
            VisualStyle {
                background: Some(tokens.surface),
                text_color: Some(tokens.text_primary),
                border_color: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(0.0),
            },
        );
    }
    set_class_state(
        tokens,
        Class::AppShell,
        ComponentState::Normal,
        VisualStyle {
            background: Some(tokens.background),
            border_width: Some(0.0),
            ..VisualStyle::EMPTY
        },
    );
    for class in [
        Class::LibraryCard,
        Class::LibraryFolderCard,
        Class::LibraryRow,
        Class::EmptyState,
        Class::MenuPanel,
        Class::JumpOverlay,
        Class::Tooltip,
        Class::AnnotationToolbar,
        Class::AnnotationPopover,
        Class::PresentationOverlay,
        Class::Minimap,
    ] {
        set_class_state(
            tokens,
            class,
            ComponentState::Normal,
            VisualStyle {
                background: Some(tokens.surface_raised),
                text_color: Some(tokens.text_primary),
                border_color: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(6.0),
            },
        );
    }
    for class in [
        Class::ToolbarButton,
        Class::MenuButton,
        Class::MenuItem,
        Class::SidebarRow,
        Class::TocEntry,
        Class::TagPill,
    ] {
        set_class_state(
            tokens,
            class,
            ComponentState::Normal,
            VisualStyle {
                background: Some(tokens.surface),
                text_color: Some(tokens.text_primary),
                border_color: Some(tokens.border),
                border_width: Some(1.0),
                radius: Some(6.0),
            },
        );
        set_class_state(
            tokens,
            class,
            ComponentState::Hovered,
            VisualStyle {
                background: Some(mix_color(tokens.surface, tokens.accent, 0.14)),
                border_color: Some(tokens.focus),
                ..VisualStyle::EMPTY
            },
        );
        set_class_state(
            tokens,
            class,
            ComponentState::Pressed,
            VisualStyle {
                background: Some(mix_color(tokens.surface, tokens.accent, 0.24)),
                ..VisualStyle::EMPTY
            },
        );
    }
}

fn set_class_state(
    tokens: &mut ThemeTokens,
    class: Class,
    state: ComponentState,
    style: VisualStyle,
) {
    tokens.class_styles[class.index()].states[state.index()] =
        tokens.class_styles[class.index()].states[state.index()].merged(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_styles_compile() {
        let style_book = StyleBook::bundled();
        let tokens = style_book.tokens("espresso");
        assert_eq!(tokens.accent, Color::from_rgb8(212, 168, 83));
    }

    #[test]
    fn invalid_color_is_rejected() {
        let result = StyleBook::from_sources(
            vec![(
                "bad.kdl".to_owned(),
                r##"theme "espresso" { color "accent" "wat" }"##.to_owned(),
            )],
            Vec::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn unknown_class_is_rejected() {
        let result = StyleBook::from_sources(
            vec![(
                "bad.kdl".to_owned(),
                r##"
                theme "espresso" {}
                theme "light" {}
                component "Nope" { normal background="#000000" }
                "##
                .to_owned(),
            )],
            Vec::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn token_references_and_mix_expressions_compile() {
        let style_book = StyleBook::from_sources(
            vec![(
                "style.kdl".to_owned(),
                r##"
                theme "espresso" { color "accent" "#D4A853" }
                theme "light" { color "accent" "#9C732B" }
                component "ToolbarButton" {
                    normal background="$surface" text="mix($accent, #000000, 0.5)"
                }
                "##
                .to_owned(),
            )],
            Vec::new(),
        )
        .expect("style book should compile");

        let tokens = style_book.tokens("espresso");
        let style =
            tokens.class_styles[Class::ToolbarButton.index()].resolve(ComponentState::Normal);
        assert_eq!(style.background, Some(tokens.surface));
        assert!(style.text_color.is_some());
    }
}
