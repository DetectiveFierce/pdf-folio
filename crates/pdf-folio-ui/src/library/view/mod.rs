//! # Library view composition
//!
//! Iced view builders for library mode. Submodules split the large surface:
//!
//! - [`root`] — top-level library pane (header, sidebars, scroll content)
//! - [`entries`] — PDF cards/rows and selection chrome
//! - [`folders`] — folder cards, drag previews, masonry helpers
//! - [`sidebar`] — navigation tree, tags, selection details panels
//!
//! ## Ownership
//!
//! Domain composition: these functions take `&PDFolioApp` and emit
//! `Element<Message>`. Reusable pure widgets (scrollable shell, density
//! pickers, dialogs) are re-exported from `components::library` so domain
//! views stay thin wrappers over app-aware layout.
//!
//! Shared toolbar pieces in this file (breadcrumbs, search, filter chips)
//! are used by the root and selection toolbars.

use crate::components::library::cards::{
    document_preview_lines, flush_media_style, ghost_tags_row,
    library_drop_zone_card as component_library_drop_zone_card,
    library_drop_zone_row as component_library_drop_zone_row, tags_row as component_tags_row,
};
use crate::components::library::view::{
    library_grid_zoom_control as component_library_grid_zoom_control,
    library_layout_toggle_button as component_library_layout_toggle_button,
    library_metadata_density_picker as component_library_metadata_density_picker,
    library_scrollable as component_library_scrollable,
    library_sort_picker as component_library_sort_picker, with_alpha,
};
use crate::components::shared::error_banner::dismissible_error_banner;
pub(crate) use crate::components::shared::sidebar::*;
use crate::components::shared::sync_status::library_sync_indicator;
use crate::shell::commands::{command_message, command_visible, CommandId, CommandSurface};
use crate::*;
use iced::widget::{column, row, stack};

/// Inline SVG for the search-field clear (X) control.
const SEARCH_CLEAR_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"##;
/// PDF cards/rows and selection chrome for the main library pane.
mod entries;
/// Folder cards, drag previews, and masonry helpers shared with entry layout.
mod folders;
/// Top-level library pane (header, sidebars, scroll content).
mod root;
/// Navigation tree, tags, and selection details panels.
mod sidebar;

pub(crate) use crate::components::library::dialogs::*;
pub(crate) use crate::components::library::folder_tree::*;
pub(crate) use crate::components::library::import_status::*;
pub(crate) use crate::components::library::inspector::*;
pub(crate) use entries::*;
pub(crate) use folders::*;
pub(crate) use root::*;
pub(crate) use sidebar::*;

/// Default library toolbar: title, import/actions, search, sort, and layout controls.
fn view_library_header(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let toolbar_width = library_toolbar_available_width(app);
    let compact_toolbar = toolbar_width
        < app
            .layout()
            .metric("LibraryToolbar", "compact_width", 760.0);
    let narrow_toolbar =
        toolbar_width < app.layout().metric("LibraryToolbar", "narrow_width", 600.0);

    let mut title_row = row![];
    if !app.library.library_tag_sidebar_open {
        title_row = title_row.push(sidebar_chevron_button(
            CHEVRON_RIGHT_SVG,
            "Expand Sidebar",
            Message::ExpandLibrarySidebar,
            tokens,
        ));
    }
    title_row = title_row
        .push(
            text(library_header_title(app))
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None)
                .width(Length::Shrink),
        )
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(if compact_toolbar {
            Length::Fill
        } else {
            Length::FillPortion(2)
        });

    let search = container(library_search_input(app, tokens)).width(if compact_toolbar {
        Length::Fill
    } else {
        Length::FillPortion(3)
    });

    let mut controls_row = row![
        library_history_icon_button(
            app.layout(),
            UNDO_SVG,
            "Undo",
            app.library.history.can_undo(),
            Message::UndoLibraryAction,
            tokens,
        ),
        library_history_icon_button(
            app.layout(),
            REDO_SVG,
            "Redo",
            app.library.history.can_redo(),
            Message::RedoLibraryAction,
            tokens,
        ),
        component_library_sort_picker(
            app.library.library_sort_mode,
            &LIBRARY_SORT_OPTIONS,
            tokens,
            Message::LibrarySortChanged,
        ),
        component_library_layout_toggle_button(
            app.library.compact_view_mode,
            tokens,
            GRID_LAYOUT_SVG,
            LIST_LAYOUT_SVG,
            Message::ToggleViewMode,
        ),
    ];
    if !narrow_toolbar {
        controls_row = controls_row.push(component_library_metadata_density_picker(
            app.library.library_metadata_density,
            &LIBRARY_METADATA_DENSITY_OPTIONS,
            tokens,
            Message::LibraryMetadataDensityChanged,
        ));
    }
    if app.viewer.doc.is_some() {
        controls_row =
            controls_row.push(toolbar_button("Viewer", tokens).on_press(Message::BackToViewer));
    }
    if !app.library.trash_view_active
        && command_visible(app, CommandId::CreateFolder, CommandSurface::HeaderMore)
    {
        controls_row = controls_row.push(
            library_new_folder_button(tokens, narrow_toolbar).on_press(
                command_message(app, CommandId::CreateFolder)
                    .unwrap_or(Message::OpenCreateFolderDialog),
            ),
        );
    }
    let import_available = [
        CommandId::ImportPdf,
        CommandId::ImportFolder,
        CommandId::ImportRaindrop,
    ]
    .into_iter()
    .any(|id| command_visible(app, id, CommandSurface::ImportMenu));
    if import_available {
        controls_row = controls_row
            .push(library_header_button("Import", tokens).on_press(Message::OpenImportMenu));
    }
    let has_more_commands = [
        CommandId::RefreshLibrary,
        CommandId::SelectAllVisible,
        CommandId::ClearFilters,
        CommandId::RebuildThumbnails,
        CommandId::ReindexFullText,
        CommandId::ResetDisplayMetadata,
        CommandId::ApplyTitleSortCleanup,
        CommandId::ToggleMissingFiles,
    ]
    .into_iter()
    .any(|id| command_visible(app, id, CommandSurface::HeaderMore));
    if has_more_commands {
        controls_row = controls_row
            .push(library_header_button("More", tokens).on_press(Message::OpenCommandPalette));
    }
    controls_row = controls_row
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Shrink);

    let header_content: Element<'_, Message> = if compact_toolbar {
        column![title_row, search, controls_row]
            .spacing(Spacing::SM)
            .width(Length::Fill)
            .into()
    } else {
        row![title_row, search, controls_row]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center)
            .width(Length::Fill)
            .into()
    };

    container(header_content)
        .width(Length::Fill)
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::LibraryControlBar))
        .into()
}

/// Header title reflecting trash, active tag, folder breadcrumbs, or the vault name.
fn library_header_title(app: &PDFolioApp) -> String {
    if app.library.trash_view_active {
        return String::from("Trash Can");
    }
    if let Some(tag) = app.library.active_tag_filter.as_ref() {
        return format!("Tag: {tag}");
    }
    let breadcrumbs = app.folder_breadcrumbs();
    if breadcrumbs.len() > 1 {
        return breadcrumbs
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>()
            .join(" / ");
    }
    app.active_library_name().to_owned()
}

/// Selection-mode toolbar: count, bulk move/export/trash actions, and clear selection.
fn view_library_selection_toolbar(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let selected_count = app.library.selected_library_entries.len();
    let label = format!("{} selected", format_count(selected_count, "PDF"));
    let selection_label = row![
        master_checkbox(
            app.master_checkbox_state(),
            tokens,
            Message::MasterCheckboxClicked,
        ),
        text(label)
            .size(FontSize::MD)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    let mut actions = row![selection_label]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);

    if command_visible(
        app,
        CommandId::MoveSelectionToFolder,
        CommandSurface::SelectionToolbar,
    ) {
        actions = actions.push(
            toolbar_button("Move", tokens).on_press(
                command_message(app, CommandId::MoveSelectionToFolder)
                    .unwrap_or(Message::OpenMoveSelectionDialog),
            ),
        );
    }
    if command_visible(
        app,
        CommandId::RefreshMetadata,
        CommandSurface::SelectionToolbar,
    ) {
        actions = actions.push(
            toolbar_button("Refresh Metadata", tokens).on_press(
                command_message(app, CommandId::RefreshMetadata)
                    .unwrap_or(Message::BulkRefreshPdfMetadata),
            ),
        );
    }
    if command_visible(
        app,
        CommandId::ExportSelectedPdfs,
        CommandSurface::SelectionToolbar,
    ) {
        actions = actions.push(
            toolbar_button("Export", tokens).on_press(
                command_message(app, CommandId::ExportSelectedPdfs)
                    .unwrap_or(Message::OpenExportDialog(ExportSource::SelectedEntries)),
            ),
        );
    }
    actions = actions.push(toolbar_button("More", tokens).on_press(Message::OpenCommandPalette));

    if app.library.trash_view_active {
        actions = actions
            .push(toolbar_button("Restore", tokens).on_press(Message::RestoreSelectedFromTrash))
            .push(
                toolbar_button("Delete", tokens).on_press(Message::RequestConfirmation(
                    ConfirmationAction::PermanentlyDeleteFromTrash,
                )),
            );
    } else {
        actions = actions.push(toolbar_button("Trash", tokens).on_press(
            Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary),
        ));
    }
    actions =
        actions.push(toolbar_button("Clear", tokens).on_press(Message::ClearLibrarySelection));

    container(actions)
        .width(Length::Fill)
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::LibraryControlBar))
        .into()
}

/// Styled secondary header/toolbar button with `label` text (import-button class).
fn library_header_button<'a>(
    label: &'a str,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label)
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| button_style(tokens, Class::LibraryImportButton, status))
}

/// Folder path breadcrumbs plus reorder/filter context for the library toolbar.
pub(crate) fn view_library_breadcrumb_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let toolbar_width = library_toolbar_available_width(app);
    let compact = toolbar_width
        < app
            .layout()
            .metric("LibraryToolbar", "compact_width", 760.0);
    let narrow = toolbar_width < app.layout().metric("LibraryToolbar", "narrow_width", 600.0);
    let breadcrumb_width = if narrow {
        app.layout()
            .metric("LibraryToolbar", "breadcrumb_narrow_width", 94.0)
    } else {
        app.layout()
            .metric("LibraryToolbar", "breadcrumb_width", 150.0)
    };
    let breadcrumbs = app.folder_breadcrumbs();
    let active_index = breadcrumbs.len().saturating_sub(1);
    let mut trail = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    for (index, (label, folder_id)) in breadcrumbs.into_iter().enumerate() {
        if index > 0 {
            trail = trail.push(
                text(">")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            );
        }

        trail = trail.push(breadcrumb_button(
            label,
            folder_id,
            index == active_index,
            tokens,
            breadcrumb_width,
        ));
    }

    let mut controls = row![
        container(trail)
            .width(if compact {
                Length::FillPortion(3)
            } else {
                Length::Shrink
            })
            .clip(true),
        library_filter_summary(app, tokens, narrow),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);
    controls = controls.push(component_library_grid_zoom_control(
        app.library_grid_zoom_min(),
        app.library_grid_zoom_max(),
        app.library_grid_zoom(),
        app.library_grid_zoom_step(),
        app.library_grid_zoom_label(),
        tokens,
        Message::LibraryGridZoomChanged,
    ));
    controls = controls.push(library_sync_indicator(app, tokens));

    let reorder_hint_width = if narrow {
        app.layout()
            .metric("LibraryToolbar", "reorder_hint_narrow_width", 118.0)
    } else {
        app.layout()
            .metric("LibraryToolbar", "reorder_hint_width", 190.0)
    };
    let visible_reorder_hint =
        truncate_for_width_with_font(reorder_hint, reorder_hint_width, 0.0, FontSize::SM);
    row![
        controls,
        text(visible_reorder_hint)
            .width(reorder_hint_width)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            })
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Search field with clear control wired to library search messages.
pub(crate) fn library_search_input<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let input = text_input("Search library", &app.library.search_query)
        .on_input(Message::SearchQueryChanged)
        .padding(iced::Padding {
            top: Spacing::SM,
            right: if app.library.search_query.is_empty() {
                Spacing::MD
            } else {
                Spacing::XL
            },
            bottom: Spacing::SM,
            left: Spacing::MD,
        })
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, Class::LibrarySearchInput, status))
        .id(Id::new(LIBRARY_SEARCH_INPUT_ID))
        .width(Length::Fill);

    let mut search = stack![input].width(Length::Fill);
    if !app.library.search_query.is_empty() {
        let clear_icon_size = app
            .layout()
            .metric("LibrarySearchInput", "clear_icon_size", 12.0);
        let clear_button_size =
            app.layout()
                .metric("LibrarySearchInput", "clear_button_size", 24.0);
        let clear_button_padding =
            app.layout()
                .metric("LibrarySearchInput", "clear_button_padding", 0.0);
        let icon = Svg::new(iced::widget::svg::Handle::from_memory(SEARCH_CLEAR_SVG))
            .width(Length::Fixed(clear_icon_size))
            .height(Length::Fixed(clear_icon_size))
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(tokens.text_secondary),
            });
        let clear_button = button(container(icon).center(Length::Fill))
            .width(Length::Fixed(clear_button_size))
            .height(Length::Fixed(clear_button_size))
            .padding(clear_button_padding)
            .on_press(Message::SearchQueryChanged(String::new()))
            .style(move |_, _| iced::widget::button::Style {
                text_color: tokens.text_secondary,
                ..iced::widget::button::Style::default()
            });
        search = search.push(
            container(clear_button)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .center_y(Length::Fill)
                .padding(iced::Padding {
                    top: 0.0,
                    right: Spacing::XS,
                    bottom: 0.0,
                    left: 0.0,
                }),
        );
    }

    search.into()
}

/// Width budget for toolbar packing (compact vs full control sets).
pub(crate) fn library_toolbar_available_width(app: &PDFolioApp) -> f32 {
    let sidebar_width = if app.library.library_tag_sidebar_open {
        app.library.library_tag_sidebar_width + app.layout().sidebar_resize_handle_width
    } else {
        0.0
    };
    let inspector_width = if app.library.library_inspector_open {
        app.library.library_inspector_width + app.layout().sidebar_resize_handle_width
    } else {
        0.0
    };
    (app.viewer.viewport_width
        - sidebar_width
        - inspector_width
        - Spacing::LG * 2.0
        - Spacing::SM * 2.0)
        .max(1.0)
}

/// Toolbar control that opens the create-folder dialog.
pub(crate) fn library_new_folder_button<'a>(
    tokens: ThemeTokens,
    compact: bool,
) -> iced::widget::Button<'a, Message> {
    button(
        text(if compact { "New" } else { "New folder" })
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
    )
    .padding(if compact {
        [Spacing::SM, Spacing::MD]
    } else {
        [Spacing::SM, Spacing::LG]
    })
    .style(move |_, status| button_style(tokens, Class::LibraryImportButton, status))
}

/// Undo or redo icon button reflecting library organization history.
pub(crate) fn library_history_icon_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    icon: &'static [u8],
    label: &'static str,
    enabled: bool,
    message: Message,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let color = if enabled {
        tokens.text_secondary
    } else {
        with_alpha(tokens.text_secondary, 0.45)
    };
    let icon_size = layout.metric("LibraryHistoryButton", "icon_size", 16.0);
    let center_size = layout.metric("LibraryHistoryButton", "center_size", 20.0);
    let button_size = layout.metric("LibraryHistoryButton", "button_size", 32.0);
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(Length::Fixed(icon_size))
        .height(Length::Fixed(icon_size))
        .style(move |_, _| iced::widget::svg::Style { color: Some(color) });
    let button = button(container(icon).center(Length::Fixed(center_size)))
        .padding(Spacing::SM)
        .width(Length::Fixed(button_size))
        .height(Length::Fixed(button_size))
        .style(move |_, status| button_style(tokens, Class::ToolbarButton, status));
    let button = if enabled {
        button.on_press(message)
    } else {
        button
    };

    tooltip(
        button,
        container(
            text(label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(400))
    .into()
}

/// Compact label describing active search and smart filters.
pub(crate) fn library_filter_summary<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    narrow: bool,
) -> Element<'a, Message> {
    let mut labels = Vec::new();
    if let Some(tag) = app.library.active_tag_filter.as_ref() {
        labels.push(format!("Tag: {tag}"));
    }
    if let Some(filter) = app.library.active_reading_filter {
        labels.push(format!("Reading: {}", filter.label()));
    }
    if app.library.active_recently_opened_filter {
        labels.push(String::from("Recently Opened"));
    }
    if app.library.missing_filter_active {
        labels.push(String::from("Missing files"));
    }
    let query = app.library.search_query.trim();
    if !query.is_empty() {
        labels.push(format!("Search: {query}"));
    }

    if labels.is_empty() {
        return container("").width(Length::Shrink).into();
    }

    let mut row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    let pill_width = if narrow {
        app.layout()
            .metric("LibraryToolbar", "filter_pill_narrow_width", 112.0)
    } else {
        app.layout()
            .metric("LibraryToolbar", "filter_pill_width", 170.0)
    };
    for label in labels {
        let visible_label = truncate_for_width_with_font(&label, pill_width, 0.0, FontSize::SM);
        row = row.push(
            container(
                text(visible_label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_primary)
                    .wrapping(Wrapping::None),
            )
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| container_style(tokens, Class::TagPill)),
        );
    }

    row = row.push(
        tag_pill("Clear filters", tokens)
            .on_press(Message::ClearLibraryFilters)
            .padding([Spacing::XS, Spacing::MD]),
    );
    row.into()
}

/// Single breadcrumb segment that navigates to a folder (or root).
pub(crate) fn breadcrumb_button<'a>(
    label: String,
    folder_id: Option<FolderId>,
    active: bool,
    tokens: ThemeTokens,
    width: f32,
) -> Element<'a, Message> {
    let visible_label = truncate_for_width_with_font(&label, width, 0.0, FontSize::SM);
    button(
        text(visible_label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.accent
            })
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_, status| {
        if active {
            let active_style =
                tokens.class_styles[Class::SidebarRow.index()].resolve(ComponentState::Active);
            button_style(tokens, Class::SidebarRow, status).with_visual_override(active_style)
        } else {
            button_style(tokens, Class::SidebarRow, status)
        }
    })
    .on_press(Message::FolderSelected(folder_id))
    .into()
}

/// App-wired scrollable for the main library list with viewport reporting.
pub(crate) fn library_scrollable<'a>(
    content: iced::widget::Column<'a, Message>,
    tokens: ThemeTokens,
    scrollbar_gutter: f32,
) -> Element<'a, Message> {
    component_library_scrollable(content, tokens, scrollbar_gutter, |viewport| {
        let offset = viewport.absolute_offset();
        let bounds = viewport.bounds();
        Message::LibraryScrolled {
            offset_y: offset.y,
            viewport_x: bounds.x,
            viewport_y: bounds.y,
            viewport_width: bounds.width,
            viewport_height: bounds.height,
        }
    })
}
