use super::*;

pub(super) fn handle_shortcut(app: &mut PDFolioApp, shortcut: Shortcut) -> Task<Message> {
    match shortcut {
        Shortcut::In => {
            app.viewer.active_zoom_preset = None;
            app.zoom_to_width(
                app.viewer.zoom_width.saturating_add(100),
                None,
                ZoomRenderPolicy::Immediate,
            )
        }
        Shortcut::Out => {
            app.viewer.active_zoom_preset = None;
            app.zoom_to_width(
                app.viewer.zoom_width.saturating_sub(100),
                None,
                ZoomRenderPolicy::Immediate,
            )
        }
        Shortcut::Reset => {
            app.viewer.active_zoom_preset = Some(ZoomPreset::Automatic);
            let width = ZoomPreset::Automatic.width_for(app);
            app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate)
        }
        Shortcut::ToggleTheme => {
            app.appearance.theme = app.appearance.theme.toggled();
            Task::none()
        }
        Shortcut::ReloadStyles => Task::done(Message::ReloadStyles),
        Shortcut::PageDown => {
            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
                app.scroll_page_mode_by(1)
            } else {
                app.scroll_by(app.viewer.viewer_viewport_height * 0.86)
            }
        }
        Shortcut::PageUp => {
            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
                app.scroll_page_mode_by(-1)
            } else {
                app.scroll_by(-(app.viewer.viewer_viewport_height * 0.86))
            }
        }
        Shortcut::FineScroll(delta) => {
            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                app.pan_horizontally_by(f32::from(delta));
                Task::none()
            } else {
                app.scroll_by(f32::from(delta))
            }
        }
        Shortcut::HorizontalPan(delta) => {
            app.pan_horizontally_by(f32::from(delta));
            Task::none()
        }
        Shortcut::SelectAll => {
            if app.mode == AppMode::Library {
                app.select_all_visible_library_entries();
            }
            Task::none()
        }
        Shortcut::OpenSelected => {
            if app.mode == AppMode::Library && app.library.selected_library_entries.len() == 1 {
                if let Some(entry_id) = app.library.selected_library_entries.iter().next().cloned()
                {
                    return Task::done(Message::OpenLibraryEntry(entry_id));
                }
            }
            Task::none()
        }
        Shortcut::FocusSearch => {
            if app.mode == AppMode::Library {
                return operation::focus(Id::new(LIBRARY_SEARCH_INPUT_ID));
            }
            if app.mode == AppMode::Viewer {
                return app.open_viewer_find();
            }
            Task::none()
        }
        Shortcut::RenameSelected => {
            if app.mode == AppMode::Library && app.library.selected_library_entries.len() == 1 {
                return operation::focus(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID));
            }
            if app.mode == AppMode::Library && app.library.selected_folder.is_some() {
                return operation::focus(Id::new(LIBRARY_FOLDER_RENAME_INPUT_ID));
            }
            Task::none()
        }
        Shortcut::DeleteSelected => {
            if app.mode == AppMode::Library && !app.library.selected_library_entries.is_empty() {
                return Task::done(Message::RequestConfirmation(
                    ConfirmationAction::BulkDeleteFromLibrary,
                ));
            }
            if app.mode == AppMode::Library {
                if let Some(folder_id) = app.library.selected_folder.clone() {
                    return Task::done(Message::RequestConfirmation(
                        ConfirmationAction::DeleteFolder(folder_id),
                    ));
                }
            }
            Task::none()
        }
        Shortcut::Jump => {
            app.viewer.page_input_editing = false;
            app.viewer.jump_dialog_open = true;
            app.viewer.jump_input = (u32::from(app.current_page()) + 1).to_string();
            Task::none()
        }
        Shortcut::Copy => {
            if app.mode == AppMode::Viewer {
                app.copy_selected_viewer_text()
            } else {
                Task::none()
            }
        }
        Shortcut::Escape => {
            if app.chrome.pending_confirmation.is_some() {
                app.chrome.pending_confirmation = None;
            } else if app.chrome.open_app_menu.is_some() {
                app.chrome.open_app_menu = None;
                app.chrome.open_view_menu_flyout = None;
            } else if app.chrome.open_selection_menu.is_some() {
                app.chrome.open_selection_menu = None;
            } else if app.viewer.zoom_menu_open {
                app.viewer.zoom_menu_open = false;
            } else if app.viewer.zoom_editing {
                app.viewer.zoom_editing = false;
                app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            } else if app.viewer.page_input_editing {
                app.viewer.page_input_editing = false;
                app.viewer.jump_input.clear();
            } else if app.mode == AppMode::Viewer && app.viewer.viewer_find.open {
                app.viewer.viewer_find.open = false;
            } else if app.mode == AppMode::Viewer && app.viewer.viewer_text_selection.is_some() {
                app.clear_viewer_text_selection();
            } else if app.mode == AppMode::Library
                && !app.library.selected_library_entries.is_empty()
            {
                app.clear_library_selection();
            } else if app.viewer.jump_dialog_open {
                app.viewer.jump_dialog_open = false;
                app.viewer.jump_input.clear();
            } else if app.library.create_folder_dialog_open {
                app.library.create_folder_dialog_open = false;
            } else {
                app.viewer.toc_open = false;
            }
            Task::none()
        }
    }
}
