//! KDL-backed style book: load, merge, and resolve themes for the UI.
//!
//! [`StyleBook`] is the single entry point the shell holds at runtime
//! (`Arc<StyleBook>`). It owns:
//!
//! - **Themes** — named palettes (`espresso`, `light`) compiled into
//!   [`ThemeTokens`](crate::ThemeTokens).
//! - **Layout** — window size, sidebar widths, card metrics, scroll increments
//!   from `application.kdl` and component `layout { … }` blocks.
//! - **Labels** — user-facing strings for menus and chrome that ship in KDL so
//!   they can be adjusted without a code change.
//! - **Style directories** — paths the UI watches for hot reload.
//!
//! # Loading
//!
//! | Constructor | Behavior |
//! | --- | --- |
//! | [`StyleBook::load`] | Bundled KDL + on-disk checkout styles + XDG user overrides |
//! | [`StyleBook::bundled`] | Embedded sources only; panics if invalid |
//! | [`StyleBook::from_sources`] | Explicit `(name, kdl)` list (tests / tooling) |
//!
//! User overrides live at `$XDG_CONFIG_HOME/pdf-folio/styles/**/*.kdl` (or
//! `~/.config/pdf-folio/styles/`). Later sources overwrite earlier ones when
//! the same theme/component/state is redefined.
//!
//! # Top-level KDL nodes
//!
//! Each style file may contain:
//!
//! - `theme "espresso" { color "background" "#…" … }`
//! - `component "LibraryCard" { normal background=…; hovered …; layout width=… }`
//! - `primitive "page_shadow_offset_x" 4` (or color primitives for find fill)
//! - `layout { metric "window_width" 960; count "card_grid_columns" 2 }`
//! - `labels { text "empty_library" "No documents yet" }`
//!
//! Component states: `normal`, `hovered`, `pressed`, `focused`, `disabled`,
//! `selected`, `active`, `error`. Optional `theme="espresso"` on a state scopes
//! that override to one palette.
//!
//! Color values accept `#RRGGBB`, `#RRGGBBAA`, `rgba(r,g,b,a)`, token refs
//! (`$accent`), and blends (`mix($surface, $accent, 0.16)`).
//!
//! # Hot reload
//!
//! The shell reloads via [`StyleBook::load`] when the user triggers
//! **View → Reload Styles**, the reload shortcut, or a filesystem notification
//! on [`StyleBook::style_dirs`]. On parse failure the previous book stays
//! active and the error is reported in the UI.
//!
//! Internal submodules: `parser` (color/value helpers) and `sources` (bundled
//! file table, XDG paths, directory walk order).

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use iced::Color;
use kdl::{KdlDocument, KdlNode, KdlValue};

use crate::classes::{mix_color, Class, ComponentState};
use crate::tokens::{
    AppLabelTokens, AppLayoutTokens, BorderSide, BoxShadow, BoxSpacing, ClassStyle, ClassStylesRef,
    ComponentLayout, ComponentTextStyle, CornerRadius, LabelSection, PrimitiveTokens, ThemeTokens,
    VisualBorder, VisualStyle,
};

/// KDL value helpers (colors, numbers, font weights, node arguments).
mod parser;
/// Bundled/user style path resolution and directory walk order.
mod sources;

use parser::*;
#[cfg(test)]
use sources::{bundled_style_dir, style_files_in_dir};
use sources::{
    bundled_style_sources, style_source_dirs, user_style_dir, user_style_files, BUNDLED_STYLE_FILES,
};

/// Parsed and validated style data ready for the UI appearance runtime.
///
/// Holds resolved theme palettes, layout metrics, chrome labels, and the list
/// of directories the shell should watch for style file changes.
#[derive(Debug, Clone)]
pub struct StyleBook {
    themes: HashMap<String, ThemeTokens>,
    layout: AppLayoutTokens,
    labels: AppLabelTokens,
    style_dirs: Vec<PathBuf>,
}

impl StyleBook {
    /// Loads bundled styles, prefers on-disk checkout files when present, then
    /// layers any user KDL under the XDG config `styles/` directory.
    ///
    /// Returns an error string describing the first parse/validation failure.
    /// Callers that hot-reload should keep the previous book on error.
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

    /// Loads only the embedded `include_str!` sources (no disk / XDG).
    ///
    /// Panics if the compiled-in styles are invalid — they must always parse.
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

    /// Builds a style book from named KDL source strings.
    ///
    /// `sources` is an ordered list of `(display_name, kdl_text)`. Later entries
    /// override earlier ones for the same theme, component, or layout key.
    /// `style_dirs` is stored for hot-reload watchers and is not read here.
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

    /// Returns resolved tokens for a theme id (`"espresso"`, `"light"`, …).
    ///
    /// Falls back to the `espresso` palette, then to
    /// [`fallback_dark_tokens`] if even that is missing.
    pub fn tokens(&self, theme: &str) -> ThemeTokens {
        self.themes
            .get(theme)
            .copied()
            .or_else(|| self.themes.get("espresso").copied())
            .unwrap_or_else(fallback_dark_tokens)
    }

    /// KDL-backed layout metrics (window size, sidebars, card grid, …).
    pub fn layout(&self) -> &AppLayoutTokens {
        &self.layout
    }

    /// KDL-backed chrome labels (menus, selection toolbar, empty states).
    pub fn labels(&self) -> &AppLabelTokens {
        &self.labels
    }

    /// Directories the shell should watch for `.kdl` changes during hot reload.
    ///
    /// Typically the checkout `styles/` tree and the user XDG styles dir when
    /// they exist on disk.
    pub fn style_dirs(&self) -> &[PathBuf] {
        &self.style_dirs
    }
}

/// Mutable accumulator while merging ordered KDL sources into a book.
///
/// Holds partially built themes plus global layout/labels before
/// [`RawStyleBook::compile`] validates required themes and freezes tokens.
#[derive(Debug, Default)]
struct RawStyleBook {
    /// Named theme palettes accumulated from `theme "…"` nodes.
    themes: HashMap<String, RawTheme>,
    /// Global layout metrics from top-level `layout { … }` nodes.
    layout: AppLayoutTokens,
    /// Chrome label maps from top-level `labels { … }` nodes.
    labels: AppLabelTokens,
}

/// One theme during parse: palette tokens plus a mutable working class-style table.
///
/// Class styles are interned into a [`ClassStylesRef`] only at [`RawStyleBook::compile`]
/// so KDL load can mutate styles without copying a 140 KiB table on every pass.
#[derive(Debug, Clone)]
struct RawTheme {
    /// Resolved palette and primitives for this theme id.
    tokens: ThemeTokens,
    /// Working class-style table (interned when the book is finalized).
    class_styles: Box<[ClassStyle; Class::COUNT]>,
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
        let mut raw_theme = match theme_name {
            "light" => raw_theme_from_fallback(fallback_light_tokens()),
            "espresso" | "dark" => raw_theme_from_fallback(fallback_dark_tokens()),
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
                    set_theme_color(&mut raw_theme.tokens, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                "primitive" => {
                    let token = node_string_arg(name, child, 0)?;
                    let value = node_f32_arg(name, child, 1)?;
                    set_primitive(&mut raw_theme.tokens.primitives, token, value)
                        .map_err(|error| format!("{name}: {error}"))?;
                }
                other => {
                    return Err(format!(
                        "{name}: unsupported theme property `{other}` in `{theme_name}`"
                    ));
                }
            }
        }
        self.themes.insert(theme_name.to_owned(), raw_theme);
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
                        let current = raw_theme.class_styles[class.index()].layout;
                        raw_theme.class_styles[class.index()].layout = current.merged(layout);
                    }
                }
                "text" => {
                    let text = parse_component_text(name, child)?;
                    for raw_theme in self.themes.values_mut() {
                        let current = raw_theme.class_styles[class.index()].text;
                        raw_theme.class_styles[class.index()].text = current.merged(text);
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
                        raw_theme.class_styles[class.index()].states[state.index()] =
                            raw_theme.class_styles[class.index()].states[state.index()]
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
            Class::ViewerToolbar => match property {
                "title_min_width" => {
                    self.layout.viewer_toolbar_title_min_width = value_as_f32(name, value)?
                }
                "title_max_width" => {
                    self.layout.viewer_toolbar_title_max_width = value_as_f32(name, value)?
                }
                "selection_width" => {
                    self.layout.viewer_toolbar_selection_width = value_as_f32(name, value)?
                }
                other => return Err(format!("{name}: unknown ViewerToolbar layout `{other}`")),
            },
            Class::ViewerPageControl => match property {
                "number_width" => self.layout.viewer_page_number_width = value_as_f32(name, value)?,
                "control_width" => {
                    self.layout.viewer_page_control_width = value_as_f32(name, value)?
                }
                "chevron_size" => self.layout.viewer_page_chevron_size = value_as_f32(name, value)?,
                other => {
                    return Err(format!(
                        "{name}: unknown ViewerPageControl layout `{other}`"
                    ))
                }
            },
            Class::ViewerZoomControl => match property {
                "control_width" => {
                    self.layout.viewer_zoom_control_width = value_as_f32(name, value)?
                }
                "menu_width" => self.layout.viewer_zoom_menu_width = value_as_f32(name, value)?,
                "menu_row_height" => {
                    self.layout.viewer_zoom_menu_row_height = value_as_f32(name, value)?
                }
                other => {
                    self.apply_generic_app_layout_property(name, "ViewerZoomControl", other, value)?
                }
            },
            Class::ViewerSidebar => match property {
                "width" => self.layout.viewer_sidebar_width = value_as_f32(name, value)?,
                other => return Err(format!("{name}: unknown ViewerSidebar layout `{other}`")),
            },
            Class::ViewerThumbnail => match property {
                "width_px" => self.layout.viewer_thumbnail_width_px = value_as_u16(name, value)?,
                other => return Err(format!("{name}: unknown ViewerThumbnail layout `{other}`")),
            },
            Class::ViewerFindBar => match property {
                "width" => self.layout.viewer_find_bar_width = value_as_f32(name, value)?,
                "height" => self.layout.viewer_find_bar_height = value_as_f32(name, value)?,
                other => {
                    self.apply_generic_app_layout_property(name, "ViewerFindBar", other, value)?
                }
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
            Class::LibrarySearchInput => match property {
                "clear_icon_size" | "clear_button_size" | "clear_button_padding" => self
                    .apply_generic_app_layout_property(
                        name,
                        "LibrarySearchInput",
                        property,
                        value,
                    )?,
                other => {
                    return Err(format!(
                        "{name}: unknown LibrarySearchInput layout `{other}`"
                    ))
                }
            },
            Class::MenuPanel => {
                self.apply_generic_app_layout_property(name, "MenuPanel", property, value)?
            }
            Class::ContextMenuPanel => match property {
                "width" => self.layout.context_menu_panel_width = value_as_f32(name, value)?,
                "item_height" => self.layout.context_menu_item_height = value_as_f32(name, value)?,
                other => {
                    self.apply_generic_app_layout_property(name, "ContextMenuPanel", other, value)?
                }
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
                    "row_spacing"
                    | "row_padding_x"
                    | "row_padding_y"
                    | "folder_row_padding_y"
                    | "dropdown_base_x"
                    | "single_dropdown_extra_x"
                    | "folders_dropdown_offset_x"
                    | "metadata_dropdown_offset_x"
                    | "maintenance_dropdown_offset_x"
                    | "icon_size"
                    | "icon_slot_size"
                    | "tooltip_delay_ms" => self.apply_generic_app_layout_property(
                        name,
                        "SelectionToolbar",
                        property,
                        entry.value(),
                    )?,
                    other => {
                        return Err(format!("{name}: unknown SelectionToolbar layout `{other}`"))
                    }
                },
                "AppMenuBar" => match property {
                    "height" => {
                        self.layout.app_menu_bar_height = value_as_f32(name, entry.value())?
                    }
                    "file_width" => {
                        self.layout.app_menu_file_width = value_as_f32(name, entry.value())?
                    }
                    "edit_width" => {
                        self.layout.app_menu_edit_width = value_as_f32(name, entry.value())?
                    }
                    "view_width" => {
                        self.layout.app_menu_view_width = value_as_f32(name, entry.value())?
                    }
                    "document_width" => {
                        self.layout.app_menu_document_width = value_as_f32(name, entry.value())?
                    }
                    "library_width" => {
                        self.layout.app_menu_library_width = value_as_f32(name, entry.value())?
                    }
                    "tools_width" => {
                        self.layout.app_menu_tools_width = value_as_f32(name, entry.value())?
                    }
                    "help_width" => {
                        self.layout.app_menu_help_width = value_as_f32(name, entry.value())?
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
                    other => self.apply_generic_app_layout_property(
                        name,
                        component_name,
                        other,
                        entry.value(),
                    )?,
                },
                other => {
                    self.apply_generic_app_layout_property(name, other, property, entry.value())?
                }
            }
        }
        Ok(())
    }

    fn apply_generic_app_layout_property(
        &mut self,
        name: &str,
        component_name: &str,
        property: &str,
        value: &KdlValue,
    ) -> Result<(), String> {
        match value {
            KdlValue::Integer(value) => {
                if let Ok(count) = usize::try_from(*value) {
                    self.layout.set_count(component_name, property, count);
                }
                self.layout
                    .set_metric(component_name, property, *value as f32);
                Ok(())
            }
            KdlValue::Float(value) => {
                self.layout
                    .set_metric(component_name, property, *value as f32);
                Ok(())
            }
            _ => Err(format!(
                "{name}: generic layout `{component_name}.{property}` must be numeric"
            )),
        }
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
        if matches!(
            primitive,
            "viewer_find_fill"
                | "viewer_find_selected_fill"
                | "viewer_annotation_fill"
                | "viewer_annotation_selected_fill"
        ) {
            let value = parse_color_literal(node_string_arg(name, node, 1)?)
                .map_err(|error| format!("{name}: primitive `{primitive}`: {error}"))?;
            for raw_theme in self.themes.values_mut() {
                set_primitive_color(&mut raw_theme.tokens.primitives, primitive, value)
                    .map_err(|error| format!("{name}: {error}"))?;
            }
        } else {
            let value = node_f32_arg(name, node, 1)?;
            for raw_theme in self.themes.values_mut() {
                set_primitive(&mut raw_theme.tokens.primitives, primitive, value)
                    .map_err(|error| format!("{name}: {error}"))?;
            }
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
            .map(|(name, mut raw)| {
                // Move working table into the intern registry; ThemeTokens stays Copy/small.
                let styles = std::mem::replace(
                    &mut raw.class_styles,
                    Box::new([ClassStyle::EMPTY; Class::COUNT]),
                );
                raw.tokens.class_styles = ClassStylesRef::intern(*styles);
                (name, raw.tokens)
            })
            .collect())
    }
}

/// Parses a component-state KDL node into a [`VisualStyle`].
///
/// Reads flat properties (`background`, `text`, `border`, `border_width`,
/// `radius`) and nested children (`colors`, `border`, `rounding`/`radius`,
/// `shadow`). The `theme` property is ignored here (scoping is handled by the
/// caller). `name` is the source path used in error messages.
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
                "shadow" => {
                    style.shadow = Some(parse_box_shadow(name, child, tokens)?);
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

/// Parses a nested `colors { … }` block: `background`, `text`/`text_color`,
/// `border`/`border_color` (each a color expression against `tokens`).
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

/// Parses a nested `border { … }` block into [`VisualBorder`] / legacy fields.
///
/// Flat props: `width`/`border_width`, `color`/`border`/`border_color`, `radius`.
/// Child nodes `top`/`right`/`bottom`/`left` set per-side width/color.
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

/// Parses one border side node (`top`/`right`/`bottom`/`left`): `width` and
/// `color` (or `border_width` / `border` / `border_color` aliases).
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

/// Merges a uniform width/color overlay onto an existing [`VisualBorder`], or
/// builds one from the legacy pair when none is present.
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

/// Sets all sides' width on `border` via a uniform legacy merge.
const fn apply_border_width(border: VisualBorder, width: f32) -> VisualBorder {
    border.merged(VisualBorder::from_legacy(Some(width), None))
}

/// Sets all sides' color on `border` via a uniform legacy merge.
const fn apply_border_color(border: VisualBorder, color: Color) -> VisualBorder {
    border.merged(VisualBorder::from_legacy(None, Some(color)))
}

/// Parses a `rounding`/`radius` node: uniform `radius` (or positional arg) plus
/// optional `top_left`/`top_right`/`bottom_right`/`bottom_left` overrides.
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

/// Parses a nested `shadow { … }` node: `offset_x`/`x`, `offset_y`/`y`,
/// `blur_radius`/`blur`, and `color` (defaults to `$shadow`).
fn parse_box_shadow(name: &str, node: &KdlNode, tokens: &ThemeTokens) -> Result<BoxShadow, String> {
    let mut shadow = BoxShadow {
        offset_x: 0.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        color: tokens.shadow,
    };
    for entry in node.entries() {
        let Some(property) = entry.name().map(|name| name.value()) else {
            continue;
        };
        match property {
            "offset_x" | "x" => shadow.offset_x = value_as_f32(name, entry.value())?,
            "offset_y" | "y" => shadow.offset_y = value_as_f32(name, entry.value())?,
            "blur_radius" | "blur" => shadow.blur_radius = value_as_f32(name, entry.value())?,
            "color" => shadow.color = parse_color_value(name, entry.value(), tokens)?,
            other => return Err(format!("{name}: unsupported shadow property `{other}`")),
        }
    }
    Ok(shadow)
}

/// Parses layout spacing from 1, 2, or 4 positional numeric args (CSS-like
/// uniform / axes / sides) on nodes such as `padding` or `margin`.
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

/// Parses a nested `text { … }` node: `size` (u32) and `weight` (keyword string).
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

/// Mutable map for one [`LabelSection`] inside [`AppLabelTokens`].
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

/// Maps a KDL component name string onto a [`Class`] variant (`"LibraryCard"`, …).
fn parse_class(value: &str) -> Option<Class> {
    Some(match value {
        "AppShell" => Class::AppShell,
        "Toolbar" => Class::Toolbar,
        "MenuBar" => Class::MenuBar,
        "MenuButton" => Class::MenuButton,
        "MenuPanel" => Class::MenuPanel,
        "MenuItem" => Class::MenuItem,
        "ContextMenuPanel" => Class::ContextMenuPanel,
        "ContextMenuItem" => Class::ContextMenuItem,
        "SelectionRestoreButton" => Class::SelectionRestoreButton,
        "SelectionDangerButton" => Class::SelectionDangerButton,
        "SelectionDangerIconButton" => Class::SelectionDangerIconButton,
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
        "SidebarFolderCard" => Class::SidebarFolderCard,
        "SidebarFolderCardTitle" => Class::SidebarFolderCardTitle,
        "SidebarFolderTextInput" => Class::SidebarFolderTextInput,
        "SidebarFolderActionButton" => Class::SidebarFolderActionButton,
        "TocEntry" => Class::TocEntry,
        "LibraryCard" => Class::LibraryCard,
        "LibraryFolderCard" => Class::LibraryFolderCard,
        "LibraryRow" => Class::LibraryRow,
        "LibraryControlBar" => Class::LibraryControlBar,
        "LibrarySearchInput" => Class::LibrarySearchInput,
        "LibrarySortDropdown" => Class::LibrarySortDropdown,
        "LibraryViewToggle" => Class::LibraryViewToggle,
        "LibraryImportButton" => Class::LibraryImportButton,
        "LibraryGridZoomSlider" => Class::LibraryGridZoomSlider,
        "TagPill" => Class::TagPill,
        "SearchInput" => Class::SearchInput,
        "ProgressBar" => Class::ProgressBar,
        "ErrorBanner" => Class::ErrorBanner,
        "ViewerCanvas" => Class::ViewerCanvas,
        "ViewerToolbar" => Class::ViewerToolbar,
        "ViewerToolbarButton" => Class::ViewerToolbarButton,
        "ViewerToolbarTitle" => Class::ViewerToolbarTitle,
        "ViewerPageControl" => Class::ViewerPageControl,
        "ViewerZoomControl" => Class::ViewerZoomControl,
        "ViewerZoomMenu" => Class::ViewerZoomMenu,
        "ViewerZoomMenuItem" => Class::ViewerZoomMenuItem,
        "ViewerSidebar" => Class::ViewerSidebar,
        "ViewerSidebarTab" => Class::ViewerSidebarTab,
        "ViewerOutlineEntry" => Class::ViewerOutlineEntry,
        "ViewerThumbnail" => Class::ViewerThumbnail,
        "ViewerFindBar" => Class::ViewerFindBar,
        "ViewerFindInput" => Class::ViewerFindInput,
        "ViewerFindButton" => Class::ViewerFindButton,
        "ViewerPagePlaceholder" => Class::ViewerPagePlaceholder,
        "PagePlaceholder" => Class::PagePlaceholder,
        "JumpOverlay" => Class::JumpOverlay,
        "Tooltip" => Class::Tooltip,
        "AnnotationToolbar" => Class::AnnotationToolbar,
        "AnnotationPopover" => Class::AnnotationPopover,
        "ViewerPresentationOverlay" => Class::ViewerPresentationOverlay,
        "PresentationOverlay" => Class::PresentationOverlay,
        "Minimap" => Class::Minimap,
        "EmptyState" => Class::EmptyState,
        "DragInsertionMarker" => Class::DragInsertionMarker,
        "SelectionCheckbox" => Class::SelectionCheckbox,
        "MasterCheckbox" => Class::MasterCheckbox,
        "DragStackGhost" => Class::DragStackGhost,
        "FolderDropTarget" => Class::FolderDropTarget,
        _ => return None,
    })
}

/// Maps a KDL state node name onto a [`ComponentState`] (`normal`, `hovered`, …).
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

/// Writes a `color "token" "…"` value into the matching [`ThemeTokens`] field.
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

/// Writes a numeric `primitive "token" value` into [`PrimitiveTokens`].
fn set_primitive(tokens: &mut PrimitiveTokens, token: &str, value: f32) -> Result<(), String> {
    match token {
        "page_shadow_offset_x" => tokens.page_shadow_offset_x = value,
        "page_shadow_offset_y" => tokens.page_shadow_offset_y = value,
        "viewer_text_selection_mix" => tokens.viewer_text_selection_mix = value,
        "viewer_text_selection_alpha" => tokens.viewer_text_selection_alpha = value,
        "progress_girth" => tokens.progress_girth = value,
        "slider_rail_width" => tokens.slider_rail_width = value,
        "slider_handle_radius" => tokens.slider_handle_radius = value,
        "scrollbar_width" => tokens.scrollbar_width = value,
        "scrollbar_scroller_width" => tokens.scrollbar_scroller_width = value,
        "sidebar_scrollbar_width" => tokens.sidebar_scrollbar_width = value,
        "sidebar_scrollbar_scroller_width" => tokens.sidebar_scrollbar_scroller_width = value,
        "scrollbar_radius" => tokens.scrollbar_radius = value,
        "auto_scroll_radius" => tokens.auto_scroll_radius = value,
        "auto_scroll_shadow_blur" => tokens.auto_scroll_shadow_blur = value,
        "document_preview_line_spacing" => tokens.document_preview_line_spacing = value,
        "document_preview_min_line_width" => tokens.document_preview_min_line_width = value,
        "document_preview_heading_line_height" => {
            tokens.document_preview_heading_line_height = value
        }
        "document_preview_body_line_height" => tokens.document_preview_body_line_height = value,
        "document_preview_line_radius" => tokens.document_preview_line_radius = value,
        "flush_media_background_mix" => tokens.flush_media_background_mix = value,
        "library_view_toggle_icon_size" => tokens.library_view_toggle_icon_size = value,
        "library_grid_zoom_label_width" => tokens.library_grid_zoom_label_width = value,
        "library_metadata_picker_width" => tokens.library_metadata_picker_width = value,
        "library_sort_menu_height" => tokens.library_sort_menu_height = value,
        "folder_parent_icon_width" => tokens.folder_parent_icon_width = value,
        "folder_parent_icon_height" => tokens.folder_parent_icon_height = value,
        "folder_icon_size" => tokens.folder_icon_size = value,
        "folder_icon_container_width" => tokens.folder_icon_container_width = value,
        "folder_icon_container_height" => tokens.folder_icon_container_height = value,
        "folder_icon_background_mix" => tokens.folder_icon_background_mix = value,
        "library_switcher_sidebar_icon_size" => tokens.library_switcher_sidebar_icon_size = value,
        "library_switcher_sidebar_icon_slot" => tokens.library_switcher_sidebar_icon_slot = value,
        "library_switcher_sidebar_button_height" => {
            tokens.library_switcher_sidebar_button_height = value
        }
        "library_switcher_sidebar_text_width" => tokens.library_switcher_sidebar_text_width = value,
        "sidebar_chevron_icon_size" => tokens.sidebar_chevron_icon_size = value,
        "sidebar_chevron_button_size" => tokens.sidebar_chevron_button_size = value,
        "sidebar_chevron_button_padding" => tokens.sidebar_chevron_button_padding = value,
        "file_tree_indent_width" => tokens.file_tree_indent_width = value,
        "file_tree_max_indent" => tokens.file_tree_max_indent = value,
        "file_tree_meta_char_width" => tokens.file_tree_meta_char_width = value,
        "file_tree_meta_min_width" => tokens.file_tree_meta_min_width = value,
        "file_tree_meta_max_width" => tokens.file_tree_meta_max_width = value,
        "file_tree_row_padding_y" => tokens.file_tree_row_padding_y = value,
        "raindrop_tree_indent_width" => tokens.raindrop_tree_indent_width = value,
        "raindrop_tree_max_indent" => tokens.raindrop_tree_max_indent = value,
        "raindrop_tree_fold_width" => tokens.raindrop_tree_fold_width = value,
        "raindrop_tree_row_padding_y" => tokens.raindrop_tree_row_padding_y = value,
        "raindrop_new_folder_icon_size" => tokens.raindrop_new_folder_icon_size = value,
        "menu_separator_height" => tokens.menu_separator_height = value,
        "context_menu_separator_height" => tokens.context_menu_separator_height = value,
        "selection_menu_button_height" => tokens.selection_menu_button_height = value,
        other => return Err(format!("unknown primitive `{other}`")),
    }
    Ok(())
}

/// Writes a color primitive (`viewer_find_fill`, annotation fills, …).
fn set_primitive_color(
    tokens: &mut PrimitiveTokens,
    token: &str,
    value: Color,
) -> Result<(), String> {
    match token {
        "viewer_find_fill" => tokens.viewer_find_fill = value,
        "viewer_find_selected_fill" => tokens.viewer_find_selected_fill = value,
        "viewer_annotation_fill" => tokens.viewer_annotation_fill = value,
        "viewer_annotation_selected_fill" => tokens.viewer_annotation_selected_fill = value,
        other => return Err(format!("unknown primitive color `{other}`")),
    }
    Ok(())
}

/// Writes a `metric "token" value` layout number into [`AppLayoutTokens`].
fn set_layout_metric(tokens: &mut AppLayoutTokens, token: &str, value: f32) -> Result<(), String> {
    match token {
        "window_width" => tokens.window_width = value,
        "window_height" => tokens.window_height = value,
        "viewer_sidebar_width" => tokens.viewer_sidebar_width = value,
        "viewer_toolbar_title_min_width" => tokens.viewer_toolbar_title_min_width = value,
        "viewer_toolbar_title_max_width" => tokens.viewer_toolbar_title_max_width = value,
        "viewer_toolbar_selection_width" => tokens.viewer_toolbar_selection_width = value,
        "viewer_find_bar_width" => tokens.viewer_find_bar_width = value,
        "viewer_find_bar_height" => tokens.viewer_find_bar_height = value,
        "viewer_page_number_width" => tokens.viewer_page_number_width = value,
        "viewer_page_control_width" => tokens.viewer_page_control_width = value,
        "viewer_page_chevron_size" => tokens.viewer_page_chevron_size = value,
        "viewer_zoom_control_width" => tokens.viewer_zoom_control_width = value,
        "viewer_zoom_menu_width" => tokens.viewer_zoom_menu_width = value,
        "viewer_zoom_menu_row_height" => tokens.viewer_zoom_menu_row_height = value,
        "viewer_thumbnail_width_px" => tokens.viewer_thumbnail_width_px = value.round() as u16,
        "viewer_page_fade_ms" => tokens.viewer_page_fade_ms = value.round() as u64,
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
        "app_menu_file_width" => tokens.app_menu_file_width = value,
        "app_menu_edit_width" => tokens.app_menu_edit_width = value,
        "app_menu_view_width" => tokens.app_menu_view_width = value,
        "app_menu_document_width" => tokens.app_menu_document_width = value,
        "app_menu_library_width" => tokens.app_menu_library_width = value,
        "app_menu_tools_width" => tokens.app_menu_tools_width = value,
        "app_menu_help_width" => tokens.app_menu_help_width = value,
        "selection_context_row_height" => tokens.selection_context_row_height = value,
        "app_menu_panel_width" => tokens.app_menu_panel_width = value,
        "app_menu_item_height" => tokens.app_menu_item_height = value,
        "context_menu_panel_width" => tokens.context_menu_panel_width = value,
        "context_menu_item_height" => tokens.context_menu_item_height = value,
        "sidebar_tab_height" => tokens.sidebar_tab_height = value,
        other => return Err(format!("unknown layout metric `{other}`")),
    }
    Ok(())
}

/// Writes a `count "token" value` layout integer into [`AppLayoutTokens`].
fn set_layout_count(tokens: &mut AppLayoutTokens, token: &str, value: usize) -> Result<(), String> {
    match token {
        "library_overscan_rows" => tokens.library_overscan_rows = value,
        "card_grid_columns" => tokens.card_grid_columns = value,
        other => return Err(format!("unknown layout count `{other}`")),
    }
    Ok(())
}

/// Built-in dark (espresso-like) palette used before styles load or as a last
/// resort when the style book cannot provide an `espresso` theme.
pub fn fallback_dark_tokens() -> ThemeTokens {
    finalize_fallback_tokens(raw_theme_from_palette(
        Color::from_rgb8(16, 12, 7),
        Color::from_rgb8(23, 17, 10),
        Color::from_rgb8(36, 26, 16),
        Color::from_rgb8(240, 230, 212),
        Color::from_rgb8(168, 144, 114),
        Color::from_rgb8(224, 180, 90),
        Color::from_rgba8(200, 184, 154, 0.22),
        Color::from_rgb8(232, 90, 90),
        Color::from_rgb8(12, 9, 5),
        Color::from_rgb8(46, 34, 20),
        Color::from_rgb8(224, 180, 90),
        Color::from_rgba8(0, 0, 0, 0.65),
    ))
}

/// Built-in light palette used before styles load or as a last resort when the
/// style book cannot provide a `light` theme.
pub fn fallback_light_tokens() -> ThemeTokens {
    finalize_fallback_tokens(raw_theme_from_palette(
        Color::from_rgb8(237, 233, 225),
        Color::from_rgb8(251, 249, 244),
        Color::from_rgb8(255, 255, 255),
        Color::from_rgb8(31, 24, 16),
        Color::from_rgb8(110, 90, 66),
        Color::from_rgb8(166, 124, 46),
        Color::from_rgb8(212, 203, 184),
        Color::from_rgb8(176, 48, 64),
        Color::from_rgb8(228, 221, 210),
        Color::from_rgb8(232, 224, 210),
        Color::from_rgb8(166, 124, 46),
        Color::from_rgba8(31, 24, 16, 0.16),
    ))
}

fn raw_theme_from_palette(
    background: Color,
    surface: Color,
    surface_raised: Color,
    text_primary: Color,
    text_secondary: Color,
    accent: Color,
    border: Color,
    error: Color,
    canvas: Color,
    placeholder: Color,
    focus: Color,
    shadow: Color,
) -> RawTheme {
    let tokens = ThemeTokens {
        background,
        surface,
        surface_raised,
        text_primary,
        text_secondary,
        accent,
        border,
        error,
        canvas,
        placeholder,
        focus,
        shadow,
        class_styles: ClassStylesRef::empty(),
        primitives: PrimitiveTokens::default(),
    };
    let mut class_styles = Box::new([ClassStyle::EMPTY; Class::COUNT]);
    apply_fallback_class_styles(&mut class_styles, &tokens);
    RawTheme {
        tokens,
        class_styles,
    }
}

fn raw_theme_from_fallback(tokens: ThemeTokens) -> RawTheme {
    // Re-build a working table from the interned fallback so theme KDL can
    // continue to layer component styles during load.
    let mut class_styles = Box::new([ClassStyle::EMPTY; Class::COUNT]);
    for index in 0..Class::COUNT {
        class_styles[index] = tokens.class_styles[index];
    }
    RawTheme {
        tokens: ThemeTokens {
            class_styles: ClassStylesRef::empty(),
            ..tokens
        },
        class_styles,
    }
}

fn finalize_fallback_tokens(mut raw: RawTheme) -> ThemeTokens {
    let styles = std::mem::replace(
        &mut raw.class_styles,
        Box::new([ClassStyle::EMPTY; Class::COUNT]),
    );
    raw.tokens.class_styles = ClassStylesRef::intern(*styles);
    raw.tokens
}

/// Seeds a minimal set of class styles on fallback palettes so UI chrome has
/// paint before KDL component blocks are applied (or when styles fail to load).
fn apply_fallback_class_styles(class_styles: &mut [ClassStyle; Class::COUNT], tokens: &ThemeTokens) {
    for class in [
        Class::AppShell,
        Class::Toolbar,
        Class::MenuBar,
        Class::Sidebar,
        Class::SidebarSection,
        Class::LibraryControlBar,
    ] {
        set_class_state(
            class_styles,
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
                shadow: None,
            },
        );
    }
    set_class_state(
        class_styles,
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
        Class::ContextMenuPanel,
        Class::SidebarDetailPanel,
        Class::SidebarDetailRow,
        Class::SidebarFolderCard,
        Class::JumpOverlay,
        Class::Tooltip,
        Class::AnnotationToolbar,
        Class::AnnotationPopover,
        Class::PresentationOverlay,
        Class::Minimap,
        Class::SelectionCheckbox,
        Class::MasterCheckbox,
        Class::DragStackGhost,
        Class::FolderDropTarget,
    ] {
        set_class_state(
            class_styles,
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
                shadow: None,
            },
        );
    }
    for class in [
        Class::ToolbarButton,
        Class::LibrarySortDropdown,
        Class::LibraryViewToggle,
        Class::LibraryImportButton,
        Class::LibraryGridZoomSlider,
        Class::SidebarActionButton,
        Class::MenuButton,
        Class::MenuItem,
        Class::ContextMenuItem,
        Class::SidebarRow,
        Class::SidebarToggleButton,
        Class::FileTreeFoldButton,
        Class::TocEntry,
        Class::TagPill,
    ] {
        set_class_state(
            class_styles,
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
                shadow: None,
            },
        );
        set_class_state(
            class_styles,
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
            class_styles,
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

/// Merges `style` into the working class-style table (used by fallbacks).
fn set_class_state(
    class_styles: &mut [ClassStyle; Class::COUNT],
    _tokens: &ThemeTokens,
    class: Class,
    state: ComponentState,
    style: VisualStyle,
) {
    class_styles[class.index()].states[state.index()] =
        class_styles[class.index()].states[state.index()].merged(style);
}

#[cfg(test)]
mod tests;
