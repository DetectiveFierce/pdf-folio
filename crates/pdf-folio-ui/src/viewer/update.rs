//! Viewer-domain `update` handler: zoom, scroll, find, outline, render results.
//!
//! Called from shell update **before** the shell match. Returns `Some(task)`
//! when the message belongs to the viewer; `None` leaves the message for shell
//! (or library) handling.
//!
//! # Message clusters handled
//!
//! - Sidebar / TOC: `ToggleSidebar`, `ViewerSidebarTabSelected`
//! - Jump / page: `OpenJumpDialog`, `Jump*`, `PreviousPage`, `NextPage`, page input
//! - Find: `OpenViewerFind`, `CloseViewerFind`, `ViewerFind*`
//! - Outline: `ToggleOutlineNode`
//! - Text selection / copy: `ViewerText*`, `CopyViewerTextSelection`
//! - Scroll / viewport / wheel: `Viewport*`, modifiers
//! - Zoom: `Zoom*`, presets, scroll/spread mode, `ZoomRenderSettled`
//! - Render completion: `PageRendered` (and related text-layer loads)
//!
//! Many arms persist session state via [`crate::with_session_save`] or
//! [`crate::schedule_session_save`] (debounced on scroll). Prefer adding new
//! viewer messages here rather than in shell update.
//!
//! Related: [`super::navigation`], [`super::tasks`], [`super::document`].

use crate::viewer::rendering::{zoom_in_width, zoom_out_width};
use crate::*;

/// Handles viewer-domain messages; returns `None` if another reducer should try.
///
/// Tasks may include tile renders, scrollable sync, session saves, or find
/// scroll-to-match work.
pub(crate) fn update(app: &mut PDFolioApp, message: &Message) -> Option<Task<Message>> {
    match message {
        Message::ToggleSidebar => {
            app.viewer.toc_open = !app.viewer.toc_open;
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
            Some(with_session_save(app.apply_active_dimension_zoom(), app))
        }
        Message::ViewerSidebarTabSelected(tab) => {
            app.viewer.viewer_sidebar_tab = *tab;
            Some(with_session_save(app.request_viewer_thumbnail_pages(), app))
        }
        Message::OpenJumpDialog => {
            app.viewer.page_input_editing = false;
            app.viewer.jump_dialog_open = true;
            app.viewer.jump_input = app
                .viewer
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
            Some(Task::none())
        }
        Message::OpenViewerFind => Some(app.open_viewer_find()),
        Message::CloseViewerFind => {
            app.viewer.viewer_find.open = false;
            app.viewer.find_text_generation = app.viewer.find_text_generation.wrapping_add(1);
            Some(save_app_session_task(app))
        }
        Message::ViewerFindTextLayersContinue(generation) => {
            Some(app.continue_find_text_layers(*generation))
        }
        Message::ViewerFindQueryChanged(query) => Some(with_session_save(
            app.set_viewer_find_query(query.clone()),
            app,
        )),
        Message::ViewerFindPrevious => {
            app.viewer.viewer_find.select_previous();
            Some(app.scroll_to_selected_viewer_find_match())
        }
        Message::ViewerFindNext => {
            app.viewer.viewer_find.select_next();
            Some(app.scroll_to_selected_viewer_find_match())
        }
        Message::ViewerFindHighlightAllToggled(value) => {
            app.viewer.viewer_find.highlight_all = *value;
            Some(save_app_session_task(app))
        }
        Message::ViewerFindMatchCaseToggled(value) => {
            app.viewer.viewer_find.match_case = *value;
            app.refresh_viewer_find_matches();
            Some(with_session_save(
                app.scroll_to_selected_viewer_find_match(),
                app,
            ))
        }
        Message::ViewerFindMatchDiacriticsToggled(value) => {
            app.viewer.viewer_find.match_diacritics = *value;
            app.refresh_viewer_find_matches();
            Some(with_session_save(
                app.scroll_to_selected_viewer_find_match(),
                app,
            ))
        }
        Message::JumpInputChanged(value) => {
            app.viewer.jump_input = value.chars().filter(char::is_ascii_digit).take(5).collect();
            Some(Task::none())
        }
        Message::StartPageInputEdit => {
            app.viewer.jump_dialog_open = false;
            app.viewer.page_input_editing = true;
            app.viewer.jump_input = app
                .viewer
                .doc
                .as_ref()
                .map(|_| (u32::from(app.current_page()) + 1).to_string())
                .unwrap_or_default();
            Some(operation::focus(Id::new(PAGE_INPUT_ID)))
        }
        Message::SubmitJump => {
            if let Ok(page) = app.viewer.jump_input.parse::<u16>() {
                let task = app.jump_to_page(page.saturating_sub(1));
                return Some(Task::batch([
                    with_session_save(task, app),
                    app.schedule_reading_progress_save(),
                ]));
            }
            app.viewer.page_input_editing = false;
            app.viewer.jump_input.clear();
            Some(Task::none())
        }
        Message::JumpToPage(page) => {
            let task = app.jump_to_page(*page);
            Some(Task::batch([
                with_session_save(task, app),
                app.schedule_reading_progress_save(),
            ]))
        }
        Message::PreviousPage => {
            let page = app.current_page().saturating_sub(1);
            let task = app.jump_to_page(page);
            Some(Task::batch([
                with_session_save(task, app),
                app.schedule_reading_progress_save(),
            ]))
        }
        Message::NextPage => {
            if let Some(doc) = &app.viewer.doc {
                let page = app
                    .current_page()
                    .saturating_add(1)
                    .min(doc.page_count().saturating_sub(1));
                let task = app.jump_to_page(page);
                return Some(Task::batch([
                    with_session_save(task, app),
                    app.schedule_reading_progress_save(),
                ]));
            }
            Some(Task::none())
        }
        Message::ToggleOutlineNode(path) => {
            if !app.viewer.expanded_outline_paths.insert(path.clone()) {
                app.viewer.expanded_outline_paths.remove(path);
            }
            Some(save_app_session_task(app))
        }
        Message::ViewerTextLayerLoaded {
            page,
            layer,
            document_generation,
        } => {
            if *document_generation != app.viewer.document_generation {
                // Stale extraction from a previously open PDF — drop silently.
                return Some(Task::none());
            }
            app.viewer.pending_text_layers.remove(page);
            app.viewer
                .viewer_text_layers
                .insert(*page, Arc::clone(layer));
            let mut tasks = Vec::new();
            if app.viewer.viewer_find.open {
                let previous_match = app.viewer.viewer_find.selected_match();
                app.refresh_viewer_find_matches();
                if !app.viewer.viewer_find.query.is_empty()
                    && previous_match != app.viewer.viewer_find.selected_match()
                    && app.viewer.viewer_find.selected_match().is_some()
                {
                    tasks.push(app.scroll_to_selected_viewer_find_match());
                }
            }
            if app.viewer.viewer_copy_pending && app.selected_text_layers_ready() {
                tasks.push(app.copy_selected_viewer_text());
            }
            Some(if tasks.is_empty() {
                Task::none()
            } else {
                Task::batch(tasks)
            })
        }
        Message::ViewerTextLayerError {
            page,
            error,
            document_generation,
        } => {
            if *document_generation != app.viewer.document_generation {
                return Some(Task::none());
            }
            app.viewer.pending_text_layers.remove(page);
            app.viewer.document_error = Some(error.clone());
            Some(Task::none())
        }
        Message::ViewerTextSelectionStarted {
            page,
            char_index,
            expand,
        } => {
            app.start_viewer_text_selection(*page, *char_index, *expand);
            Some(Task::none())
        }
        Message::ViewerTextSelectionChanged { page, char_index } => {
            app.update_viewer_text_selection(*page, *char_index);
            Some(Task::none())
        }
        Message::ViewerTextSelectionEnded => {
            app.finish_viewer_text_selection();
            Some(Task::none())
        }
        Message::ViewerCanvasClicked | Message::ClearViewerTextSelection => {
            app.clear_viewer_text_selection();
            Some(Task::none())
        }
        Message::CopyViewerTextSelection => Some(app.copy_selected_viewer_text()),
        Message::ViewportChanged {
            horizontal_offset,
            scroll_offset,
            width,
            height,
        } => {
            app.viewer.last_scroll_offset = app.viewer.scroll_offset;
            app.viewer.horizontal_offset = *horizontal_offset;
            app.viewer.scroll_offset = *scroll_offset;
            app.viewer.viewer_viewport_width = width.max(1.0);
            app.viewer.viewer_viewport_height = height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();
            Some(Task::batch([
                app.apply_active_dimension_zoom(),
                app.request_visible_pages(),
                schedule_session_save(app),
                app.schedule_reading_progress_save(),
            ]))
        }
        Message::ViewportWheelScrolled {
            delta_x,
            delta_y,
            cursor,
            viewport_width,
            viewport_height,
        } => {
            app.viewer.viewer_viewport_width = viewport_width.max(1.0);
            app.viewer.viewer_viewport_height = viewport_height.max(1.0);
            app.clamp_horizontal_offset();
            app.clamp_scroll_offset();

            if app.viewer.modifiers.control() {
                app.viewer.active_zoom_preset = None;
                let direction = if delta_y.abs() >= delta_x.abs() {
                    *delta_y
                } else {
                    -*delta_x
                };
                let width = if direction > 0.0 {
                    zoom_in_width(app.viewer.zoom_width)
                } else {
                    zoom_out_width(app.viewer.zoom_width)
                };
                let task = app.zoom_to_width(width, Some(*cursor), ZoomRenderPolicy::Debounced);
                return Some(with_session_save_debounced(task, app));
            }

            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
                let Some(direction) = app.take_page_mode_wheel_turn(*delta_x, *delta_y) else {
                    return Some(Task::none());
                };
                let task = app.scroll_page_mode_by(direction);
                return Some(Task::batch([
                    with_session_save_debounced(task, app),
                    app.schedule_reading_progress_save(),
                ]));
            }

            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                let delta = if *delta_x != 0.0 { *delta_x } else { *delta_y };
                app.viewer.horizontal_offset =
                    (app.viewer.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
                return Some(Task::batch([
                    app.request_visible_pages(),
                    app.scroll_viewer_to_offsets_task(),
                    schedule_session_save(app),
                    app.schedule_reading_progress_save(),
                ]));
            }

            if app.viewer.modifiers.shift() || *delta_x != 0.0 {
                let delta = if *delta_x != 0.0 { *delta_x } else { *delta_y };
                app.viewer.horizontal_offset =
                    (app.viewer.horizontal_offset - delta).clamp(0.0, app.max_horizontal_offset());
                Some(Task::batch([
                    app.request_visible_pages(),
                    app.scroll_viewer_to_offsets_task(),
                    schedule_session_save(app),
                    app.schedule_reading_progress_save(),
                ]))
            } else {
                app.viewer.last_scroll_offset = app.viewer.scroll_offset;
                app.viewer.scroll_offset =
                    (app.viewer.scroll_offset - *delta_y).clamp(0.0, app.max_scroll_offset());
                Some(Task::batch([
                    app.request_visible_pages(),
                    schedule_session_save(app),
                    app.schedule_reading_progress_save(),
                ]))
            }
        }
        Message::ModifiersChanged(modifiers) => {
            app.viewer.modifiers = *modifiers;
            Some(Task::none())
        }
        Message::ZoomRenderSettled(generation) => {
            if *generation == app.viewer.zoom_generation {
                return Some(app.request_visible_pages());
            }
            Some(Task::none())
        }
        Message::ZoomIn => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(
                zoom_in_width(app.viewer.zoom_width),
                None,
                ZoomRenderPolicy::Immediate,
            );
            Some(with_session_save(task, app))
        }
        Message::ZoomOut => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(
                zoom_out_width(app.viewer.zoom_width),
                None,
                ZoomRenderPolicy::Immediate,
            );
            Some(with_session_save(task, app))
        }
        Message::ZoomSet(width) => {
            app.viewer.active_zoom_preset = None;
            let task = app.zoom_to_width(*width, None, ZoomRenderPolicy::Immediate);
            Some(with_session_save(task, app))
        }
        Message::StartZoomInputEdit => {
            app.viewer.zoom_editing = true;
            app.viewer.zoom_menu_open = false;
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            Some(operation::focus(Id::new(ZOOM_INPUT_ID)))
        }
        Message::ZoomInputChanged(value) => {
            app.viewer.zoom_input = value.clone();
            Some(Task::none())
        }
        Message::SubmitZoomInput => {
            let width = width_from_percent_input(&app.viewer.zoom_input);
            app.viewer.zoom_editing = false;
            if let Some(width) = width {
                app.viewer.active_zoom_preset = None;
                let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
                return Some(with_session_save(task, app));
            }
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            Some(Task::none())
        }
        Message::ToggleZoomMenu => {
            app.chrome.open_context_menu = None;
            app.viewer.zoom_menu_open = !app.viewer.zoom_menu_open;
            app.viewer.zoom_editing = false;
            app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            Some(Task::none())
        }
        Message::CloseZoomMenu => {
            app.viewer.zoom_menu_open = false;
            Some(Task::none())
        }
        Message::ZoomPresetSelected(preset) => {
            app.viewer.zoom_menu_open = false;
            app.viewer.zoom_editing = false;
            app.viewer.active_zoom_preset = Some(*preset);
            let width = preset.width_for(app);
            app.viewer.zoom_input = zoom_percent_label(width);
            let task = app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
            if matches!(preset, ZoomPreset::PageWidth) {
                app.viewer.horizontal_offset = 0.0;
            }
            Some(with_session_save(task, app))
        }
        Message::ViewerScrollModeSelected(mode) => {
            let task = app.set_viewer_scroll_mode(*mode);
            Some(with_session_save(task, app))
        }
        Message::ViewerSpreadModeSelected(mode) => {
            let task = app.set_viewer_spread_mode(*mode);
            Some(with_session_save(task, app))
        }
        _ => None,
    }
}
