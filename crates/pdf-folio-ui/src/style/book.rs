//! External KDL-backed style book.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use iced::Color;
use kdl::{KdlDocument, KdlNode, KdlValue};

use super::classes::{mix_color, Class, ComponentState};
use super::tokens::{
    AppLabelTokens, AppLayoutTokens, BorderSide, BoxSpacing, ClassStyle, ComponentLayout,
    ComponentTextStyle, CornerRadius, LabelSection, PrimitiveTokens, ThemeTokens, VisualBorder,
    VisualStyle,
};

const BUNDLED_STYLE_FILES: [(&str, &str); 6] = [
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
        "styles/components/library/sidebar.kdl",
        include_str!("../../styles/components/library/sidebar.kdl"),
    ),
    (
        "styles/components/library/library.kdl",
        include_str!("../../styles/components/library/library.kdl"),
    ),
    (
        "styles/application.kdl",
        include_str!("../../styles/application.kdl"),
    ),
];

/// Parsed and validated style data.
#[derive(Debug, Clone)]
pub struct StyleBook {
    themes: HashMap<String, ThemeTokens>,
    layout: AppLayoutTokens,
    labels: AppLabelTokens,
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
        let layout = raw.layout.clone();
        let labels = raw.labels.clone();
        Ok(Self {
            themes: raw.compile()?,
            layout,
            labels,
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

    /// Returns KDL-backed layout tokens for the application shell.
    pub fn layout(&self) -> &AppLayoutTokens {
        &self.layout
    }

    /// Returns KDL-backed label tokens for the application shell.
    pub fn labels(&self) -> &AppLabelTokens {
        &self.labels
    }

    /// Directories watched for style changes.
    pub fn style_dirs(&self) -> &[PathBuf] {
        &self.style_dirs
    }
}

#[derive(Debug, Default)]
struct RawStyleBook {
    themes: HashMap<String, RawTheme>,
    layout: AppLayoutTokens,
    labels: AppLabelTokens,
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
                "layout" => self.apply_layout_node(name, node)?,
                "labels" => self.apply_labels_node(name, node)?,
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
        let component_name = node_string_arg(name, node, 0)?;
        let Some(class) = parse_class(component_name) else {
            return self.apply_app_component_node(name, component_name, node);
        };
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: component `{class:?}` must have state children"))?;

        for child in children.nodes() {
            match child.name().value() {
                "layout" => {
                    let layout = self.apply_class_component_layout_node(name, class, child)?;
                    for raw_theme in self.themes.values_mut() {
                        let current = raw_theme.tokens.class_styles[class.index()].layout;
                        raw_theme.tokens.class_styles[class.index()].layout =
                            current.merged(layout);
                    }
                }
                "text" => {
                    let text = parse_component_text(name, child)?;
                    for raw_theme in self.themes.values_mut() {
                        let current = raw_theme.tokens.class_styles[class.index()].text;
                        raw_theme.tokens.class_styles[class.index()].text = current.merged(text);
                    }
                }
                "labels" => {
                    self.apply_component_labels_node(name, class, child)?;
                }
                state_name => {
                    let state = parse_state(state_name).ok_or_else(|| {
                        format!("{name}: unknown component property or state `{state_name}`")
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
            }
        }
        Ok(())
    }

    fn apply_class_component_layout_node(
        &mut self,
        name: &str,
        class: Class,
        node: &KdlNode,
    ) -> Result<ComponentLayout, String> {
        let mut layout = ComponentLayout::EMPTY;
        for entry in node.entries() {
            let Some(property) = entry.name().map(|name| name.value()) else {
                continue;
            };
            match property {
                "width" => {
                    let value = value_as_f32(name, entry.value())?;
                    layout.width = Some(value);
                    match class {
                        Class::Sidebar => self.layout.library_sidebar_width = value,
                        Class::LibraryCard => self.layout.library_grid_card_width = value,
                        _ => {}
                    }
                }
                "width_portion" => {
                    layout.width_portion = Some(value_as_u16(name, entry.value())?);
                }
                "height" => layout.height = Some(value_as_f32(name, entry.value())?),
                "padding" => {
                    layout.padding = layout
                        .padding
                        .merged(BoxSpacing::uniform(value_as_f32(name, entry.value())?));
                }
                "padding_x" => {
                    let value = value_as_f32(name, entry.value())?;
                    layout.padding = layout.padding.merged(BoxSpacing {
                        left: Some(value),
                        right: Some(value),
                        ..BoxSpacing::EMPTY
                    });
                }
                "padding_y" => {
                    let value = value_as_f32(name, entry.value())?;
                    layout.padding = layout.padding.merged(BoxSpacing {
                        top: Some(value),
                        bottom: Some(value),
                        ..BoxSpacing::EMPTY
                    });
                }
                "padding_left" => layout.padding.left = Some(value_as_f32(name, entry.value())?),
                "padding_right" => layout.padding.right = Some(value_as_f32(name, entry.value())?),
                "padding_top" => layout.padding.top = Some(value_as_f32(name, entry.value())?),
                "padding_bottom" => {
                    layout.padding.bottom = Some(value_as_f32(name, entry.value())?);
                }
                "margin" => {
                    layout.margin = layout
                        .margin
                        .merged(BoxSpacing::uniform(value_as_f32(name, entry.value())?));
                }
                "margin_x" => {
                    let value = value_as_f32(name, entry.value())?;
                    layout.margin = layout.margin.merged(BoxSpacing {
                        left: Some(value),
                        right: Some(value),
                        ..BoxSpacing::EMPTY
                    });
                }
                "margin_y" => {
                    let value = value_as_f32(name, entry.value())?;
                    layout.margin = layout.margin.merged(BoxSpacing {
                        top: Some(value),
                        bottom: Some(value),
                        ..BoxSpacing::EMPTY
                    });
                }
                "margin_left" => layout.margin.left = Some(value_as_f32(name, entry.value())?),
                "margin_right" => layout.margin.right = Some(value_as_f32(name, entry.value())?),
                "margin_top" => layout.margin.top = Some(value_as_f32(name, entry.value())?),
                "margin_bottom" => {
                    layout.margin.bottom = Some(value_as_f32(name, entry.value())?);
                }
                "spacing" => layout.spacing = Some(value_as_f32(name, entry.value())?),
                other => self.apply_class_layout_property(name, class, other, entry.value())?,
            }
        }
        if let Some(children) = node.children() {
            for child in children.nodes() {
                match child.name().value() {
                    "padding" => {
                        layout.padding = layout.padding.merged(parse_box_spacing(name, child)?);
                    }
                    "margin" => {
                        layout.margin = layout.margin.merged(parse_box_spacing(name, child)?);
                    }
                    other => return Err(format!("{name}: unknown layout child `{other}`")),
                }
            }
        }
        Ok(layout)
    }

    fn apply_class_layout_property(
        &mut self,
        name: &str,
        class: Class,
        property: &str,
        value: &KdlValue,
    ) -> Result<(), String> {
        match class {
            Class::Sidebar => match property {
                "min_width" => self.layout.library_sidebar_min_width = value_as_f32(name, value)?,
                "max_width" => self.layout.library_sidebar_max_width = value_as_f32(name, value)?,
                "resize_handle_width" => {
                    self.layout.sidebar_resize_handle_width = value_as_f32(name, value)?
                }
                "resize_handle_visual_width" => {
                    self.layout.sidebar_resize_handle_visual_width = value_as_f32(name, value)?
                }
                other => return Err(format!("{name}: unknown Sidebar layout `{other}`")),
            },
            Class::LibraryCard => match property {
                "columns" => self.layout.card_grid_columns = value_as_usize(name, value)?,
                "row_height" => self.layout.library_grid_row_height = value_as_f32(name, value)?,
                "content_width" => {
                    self.layout.library_card_content_width = value_as_f32(name, value)?
                }
                "title_width" => self.layout.library_card_title_width = value_as_f32(name, value)?,
                "info_height" => self.layout.library_card_info_height = value_as_f32(name, value)?,
                "media_max_height" => {
                    self.layout.library_card_media_max_height = value_as_f32(name, value)?
                }
                "thumbnail_width" => {
                    self.layout.library_card_thumbnail_width = value_as_f32(name, value)?
                }
                "masonry_gap" => self.layout.library_masonry_gap = value_as_f32(name, value)?,
                "scrollbar_gutter" => {
                    self.layout.library_scrollbar_gutter = value_as_f32(name, value)?
                }
                other => return Err(format!("{name}: unknown LibraryCard layout `{other}`")),
            },
            Class::LibraryFolderCard => match property {
                "row_height" => {
                    self.layout.library_folder_grid_row_height = value_as_f32(name, value)?
                }
                other => {
                    return Err(format!(
                        "{name}: unknown LibraryFolderCard layout `{other}`"
                    ))
                }
            },
            Class::LibraryRow => match property {
                "row_height" => self.layout.library_list_row_height = value_as_f32(name, value)?,
                "folder_row_height" => {
                    self.layout.library_folder_list_row_height = value_as_f32(name, value)?
                }
                "title_width" => self.layout.library_row_title_width = value_as_f32(name, value)?,
                "thumbnail_width" => {
                    self.layout.library_row_thumbnail_width = value_as_f32(name, value)?
                }
                "progress_width" => {
                    self.layout.library_row_progress_width = value_as_f32(name, value)?
                }
                other => return Err(format!("{name}: unknown LibraryRow layout `{other}`")),
            },
            Class::DragInsertionMarker => match property {
                "preview_grid_x_offset" => {
                    self.layout.library_drag_preview_grid_x_offset = value_as_f32(name, value)?
                }
                "preview_grid_y_offset" => {
                    self.layout.library_drag_preview_grid_y_offset = value_as_f32(name, value)?
                }
                "preview_list_x_offset" => {
                    self.layout.library_drag_preview_list_x_offset = value_as_f32(name, value)?
                }
                "preview_list_y_offset" => {
                    self.layout.library_drag_preview_list_y_offset = value_as_f32(name, value)?
                }
                "placeholder_content_alpha" => {
                    self.layout.library_drag_placeholder_content_alpha = value_as_f32(name, value)?
                }
                other => {
                    return Err(format!(
                        "{name}: unknown DragInsertionMarker layout `{other}`"
                    ))
                }
            },
            Class::JumpOverlay => match property {
                "input_width" => self.layout.jump_input_width = value_as_f32(name, value)?,
                other => return Err(format!("{name}: unknown JumpOverlay layout `{other}`")),
            },
            _ => return Err(format!("{name}: unknown layout property `{property}`")),
        }
        Ok(())
    }

    fn apply_app_component_node(
        &mut self,
        name: &str,
        component_name: &str,
        node: &KdlNode,
    ) -> Result<(), String> {
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: component `{component_name}` must have children"))?;
        for child in children.nodes() {
            match child.name().value() {
                "layout" => self.apply_app_component_layout_node(name, component_name, child)?,
                "labels" => self.apply_app_component_labels_node(name, component_name, child)?,
                other => {
                    return Err(format!(
                        "{name}: unsupported `{component_name}` component property `{other}`"
                    ));
                }
            }
        }
        Ok(())
    }

    fn apply_app_component_layout_node(
        &mut self,
        name: &str,
        component_name: &str,
        node: &KdlNode,
    ) -> Result<(), String> {
        for entry in node.entries() {
            let Some(property) = entry.name().map(|name| name.value()) else {
                continue;
            };
            match component_name {
                "AppWindow" => match property {
                    "width" => self.layout.window_width = value_as_f32(name, entry.value())?,
                    "height" => self.layout.window_height = value_as_f32(name, entry.value())?,
                    other => return Err(format!("{name}: unknown AppWindow layout `{other}`")),
                },
                "ViewerSidebar" => match property {
                    "width" => {
                        self.layout.viewer_sidebar_width = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown ViewerSidebar layout `{other}`")),
                },
                "LibrarySidebar" => match property {
                    "width" => {
                        self.layout.library_sidebar_width = value_as_f32(name, entry.value())?
                    }
                    "min_width" => {
                        self.layout.library_sidebar_min_width = value_as_f32(name, entry.value())?
                    }
                    "max_width" => {
                        self.layout.library_sidebar_max_width = value_as_f32(name, entry.value())?
                    }
                    "resize_handle_width" => {
                        self.layout.sidebar_resize_handle_width = value_as_f32(name, entry.value())?
                    }
                    "resize_handle_visual_width" => {
                        self.layout.sidebar_resize_handle_visual_width =
                            value_as_f32(name, entry.value())?
                    }
                    other => {
                        return Err(format!("{name}: unknown LibrarySidebar layout `{other}`"))
                    }
                },
                "LibraryVirtualization" => match property {
                    "overscan_rows" => {
                        self.layout.library_overscan_rows = value_as_usize(name, entry.value())?
                    }
                    "line_scroll_pixels" => {
                        self.layout.line_scroll_pixels = value_as_f32(name, entry.value())?
                    }
                    other => {
                        return Err(format!(
                            "{name}: unknown LibraryVirtualization layout `{other}`"
                        ))
                    }
                },
                "LibraryGrid" => match property {
                    "columns" => {
                        self.layout.card_grid_columns = value_as_usize(name, entry.value())?
                    }
                    "card_width" => {
                        self.layout.library_grid_card_width = value_as_f32(name, entry.value())?
                    }
                    "row_height" => {
                        self.layout.library_grid_row_height = value_as_f32(name, entry.value())?
                    }
                    "folder_row_height" => {
                        self.layout.library_folder_grid_row_height =
                            value_as_f32(name, entry.value())?
                    }
                    "thumbnail_width" => {
                        self.layout.library_card_thumbnail_width =
                            value_as_f32(name, entry.value())?
                    }
                    "card_title_width" => {
                        self.layout.library_card_title_width = value_as_f32(name, entry.value())?
                    }
                    "card_content_width" => {
                        self.layout.library_card_content_width = value_as_f32(name, entry.value())?
                    }
                    "card_info_height" => {
                        self.layout.library_card_info_height = value_as_f32(name, entry.value())?
                    }
                    "card_media_max_height" => {
                        self.layout.library_card_media_max_height =
                            value_as_f32(name, entry.value())?
                    }
                    "masonry_gap" => {
                        self.layout.library_masonry_gap = value_as_f32(name, entry.value())?
                    }
                    "scrollbar_gutter" => {
                        self.layout.library_scrollbar_gutter = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown LibraryGrid layout `{other}`")),
                },
                "LibraryList" => match property {
                    "row_height" => {
                        self.layout.library_list_row_height = value_as_f32(name, entry.value())?
                    }
                    "folder_row_height" => {
                        self.layout.library_folder_list_row_height =
                            value_as_f32(name, entry.value())?
                    }
                    "thumbnail_width" => {
                        self.layout.library_row_thumbnail_width = value_as_f32(name, entry.value())?
                    }
                    "progress_width" => {
                        self.layout.library_row_progress_width = value_as_f32(name, entry.value())?
                    }
                    "title_width" => {
                        self.layout.library_row_title_width = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown LibraryList layout `{other}`")),
                },
                "LibraryDrag" => match property {
                    "preview_grid_x_offset" => {
                        self.layout.library_drag_preview_grid_x_offset =
                            value_as_f32(name, entry.value())?
                    }
                    "preview_grid_y_offset" => {
                        self.layout.library_drag_preview_grid_y_offset =
                            value_as_f32(name, entry.value())?
                    }
                    "preview_list_x_offset" => {
                        self.layout.library_drag_preview_list_x_offset =
                            value_as_f32(name, entry.value())?
                    }
                    "preview_list_y_offset" => {
                        self.layout.library_drag_preview_list_y_offset =
                            value_as_f32(name, entry.value())?
                    }
                    "placeholder_content_alpha" => {
                        self.layout.library_drag_placeholder_content_alpha =
                            value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown LibraryDrag layout `{other}`")),
                },
                "SelectionToolbar" => match property {
                    "bulk_tag_input_width" => {
                        self.layout.bulk_tag_input_width = value_as_f32(name, entry.value())?
                    }
                    "bulk_tag_input_min_width" => {
                        self.layout.bulk_tag_input_min_width = value_as_f32(name, entry.value())?
                    }
                    "title_input_width" => {
                        self.layout.selection_title_input_width = value_as_f32(name, entry.value())?
                    }
                    "title_input_min_width" => {
                        self.layout.selection_title_input_min_width =
                            value_as_f32(name, entry.value())?
                    }
                    "author_input_width" => {
                        self.layout.selection_author_input_width =
                            value_as_f32(name, entry.value())?
                    }
                    "author_input_min_width" => {
                        self.layout.selection_author_input_min_width =
                            value_as_f32(name, entry.value())?
                    }
                    "context_row_height" => {
                        self.layout.selection_context_row_height =
                            value_as_f32(name, entry.value())?
                    }
                    other => {
                        return Err(format!("{name}: unknown SelectionToolbar layout `{other}`"))
                    }
                },
                "AppMenuBar" => match property {
                    "height" => {
                        self.layout.app_menu_bar_height = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown AppMenuBar layout `{other}`")),
                },
                "AppMenuPanel" => match property {
                    "width" => {
                        self.layout.app_menu_panel_width = value_as_f32(name, entry.value())?
                    }
                    "item_height" => {
                        self.layout.app_menu_item_height = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown AppMenuPanel layout `{other}`")),
                },
                "JumpOverlay" => match property {
                    "input_width" => {
                        self.layout.jump_input_width = value_as_f32(name, entry.value())?
                    }
                    other => return Err(format!("{name}: unknown JumpOverlay layout `{other}`")),
                },
                other => return Err(format!("{name}: unknown app component `{other}`")),
            }
        }
        Ok(())
    }

    fn apply_app_component_labels_node(
        &mut self,
        name: &str,
        component_name: &str,
        node: &KdlNode,
    ) -> Result<(), String> {
        let children = node.children().ok_or_else(|| {
            format!("{name}: labels block for `{component_name}` must have children")
        })?;
        for child in children.nodes() {
            let key = node_string_arg(name, child, 0)?.to_owned();
            let value = node_string_arg(name, child, 1)?.to_owned();
            match component_name {
                "AppMenu" => {
                    self.labels.app_menu.insert(key, value);
                }
                "AppMenuActions" => {
                    self.labels.app_menu_action.insert(key, value);
                }
                "SelectionToolbar" => {
                    self.labels.selection_toolbar_action.insert(key, value);
                }
                "HelpPanel" => {
                    self.labels.text.insert(key, value);
                }
                other => return Err(format!("{name}: `{other}` does not support labels")),
            }
        }
        Ok(())
    }

    fn apply_component_labels_node(
        &mut self,
        name: &str,
        class: Class,
        node: &KdlNode,
    ) -> Result<(), String> {
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: labels block for `{class:?}` must have children"))?;
        for child in children.nodes() {
            match (class, child.name().value()) {
                (Class::SidebarTab, "label") => {
                    let key = node_string_arg(name, child, 0)?.to_owned();
                    let value = node_string_arg(name, child, 1)?.to_owned();
                    self.labels.library_sidebar_tab.insert(key, value);
                }
                (_, "label") => {
                    let key = node_string_arg(name, child, 0)?.to_owned();
                    let value = node_string_arg(name, child, 1)?.to_owned();
                    self.labels.text.insert(format!("{class:?}.{key}"), value);
                }
                other => {
                    return Err(format!(
                        "{name}: unsupported label node `{}` for `{class:?}`",
                        other.1
                    ));
                }
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

    fn apply_layout_node(&mut self, name: &str, node: &KdlNode) -> Result<(), String> {
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: layout must have children"))?;
        for child in children.nodes() {
            match child.name().value() {
                "metric" => {
                    let token = node_string_arg(name, child, 0)?;
                    let value = node_f32_arg(name, child, 1)?;
                    set_layout_metric(&mut self.layout, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                "count" => {
                    let token = node_string_arg(name, child, 0)?;
                    let value = node_usize_arg(name, child, 1)?;
                    set_layout_count(&mut self.layout, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                other => return Err(format!("{name}: unsupported layout property `{other}`")),
            }
        }
        Ok(())
    }

    fn apply_labels_node(&mut self, name: &str, node: &KdlNode) -> Result<(), String> {
        let children = node
            .children()
            .ok_or_else(|| format!("{name}: labels must have children"))?;
        for child in children.nodes() {
            let section = match child.name().value() {
                "app_menu" => LabelSection::AppMenu,
                "app_menu_action" => LabelSection::AppMenuAction,
                "selection_toolbar_action" => LabelSection::SelectionToolbarAction,
                "library_sidebar_tab" => LabelSection::LibrarySidebarTab,
                "text" => LabelSection::Text,
                other => return Err(format!("{name}: unsupported label section `{other}`")),
            };
            let key = node_string_arg(name, child, 0)?.to_owned();
            let value = node_string_arg(name, child, 1)?.to_owned();
            label_map_mut(&mut self.labels, section).insert(key, value);
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
                let color = parse_color_value(name, entry.value(), tokens)?;
                style.border_color = Some(color);
                style.border = Some(merge_uniform_border_property(
                    style.border,
                    style.border_width,
                    Some(color),
                ));
            }
            "border_width" => {
                let width = value_as_f32(name, entry.value())?;
                style.border_width = Some(width);
                style.border = Some(merge_uniform_border_property(
                    style.border,
                    Some(width),
                    style.border_color,
                ));
            }
            "radius" => {
                style.radius = Some(CornerRadius::uniform(value_as_f32(name, entry.value())?));
            }
            "theme" => {}
            other => {
                return Err(format!("{name}: unsupported component property `{other}`"));
            }
        }
    }
    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                "colors" => parse_visual_colors(name, child, tokens, &mut style)?,
                "border" => parse_visual_border(name, child, tokens, &mut style)?,
                "rounding" | "radius" => {
                    style.radius = Some(parse_corner_radius(name, child, style.radius)?);
                }
                other => {
                    return Err(format!(
                        "{name}: unsupported nested visual property `{other}`"
                    ));
                }
            }
        }
    }
    Ok(style)
}

fn parse_visual_colors(
    name: &str,
    node: &KdlNode,
    tokens: &ThemeTokens,
    style: &mut VisualStyle,
) -> Result<(), String> {
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
                let color = parse_color_value(name, entry.value(), tokens)?;
                style.border_color = Some(color);
                style.border = Some(merge_uniform_border_property(
                    style.border,
                    style.border_width,
                    Some(color),
                ));
            }
            other => return Err(format!("{name}: unsupported color property `{other}`")),
        }
    }
    Ok(())
}

fn parse_visual_border(
    name: &str,
    node: &KdlNode,
    tokens: &ThemeTokens,
    style: &mut VisualStyle,
) -> Result<(), String> {
    let mut border = style
        .border
        .unwrap_or_else(|| VisualBorder::from_legacy(style.border_width, style.border_color));

    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        match property {
            "width" | "border_width" => {
                let width = value_as_f32(name, entry.value())?;
                style.border_width = Some(width);
                border = apply_border_width(border, width);
            }
            "color" | "border" | "border_color" => {
                let color = parse_color_value(name, entry.value(), tokens)?;
                style.border_color = Some(color);
                border = apply_border_color(border, color);
            }
            "radius" => {
                style.radius = Some(CornerRadius::uniform(value_as_f32(name, entry.value())?));
            }
            other => return Err(format!("{name}: unsupported border property `{other}`")),
        }
    }
    if let Some(children) = node.children() {
        for child in children.nodes() {
            let side = parse_border_side(name, child, tokens)?;
            match child.name().value() {
                "top" => border.top = border.top.merged(side),
                "right" => border.right = border.right.merged(side),
                "bottom" => border.bottom = border.bottom.merged(side),
                "left" => border.left = border.left.merged(side),
                other => return Err(format!("{name}: unsupported border side `{other}`")),
            }
        }
    }
    style.border = Some(border);
    if let Some((width, color)) = border.uniform_style() {
        style.border_width = Some(width);
        style.border_color = Some(color);
    }
    Ok(())
}

fn parse_border_side(
    name: &str,
    node: &KdlNode,
    tokens: &ThemeTokens,
) -> Result<BorderSide, String> {
    let mut side = BorderSide::EMPTY;
    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        match property {
            "width" | "border_width" => side.width = Some(value_as_f32(name, entry.value())?),
            "color" | "border" | "border_color" => {
                side.color = Some(parse_color_value(name, entry.value(), tokens)?);
            }
            other => {
                return Err(format!(
                    "{name}: unsupported border side property `{other}`"
                ))
            }
        }
    }
    Ok(side)
}

const fn merge_uniform_border_property(
    current: Option<VisualBorder>,
    width: Option<f32>,
    color: Option<Color>,
) -> VisualBorder {
    let overlay = VisualBorder::from_legacy(width, color);
    match current {
        Some(border) => border.merged(overlay),
        None => overlay,
    }
}

const fn apply_border_width(border: VisualBorder, width: f32) -> VisualBorder {
    border.merged(VisualBorder::from_legacy(Some(width), None))
}

const fn apply_border_color(border: VisualBorder, color: Color) -> VisualBorder {
    border.merged(VisualBorder::from_legacy(None, Some(color)))
}

fn parse_corner_radius(
    name: &str,
    node: &KdlNode,
    fallback: Option<CornerRadius>,
) -> Result<CornerRadius, String> {
    let mut radius = if let Some(value) = node.get("radius").or_else(|| node.get(0)) {
        CornerRadius::uniform(value_as_f32(name, value)?)
    } else {
        fallback.unwrap_or_else(|| CornerRadius::uniform(0.0))
    };

    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        let value = value_as_f32(name, entry.value())?;
        match property {
            "radius" => radius = CornerRadius::uniform(value),
            "top_left" | "top-left" => radius.top_left = value,
            "top_right" | "top-right" => radius.top_right = value,
            "bottom_right" | "bottom-right" => radius.bottom_right = value,
            "bottom_left" | "bottom-left" => radius.bottom_left = value,
            other => return Err(format!("{name}: unsupported radius property `{other}`")),
        }
    }
    Ok(radius)
}

fn parse_box_spacing(name: &str, node: &KdlNode) -> Result<BoxSpacing, String> {
    let values = node
        .entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .map(|entry| value_as_f32(name, entry.value()))
        .collect::<Result<Vec<_>, _>>()?;

    match values.as_slice() {
        [all] => Ok(BoxSpacing::uniform(*all)),
        [vertical, horizontal] => Ok(BoxSpacing::axes(*vertical, *horizontal)),
        [top, right, bottom, left] => Ok(BoxSpacing::sides(*top, *right, *bottom, *left)),
        _ => Err(format!(
            "{name}: `{}` expects 1, 2, or 4 numeric arguments",
            node.name().value()
        )),
    }
}

fn parse_component_text(name: &str, node: &KdlNode) -> Result<ComponentTextStyle, String> {
    let mut text = ComponentTextStyle::EMPTY;
    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        match property {
            "size" => text.size = Some(value_as_u32(name, entry.value())?),
            "weight" => {
                let value = entry
                    .value()
                    .as_string()
                    .ok_or_else(|| format!("{name}: expected font weight string"))?;
                text.weight = Some(parse_font_weight(name, value)?);
            }
            other => return Err(format!("{name}: unsupported text property `{other}`")),
        }
    }
    Ok(text)
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

fn node_usize_arg(name: &str, node: &KdlNode, index: usize) -> Result<usize, String> {
    let value = node.get(index).ok_or_else(|| {
        format!(
            "{name}: node `{}` missing integer argument {index}",
            node.name().value()
        )
    })?;
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    usize::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
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

fn value_as_u16(name: &str, value: &KdlValue) -> Result<u16, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    u16::try_from(*value).map_err(|_| format!("{name}: expected integer from 0 to 65535"))
}

fn value_as_usize(name: &str, value: &KdlValue) -> Result<usize, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    usize::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
}

fn value_as_u32(name: &str, value: &KdlValue) -> Result<u32, String> {
    let KdlValue::Integer(value) = value else {
        return Err(format!("{name}: expected integer value"));
    };
    u32::try_from(*value).map_err(|_| format!("{name}: expected non-negative integer"))
}

fn parse_font_weight(name: &str, value: &str) -> Result<iced::font::Weight, String> {
    Ok(match value {
        "regular" | "normal" => iced::font::Weight::Normal,
        "medium" => iced::font::Weight::Medium,
        "semibold" | "semi_bold" => iced::font::Weight::Semibold,
        "bold" => iced::font::Weight::Bold,
        other => return Err(format!("{name}: unsupported font weight `{other}`")),
    })
}

fn label_map_mut(
    labels: &mut AppLabelTokens,
    section: LabelSection,
) -> &mut HashMap<String, String> {
    match section {
        LabelSection::AppMenu => &mut labels.app_menu,
        LabelSection::AppMenuAction => &mut labels.app_menu_action,
        LabelSection::SelectionToolbarAction => &mut labels.selection_toolbar_action,
        LabelSection::LibrarySidebarTab => &mut labels.library_sidebar_tab,
        LabelSection::Text => &mut labels.text,
    }
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
        "SidebarTab" => Class::SidebarTab,
        "FileTree" => Class::FileTree,
        "FileTreeFoldButton" => Class::FileTreeFoldButton,
        "SidebarToggleButton" => Class::SidebarToggleButton,
        "SidebarDetailPanel" => Class::SidebarDetailPanel,
        "SidebarDetailRow" => Class::SidebarDetailRow,
        "SidebarActionButton" => Class::SidebarActionButton,
        "TocEntry" => Class::TocEntry,
        "LibraryCard" => Class::LibraryCard,
        "LibraryFolderCard" => Class::LibraryFolderCard,
        "LibraryRow" => Class::LibraryRow,
        "LibraryControlBar" => Class::LibraryControlBar,
        "LibrarySearchInput" => Class::LibrarySearchInput,
        "LibrarySortDropdown" => Class::LibrarySortDropdown,
        "LibraryViewToggle" => Class::LibraryViewToggle,
        "LibraryImportButton" => Class::LibraryImportButton,
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

fn set_layout_metric(tokens: &mut AppLayoutTokens, token: &str, value: f32) -> Result<(), String> {
    match token {
        "window_width" => tokens.window_width = value,
        "window_height" => tokens.window_height = value,
        "viewer_sidebar_width" => tokens.viewer_sidebar_width = value,
        "library_sidebar_width" => tokens.library_sidebar_width = value,
        "library_sidebar_min_width" => tokens.library_sidebar_min_width = value,
        "library_sidebar_max_width" => tokens.library_sidebar_max_width = value,
        "sidebar_resize_handle_width" => tokens.sidebar_resize_handle_width = value,
        "sidebar_resize_handle_visual_width" => tokens.sidebar_resize_handle_visual_width = value,
        "toolbar_height" => tokens.toolbar_height = value,
        "library_grid_card_width" => tokens.library_grid_card_width = value,
        "library_grid_row_height" => tokens.library_grid_row_height = value,
        "library_folder_grid_row_height" => tokens.library_folder_grid_row_height = value,
        "library_list_row_height" => tokens.library_list_row_height = value,
        "library_folder_list_row_height" => tokens.library_folder_list_row_height = value,
        "library_card_thumbnail_width" => tokens.library_card_thumbnail_width = value,
        "library_row_thumbnail_width" => tokens.library_row_thumbnail_width = value,
        "library_row_progress_width" => tokens.library_row_progress_width = value,
        "line_scroll_pixels" => tokens.line_scroll_pixels = value,
        "jump_input_width" => tokens.jump_input_width = value,
        "library_card_content_width" => tokens.library_card_content_width = value,
        "library_card_title_width" => tokens.library_card_title_width = value,
        "library_card_info_height" => tokens.library_card_info_height = value,
        "library_card_media_max_height" => tokens.library_card_media_max_height = value,
        "library_masonry_gap" => tokens.library_masonry_gap = value,
        "library_scrollbar_gutter" => tokens.library_scrollbar_gutter = value,
        "library_row_title_width" => tokens.library_row_title_width = value,
        "library_drag_preview_grid_x_offset" => tokens.library_drag_preview_grid_x_offset = value,
        "library_drag_preview_grid_y_offset" => tokens.library_drag_preview_grid_y_offset = value,
        "library_drag_preview_list_x_offset" => tokens.library_drag_preview_list_x_offset = value,
        "library_drag_preview_list_y_offset" => tokens.library_drag_preview_list_y_offset = value,
        "library_drag_placeholder_content_alpha" => {
            tokens.library_drag_placeholder_content_alpha = value
        }
        "bulk_tag_input_width" => tokens.bulk_tag_input_width = value,
        "bulk_tag_input_min_width" => tokens.bulk_tag_input_min_width = value,
        "selection_title_input_width" => tokens.selection_title_input_width = value,
        "selection_author_input_width" => tokens.selection_author_input_width = value,
        "selection_title_input_min_width" => tokens.selection_title_input_min_width = value,
        "selection_author_input_min_width" => tokens.selection_author_input_min_width = value,
        "app_menu_bar_height" => tokens.app_menu_bar_height = value,
        "selection_context_row_height" => tokens.selection_context_row_height = value,
        "app_menu_panel_width" => tokens.app_menu_panel_width = value,
        "app_menu_item_height" => tokens.app_menu_item_height = value,
        "sidebar_tab_height" => tokens.sidebar_tab_height = value,
        other => return Err(format!("unknown layout metric `{other}`")),
    }
    Ok(())
}

fn set_layout_count(tokens: &mut AppLayoutTokens, token: &str, value: usize) -> Result<(), String> {
    match token {
        "library_overscan_rows" => tokens.library_overscan_rows = value,
        "card_grid_columns" => tokens.card_grid_columns = value,
        other => return Err(format!("unknown layout count `{other}`")),
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
    let disk_files = style_files_in_dir(&bundled_dir);
    let mut sources = Vec::new();

    for (relative, fallback) in BUNDLED_STYLE_FILES {
        let relative_path = relative.strip_prefix("styles/").unwrap_or(relative);
        let path = bundled_dir.join(relative_path);
        if path.exists() {
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            sources.push((path.display().to_string(), source));
        } else {
            sources.push((relative.to_owned(), fallback.to_owned()));
        }
    }

    for path in disk_files {
        if bundled_style_relative_path(&bundled_dir, &path).is_some_and(|relative| {
            BUNDLED_STYLE_FILES
                .iter()
                .any(|(bundled, _)| bundled.strip_prefix("styles/").unwrap_or(bundled) == relative)
        }) {
            continue;
        }

        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        sources.push((path.display().to_string(), source));
    }

    Ok(sources)
}

fn user_style_files(dir: &Path) -> Vec<PathBuf> {
    style_files_in_dir(dir)
}

fn style_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_kdl_files(dir, &mut files);
    files.sort();
    files.sort_by_key(|path| style_file_order_key(dir, path));
    files
}

fn collect_kdl_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        {
            files.push(path.to_path_buf());
        }
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        collect_kdl_files(&entry.path(), files);
    }
}

fn style_file_order_key(root: &Path, path: &Path) -> (u8, PathBuf) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let first_component = relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str());
    let file_stem = relative.file_stem().and_then(|stem| stem.to_str());
    let group = match (first_component, file_stem) {
        (Some("themes"), _) | (_, Some("theme" | "themes")) => 0,
        (Some("components"), _) | (_, Some("component" | "components")) => 1,
        (Some("application.kdl"), _) | (_, Some("application")) => 2,
        _ => 3,
    };
    (group, relative.to_path_buf())
}

fn bundled_style_relative_path<'a>(root: &'a Path, path: &'a Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(|path| path.to_str())
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
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
        Class::LibraryControlBar,
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
                border: Some(VisualBorder::uniform(1.0, tokens.border)),
                radius: Some(CornerRadius::uniform(0.0)),
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
        Class::SidebarDetailPanel,
        Class::SidebarDetailRow,
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
                border: Some(VisualBorder::uniform(1.0, tokens.border)),
                radius: Some(CornerRadius::uniform(6.0)),
            },
        );
    }
    for class in [
        Class::ToolbarButton,
        Class::LibrarySortDropdown,
        Class::LibraryViewToggle,
        Class::LibraryImportButton,
        Class::SidebarActionButton,
        Class::MenuButton,
        Class::MenuItem,
        Class::SidebarRow,
        Class::SidebarToggleButton,
        Class::FileTreeFoldButton,
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
                border: Some(VisualBorder::uniform(1.0, tokens.border)),
                radius: Some(CornerRadius::uniform(6.0)),
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
    fn bundled_sources_include_every_bundled_kdl_file() {
        let bundled_dir = bundled_style_dir();
        let sources = bundled_style_sources().expect("bundled sources should load");
        let source_names = sources
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();

        for path in style_files_in_dir(&bundled_dir) {
            let source_name = path.display().to_string();
            assert!(
                source_names.contains(&source_name.as_str()),
                "{} should be included in bundled style sources",
                path.display()
            );
        }
    }

    #[test]
    fn bundled_file_tree_active_border_uses_side_widths() {
        let style_book = StyleBook::bundled();

        let espresso = style_book.tokens("espresso").class_styles[Class::FileTree.index()]
            .resolve(ComponentState::Active)
            .border
            .expect("active espresso file tree border should be set");
        assert_eq!(espresso.left.width, Some(3.0));
        assert!(espresso.uniform_style().is_none());

        let light = style_book.tokens("light").class_styles[Class::FileTree.index()]
            .resolve(ComponentState::Active)
            .border
            .expect("active light file tree border should be set");
        assert_eq!(light.left.width, Some(20.0));
        assert!(light.uniform_style().is_none());
    }

    #[test]
    fn user_style_files_include_nested_kdl_files() {
        let root =
            std::env::temp_dir().join(format!("pdf-folio-style-test-{}", std::process::id()));
        let nested = root.join("components").join("library");
        std::fs::create_dir_all(&nested).expect("nested test style dir should be created");
        let top_level = root.join("theme.kdl");
        let nested_file = nested.join("sidebar.kdl");
        std::fs::write(&top_level, "").expect("top-level test style should be written");
        std::fs::write(&nested_file, "").expect("nested test style should be written");

        let files = user_style_files(&root);

        assert!(files.contains(&top_level));
        assert!(files.contains(&nested_file));
        assert!(
            files.iter().position(|path| path == &top_level)
                < files.iter().position(|path| path == &nested_file),
            "theme overrides should be loaded before component overrides"
        );

        std::fs::remove_dir_all(&root).expect("test style dir should be removed");
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

    #[test]
    fn border_shorthand_applies_to_all_sides() {
        let style_book = StyleBook::from_sources(
            vec![(
                "style.kdl".to_owned(),
                r##"
                theme "espresso" {}
                theme "light" {}
                component "Toolbar" {
                    normal {
                        border width=0 color="#00000000"
                    }
                }
                "##
                .to_owned(),
            )],
            Vec::new(),
        )
        .expect("style book should compile");

        let style = style_book.tokens("espresso").class_styles[Class::Toolbar.index()]
            .resolve(ComponentState::Normal);
        let border = style.border.expect("border should be set");
        assert_eq!(border.uniform_style(), Some((0.0, Color::TRANSPARENT)));
    }

    #[test]
    fn border_sides_can_override_width_and_color_independently() {
        let style_book = StyleBook::from_sources(
            vec![(
                "style.kdl".to_owned(),
                r##"
                theme "espresso" {}
                theme "light" {}
                component "Toolbar" {
                    normal {
                        border width=1 color="#111111" {
                            top width=2 color="#222222"
                            right width=3 color="#333333"
                            bottom width=4 color="#444444"
                            left width=0 color="#00000000"
                        }
                    }
                }
                "##
                .to_owned(),
            )],
            Vec::new(),
        )
        .expect("style book should compile");

        let style = style_book.tokens("espresso").class_styles[Class::Toolbar.index()]
            .resolve(ComponentState::Normal);
        let border = style.border.expect("border should be set");
        assert_eq!(border.top.width, Some(2.0));
        assert_eq!(border.top.color, Some(Color::from_rgb8(0x22, 0x22, 0x22)));
        assert_eq!(border.right.width, Some(3.0));
        assert_eq!(border.right.color, Some(Color::from_rgb8(0x33, 0x33, 0x33)));
        assert_eq!(border.bottom.width, Some(4.0));
        assert_eq!(
            border.bottom.color,
            Some(Color::from_rgb8(0x44, 0x44, 0x44))
        );
        assert_eq!(border.left.width, Some(0.0));
        assert_eq!(border.left.color, Some(Color::TRANSPARENT));
    }
}
