//! Library view rendering.

use crate::app_view::dismissible_error_banner;
use crate::menu::library_sidebar_tab_label;
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{column, row, stack};
use pdf_folio_ui_components::library::view::{
    breadcrumb_button as component_breadcrumb_button, document_preview_lines, flush_media_style,
    ghost_tags_row, library_drop_zone_card as component_library_drop_zone_card,
    library_drop_zone_row as component_library_drop_zone_row,
    library_grid_zoom_control as component_library_grid_zoom_control,
    library_layout_toggle_button as component_library_layout_toggle_button,
    library_metadata_density_picker as component_library_metadata_density_picker,
    library_new_folder_button as component_library_new_folder_button,
    library_scrollable as component_library_scrollable,
    library_sort_picker as component_library_sort_picker, tags_row as component_tags_row,
    with_alpha,
};
use std::time::Duration;

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
    let mut header = row![];
    if !app.library.library_tag_sidebar_open {
        header = header.push(sidebar_chevron_button(
            CHEVRON_RIGHT_SVG,
            "Expand Sidebar",
            Message::ExpandLibrarySidebar,
            tokens,
        ));
    }
    let mut header = header
        .push(
            search_input_with_class(
                "Search library",
                &app.library.search_query,
                tokens,
                Class::LibrarySearchInput,
                Message::SearchQueryChanged,
            )
            .id(Id::new(LIBRARY_SEARCH_INPUT_ID))
            .width(Length::Fill),
        )
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
        ))
        .push(component_library_metadata_density_picker(
            app.library.library_metadata_density,
            &LIBRARY_METADATA_DENSITY_OPTIONS,
            tokens,
            Message::LibraryMetadataDensityChanged,
        ));
    if app.viewer.doc.is_some() {
        header = header.push(toolbar_button("Viewer", tokens).on_press(Message::BackToViewer));
    }
    let header = header
        .push(component_library_new_folder_button(tokens).on_press(Message::OpenCreateFolderDialog))
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center);
    let header = container(header)
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
            Message::DismissLibraryError,
        ));
    }

    if entries.is_empty() && child_folders.is_empty() {
        content = content.push(empty_state(
            if app.library.selected_folder.is_some() {
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
        let scroll_content = if child_folders.is_empty() {
            rows
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), rows]
                .spacing(Spacing::MD)
        };
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
        let scroll_content = if child_folders.is_empty() {
            column![grid]
        } else {
            column![view_folder_cards(app, child_folders.clone(), tokens), grid]
                .spacing(Spacing::MD)
        };
        content = content.push(library_scrollable(scroll_content, tokens));
    }

    let main_content = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell));

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
        ));
    }

    row![
        row![
            trail.width(Length::Shrink),
            library_quick_filter_chips(app, tokens),
            library_filter_summary(app, tokens),
            component_library_grid_zoom_control(
                LIBRARY_GRID_ZOOM_MIN,
                app.library_grid_zoom_max(),
                app.library_grid_zoom(),
                LIBRARY_GRID_ZOOM_STEP,
                app.library_grid_zoom_label(),
                tokens,
                Message::LibraryGridZoomChanged,
            ),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
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

pub(crate) fn library_quick_filter_chips<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut chips = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);

    let missing_active = app.library.missing_filter_active;
    let library_menu_text = tokens.class_styles[Class::LibraryControlBar.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_secondary);
    chips = chips.push(
        button(
            text("Missing")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(library_menu_text),
        )
        .on_press(Message::MissingFilterChanged(!missing_active))
        .padding([Spacing::XS, Spacing::MD])
        .style(move |_, status| {
            if missing_active {
                crate::style::button_style(tokens, Class::LibraryImportButton, status)
            } else {
                crate::style::button_style(tokens, Class::TagPill, status)
            }
        }),
    );

    chips.into()
}

pub(crate) fn library_filter_summary<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut labels = Vec::new();
    if let Some(folder_name) = app.selected_folder_name() {
        labels.push(format!("Folder: {folder_name}"));
    }
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
    for label in labels {
        row = row.push(
            container(
                text(label)
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
) -> Element<'a, Message> {
    component_breadcrumb_button(label, active, tokens, Message::FolderSelected(folder_id))
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
