//! Library view rendering.

use crate::app_view::dismissible_error_banner;
use crate::menu::library_sidebar_tab_label;
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{column, row, stack};
use pdf_folio_ui_components::library::view::{
    document_preview_lines, flush_media_style, ghost_tags_row,
    library_drop_zone_card as component_library_drop_zone_card,
    library_drop_zone_row as component_library_drop_zone_row,
    library_grid_zoom_control as component_library_grid_zoom_control,
    library_layout_toggle_button as component_library_layout_toggle_button,
    library_metadata_density_picker as component_library_metadata_density_picker,
    library_scrollable as component_library_scrollable,
    library_sort_picker as component_library_sort_picker, tags_row as component_tags_row,
    with_alpha,
};
use std::time::Duration;

const SEARCH_CLEAR_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"##;

#[path = "view/dialogs.rs"]
mod dialogs;
#[path = "view/entries.rs"]
mod entries;
#[path = "view/folders.rs"]
mod folders;
#[path = "view/sidebar.rs"]
mod sidebar;

pub(crate) use dialogs::*;
pub(crate) use entries::*;
pub(crate) use folders::*;
pub(crate) use sidebar::*;

pub(crate) fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let entries = app.visible_library_entries();
    let child_folders = app.child_folders();
    let render_items = library_render_items(app, &entries);
    let folder_section_height = folder_cards_section_height(app, child_folders.len());
    let entry_scroll_offset = (app.library.library_scroll_offset - folder_section_height).max(0.0);
    let window = app.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
    let toolbar_width = library_toolbar_available_width(app);
    let compact_toolbar = toolbar_width
        < app
            .layout()
            .metric("LibraryToolbar", "compact_width", 760.0);
    let narrow_toolbar =
        toolbar_width < app.layout().metric("LibraryToolbar", "narrow_width", 600.0);
    let mut search_row = row![];
    if !app.library.library_tag_sidebar_open {
        search_row = search_row.push(sidebar_chevron_button(
            CHEVRON_RIGHT_SVG,
            "Expand Sidebar",
            Message::ExpandLibrarySidebar,
            tokens,
        ));
    }
    search_row = search_row
        .push(library_search_input(app, tokens))
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill);

    let mut controls_row = row![]
        .push(library_history_icon_button(
            UNDO_SVG,
            "Undo",
            app.library.history.can_undo(),
            Message::UndoLibraryAction,
            tokens,
        ))
        .push(library_history_icon_button(
            REDO_SVG,
            "Redo",
            app.library.history.can_redo(),
            Message::RedoLibraryAction,
            tokens,
        ))
        .push(component_library_sort_picker(
            app.library.library_sort_mode,
            &LIBRARY_SORT_OPTIONS,
            tokens,
            Message::LibrarySortChanged,
        ))
        .push(component_library_layout_toggle_button(
            app.library.compact_view_mode,
            tokens,
            GRID_LAYOUT_SVG,
            LIST_LAYOUT_SVG,
            Message::ToggleViewMode,
        ));
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
    if !app.library.trash_view_active {
        controls_row = controls_row.push(
            library_new_folder_button(tokens, narrow_toolbar)
                .on_press(Message::OpenCreateFolderDialog),
        );
    }
    controls_row = controls_row
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(if compact_toolbar {
            Length::Shrink
        } else {
            Length::Fill
        });
    let header_content: Element<'_, Message> = if compact_toolbar {
        column![search_row, controls_row]
            .spacing(Spacing::SM)
            .width(Length::Fill)
            .into()
    } else {
        row![search_row, controls_row]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center)
            .width(Length::Fill)
            .into()
    };
    let header = container(header_content)
        .width(Length::Fill)
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::LibraryControlBar));

    let reorder_hint = if app.can_drag_reorder_library() {
        "Manual reorder enabled"
    } else {
        "Reordering requires unfiltered Manual sort"
    };
    let context_row = if app.library.selected_library_entries.is_empty() {
        view_library_breadcrumb_row(app, tokens, reorder_hint)
    } else {
        view_library_selection_status_row(app, tokens, reorder_hint)
    };
    let mut content = column![header, context_row,]
        .spacing(Spacing::MD)
        .padding(Spacing::LG);
    if let Some(progress) = app.library.bulk_operation_progress.as_ref() {
        content = content.push(bulk_operation_progress_banner(app, progress, tokens));
    }
    if let Some(error) = app.library.library_error.as_deref() {
        content = content.push(dismissible_error_banner(
            error,
            tokens,
            app.layout(),
            Message::DismissLibraryError,
        ));
    }

    if entries.is_empty() && child_folders.is_empty() {
        content = content.push(empty_state(
            if app.library.trash_view_active {
                "Trash Can is empty."
            } else if app.library.selected_folder.is_some() {
                "This folder is empty."
            } else {
                "Import a folder of PDFs to build your library."
            },
            tokens,
        ));
    } else if app.library.compact_view_mode {
        let mut rows = column![].spacing(Spacing::SM);
        let top_spacer = window.start as f32 * app.library_row_height();
        let bottom_spacer =
            entries.len().saturating_sub(window.end) as f32 * app.library_row_height();
        if top_spacer > 0.0 {
            rows = rows.push(container("").height(top_spacer));
        }
        for item in render_items[window.clone()].iter().cloned() {
            rows = rows.push(match item {
                LibraryRenderItem::Entry(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Normal)
                }
                LibraryRenderItem::Ghost(entry) => {
                    library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Placeholder)
                }
                LibraryRenderItem::DropZone(_) => {
                    component_library_drop_zone_row(app.layout().library_list_row_height, tokens)
                }
            });
        }
        if bottom_spacer > 0.0 {
            rows = rows.push(container("").height(bottom_spacer));
        }
        let mut scroll_content = column![].spacing(Spacing::MD);
        if app.parent_directory_drop_box_visible() {
            scroll_content = scroll_content.push(view_parent_directory_drop_box(app, tokens));
        }
        if !child_folders.is_empty() {
            scroll_content =
                scroll_content.push(view_folder_cards(app, child_folders.clone(), tokens));
        }
        scroll_content = scroll_content.push(rows);
        content = content.push(library_scrollable(scroll_content, tokens));
    } else {
        let layout = app.library_render_item_masonry_layout(&render_items);
        let mut grid = row![]
            .spacing(app.layout().library_masonry_gap)
            .height(layout.content_height);
        for column_items in &layout.columns {
            let mut stack = column![]
                .width(app.library_grid_card_width())
                .height(layout.content_height);
            let mut cursor_y = 0.0;
            for item_layout in column_items {
                let bottom = item_layout.top + item_layout.height;
                let visible_top = entry_scroll_offset
                    - app.layout().library_overscan_rows as f32 * app.library_row_height();
                let visible_bottom = entry_scroll_offset
                    + app.library.library_viewport_height.max(1.0)
                    + app.layout().library_overscan_rows as f32 * app.library_row_height();
                if bottom < visible_top || item_layout.top > visible_bottom {
                    continue;
                }

                let spacer = item_layout.top - cursor_y;
                if spacer > 0.0 {
                    stack = stack.push(container("").height(spacer));
                }
                if let Some(item) = render_items.get(item_layout.index).cloned() {
                    stack = stack.push(match item {
                        LibraryRenderItem::Entry(entry) => {
                            library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Normal)
                        }
                        LibraryRenderItem::Ghost(entry) => library_entry_card(
                            app,
                            entry,
                            tokens,
                            LibraryEntryRenderMode::Placeholder,
                        ),
                        LibraryRenderItem::DropZone(_) => component_library_drop_zone_card(
                            app.library_grid_card_width(),
                            app.library_card_estimated_height(&EntryId::new("__drop_zone__")),
                            app.library_card_font_size(FontSize::SM),
                            tokens,
                        ),
                    });
                    cursor_y = bottom;
                }
            }
            let trailing = layout.content_height - cursor_y;
            if trailing > 0.0 {
                stack = stack.push(container("").height(trailing));
            }
            grid = grid.push(stack);
        }
        grid = grid.push(container("").width(app.layout().library_scrollbar_gutter));
        let mut scroll_content = column![].spacing(Spacing::MD);
        if app.parent_directory_drop_box_visible() {
            scroll_content = scroll_content.push(view_parent_directory_drop_box(app, tokens));
        }
        if !child_folders.is_empty() {
            scroll_content =
                scroll_content.push(view_folder_cards(app, child_folders.clone(), tokens));
        }
        scroll_content = scroll_content.push(grid);
        content = content.push(library_scrollable(scroll_content, tokens));
    }

    let main_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell));
    let mut main_content = mouse_area(main_content).on_right_press(Message::ContextMenuOpened(
        ContextMenuTarget::LibraryBackground,
    ));
    if app.library.renaming_tag.is_some() {
        main_content = main_content.on_press(Message::CancelTagRename);
    }

    let mut layout = row![].height(Length::Fill);
    if app.library.library_tag_sidebar_open {
        layout = layout.push(view_library_tag_sidebar(app));
    }
    layout = layout.push(main_content);
    layout.height(Length::Fill).into()
}

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
        library_quick_filter_chips(app, tokens),
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
        let icon = Svg::new(iced::widget::svg::Handle::from_memory(SEARCH_CLEAR_SVG))
            .width(Length::Fixed(12.0))
            .height(Length::Fixed(12.0))
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(tokens.text_secondary),
            });
        let clear_button = button(container(icon).center(Length::Fill))
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(0)
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

pub(crate) fn library_toolbar_available_width(app: &PDFolioApp) -> f32 {
    let sidebar_width = if app.library.library_tag_sidebar_open {
        app.library.library_tag_sidebar_width + app.layout().sidebar_resize_handle_width
    } else {
        0.0
    };
    (app.viewer.viewport_width - sidebar_width - Spacing::LG * 2.0 - Spacing::SM * 2.0).max(1.0)
}

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

pub(crate) fn library_history_icon_button<'a>(
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
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(move |_, _| iced::widget::svg::Style { color: Some(color) });
    let button = button(container(icon).center(Length::Fixed(20.0)))
        .padding(Spacing::SM)
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
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

pub(crate) fn library_quick_filter_chips<'a>(
    _app: &'a PDFolioApp,
    _tokens: ThemeTokens,
) -> Element<'a, Message> {
    container("").width(Length::Shrink).into()
}

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

pub(crate) fn view_library_selection_status_row<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    reorder_hint: &'a str,
) -> Element<'a, Message> {
    let selected_count = app.library.selected_library_entries.len();
    let mut details = row![
        master_checkbox(
            app.master_checkbox_state(),
            tokens,
            Message::MasterCheckboxClicked
        ),
        text(format!("{} selected", format_count(selected_count, "PDF")))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.accent),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    if let Some(status) = app.library.library_status.as_deref() {
        details = details.push(
            text(status)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary),
        );
    }

    row![
        details,
        text(reorder_hint)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(if app.can_drag_reorder_library() {
                tokens.accent
            } else {
                tokens.text_secondary
            }),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center)
    .into()
}

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

pub(crate) fn library_scrollable<'a>(
    content: iced::widget::Column<'a, Message>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    component_library_scrollable(content, tokens, |viewport| {
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
