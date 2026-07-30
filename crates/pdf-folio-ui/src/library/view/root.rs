//! # Library root view
//!
//! Entry point for library mode rendering: header or selection toolbar,
//! breadcrumbs, virtualized entry/folder content, navigation sidebar, and
//! optional inspector. Dialogs and drag previews are stacked by the shell
//! around this tree.
//!
//! Reads derived data via `visible_library_entries`, `child_folders`, and
//! layout helpers; does not mutate app state.

use super::*;
use iced::widget::{column, row};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Counts startup-probe view logs so `PDF_FOLIO_STARTUP_PROBE` only samples a few frames.
static LIBRARY_VIEW_PROBE_LOGS: AtomicUsize = AtomicUsize::new(0);

/// Compose the full library mode UI for the current app state.
///
/// Builds header or selection toolbar, breadcrumbs, virtualized folder/entry
/// content, navigation sidebar, and optional inspector. Dialogs and drag
/// previews are layered by the shell around this tree. Read-only — derives
/// data via `visible_library_entries` and layout helpers.
pub(crate) fn view_library(app: &PDFolioApp) -> Element<'_, Message> {
    let probe_started_at = std::env::var_os("PDF_FOLIO_STARTUP_PROBE").map(|_| Instant::now());
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let entries = app.visible_library_entries();
    let child_folders = app.child_folders();
    let render_items = library_render_items(app, &entries);
    let folder_section_height = folder_cards_section_height(app, child_folders.len());
    let entry_scroll_offset = (app.library.library_scroll_offset - folder_section_height).max(0.0);
    let window = app.visible_library_entry_window_at(render_items.len(), entry_scroll_offset);
    let reorder_hint = if app.can_drag_reorder_library() {
        "Manual reorder enabled"
    } else {
        "Reordering requires unfiltered Manual sort"
    };
    let header = if app.library.selected_library_entries.is_empty() {
        view_library_header(app, tokens)
    } else {
        view_library_selection_toolbar(app, tokens)
    };
    let context_row = view_library_breadcrumb_row(app, tokens, reorder_hint);
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
            render_items.len().saturating_sub(window.end) as f32 * app.library_row_height();
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
        content = content.push(library_scrollable(
            scroll_content,
            tokens,
            app.layout().library_scrollbar_gutter,
        ));
    } else {
        let layout = app.library_render_item_masonry_layout(&render_items);
        let mut grid = row![]
            .spacing(app.library_grid_column_gap())
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
                        LibraryRenderItem::DropZone(entry) => component_library_drop_zone_card(
                            app.library_grid_card_width(),
                            app.library_card_estimated_height(&entry.id),
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
        let mut scroll_content = column![].spacing(Spacing::MD);
        if app.parent_directory_drop_box_visible() {
            scroll_content = scroll_content.push(view_parent_directory_drop_box(app, tokens));
        }
        if !child_folders.is_empty() {
            scroll_content =
                scroll_content.push(view_folder_cards(app, child_folders.clone(), tokens));
        }
        scroll_content = scroll_content.push(grid);
        content = content.push(library_scrollable(
            scroll_content,
            tokens,
            app.layout().library_scrollbar_gutter,
        ));
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
    if library_inspector_visible(app) {
        layout = layout.push(view_library_inspector(app));
    }
    let element = layout.height(Length::Fill).into();
    if let Some(started_at) = probe_started_at {
        if LIBRARY_VIEW_PROBE_LOGS.fetch_add(1, Ordering::Relaxed) < 8 {
            tracing::warn!(
                elapsed_ms = started_at.elapsed().as_millis(),
                entries = entries.len(),
                child_folders = child_folders.len(),
                "PDF-Folio library view tree constructed"
            );
        }
    }
    element
}
