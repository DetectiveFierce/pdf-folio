use crate::*;

pub(crate) fn update(app: &mut PDFolioApp, message: &Message) -> Option<Task<Message>> {
    match message {
        Message::ToggleViewMode => {
            app.library.compact_view_mode = !app.library.compact_view_mode;
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]))
        }
        Message::LibrarySortChanged(sort_mode) => {
            app.library.library_sort_mode = *sort_mode;
            app.library.library_scroll_offset = 0.0;
            app.library.library_drag = None;
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
                app.refresh_library(),
            ]))
        }
        Message::LibraryGridZoomChanged(zoom) => {
            app.library.library_grid_zoom =
                zoom.clamp(app.library_grid_zoom_min(), app.library_grid_zoom_limit());
            app.library.library_scroll_offset = app
                .library
                .library_scroll_offset
                .min(app.max_library_scroll_offset());
            app.update_library_drag_target_from_cursor();
            Some(Task::batch([
                save_app_session_task(app),
                app.request_visible_thumbnails(),
            ]))
        }
        Message::LibraryMetadataDensityChanged(density) => {
            app.library.library_metadata_density = *density;
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]))
        }
        Message::SearchQueryChanged(query) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.search_query = query.clone();
            app.library.library_drag = None;
            app.library.search_generation = app.library.search_generation.wrapping_add(1);
            let query = app.library.search_query.clone();
            if query.trim().is_empty() {
                app.library.search_results = None;
                app.library.search_hit_pages.clear();
                return Some(with_session_save(app.request_visible_thumbnails(), app));
            }
            Some(with_session_save(schedule_search(query), app))
        }
        Message::SearchDebounced(query) => {
            if query == &app.library.search_query {
                let db = Arc::clone(&app.db);
                let sort_mode = app.library.library_sort_mode;
                let trash_view_active = app.library.trash_view_active;
                return Some(Task::perform(
                    search_library_task(db, query.clone(), sort_mode, trash_view_active),
                    |result| match result {
                        Ok((entries, hit_pages)) => Message::SearchResults { entries, hit_pages },
                        Err(error) => Message::LibraryError(error.to_string()),
                    },
                ));
            }
            Some(Task::none())
        }
        Message::SearchResults { entries, hit_pages } => {
            app.library.search_results = Some(entries.clone());
            app.library.search_hit_pages = hit_pages.clone();
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(with_session_save(app.request_visible_thumbnails(), app))
        }
        Message::LibraryScrolled {
            offset_y,
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
        } => {
            app.library.library_scroll_offset = offset_y.max(0.0);
            app.library.library_viewport_x = *viewport_x;
            app.library.library_viewport_y = *viewport_y;
            app.library.library_viewport_width = viewport_width.max(1.0);
            app.library.library_viewport_height = viewport_height.max(1.0);
            app.update_library_drag_target_from_cursor();
            Some(with_session_save(app.request_visible_thumbnails(), app))
        }
        Message::CollapseLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library.library_tag_sidebar_open = false;
            app.library.resizing_library_tag_sidebar = false;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            Some(with_session_save(app.request_visible_thumbnails(), app))
        }
        Message::ExpandLibrarySidebar => {
            let columns = app.library_entries_per_row();
            app.library.library_tag_sidebar_open = true;
            app.recalculate_library_viewport_width();
            app.fit_library_grid_zoom_to_columns(columns);
            Some(with_session_save(app.request_visible_thumbnails(), app))
        }
        Message::ToggleLibrarySidebar => {
            if app.mode == AppMode::Library {
                let columns = app.library_entries_per_row();
                app.library.library_tag_sidebar_open = !app.library.library_tag_sidebar_open;
                app.library.resizing_library_tag_sidebar = false;
                app.recalculate_library_viewport_width();
                app.fit_library_grid_zoom_to_columns(columns);
                return Some(with_session_save(app.request_visible_thumbnails(), app));
            }
            Some(Task::none())
        }
        Message::BeginTagSidebarResize => {
            app.library.resizing_library_tag_sidebar = true;
            Some(Task::none())
        }
        Message::TagSidebarResizeDragged(width) => {
            if app.library.resizing_library_tag_sidebar {
                app.library.library_tag_sidebar_width = width.clamp(
                    app.layout().library_sidebar_min_width,
                    app.layout().library_sidebar_max_width,
                );
                app.recalculate_library_viewport_width();
            }
            Some(Task::none())
        }
        Message::EndTagSidebarResize => {
            app.library.resizing_library_tag_sidebar = false;
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]))
        }
        Message::ClearLibrarySidebarDetails => {
            app.clear_library_sidebar_details();
            Some(Task::none())
        }
        Message::ToggleLibraryInspector => {
            if app.mode == AppMode::Library {
                let columns = app.library_entries_per_row();
                app.library.library_inspector_open = !app.library.library_inspector_open;
                app.library.resizing_library_inspector = false;
                app.recalculate_library_viewport_width();
                app.fit_library_grid_zoom_to_columns(columns);
                return Some(with_session_save(app.request_visible_thumbnails(), app));
            }
            Some(Task::none())
        }
        Message::BeginLibraryInspectorResize => {
            app.library.resizing_library_inspector = true;
            app.library.library_inspector_open = true;
            Some(Task::none())
        }
        Message::LibraryInspectorResizeDragged(cursor_x) => {
            if app.library.resizing_library_inspector {
                let width = (app.viewer.viewport_width - cursor_x).max(1.0);
                app.library.library_inspector_width = width.clamp(
                    app.layout().metric("LibraryInspector", "min_width", 260.0),
                    app.layout().metric("LibraryInspector", "max_width", 520.0),
                );
                app.recalculate_library_viewport_width();
            }
            Some(Task::none())
        }
        Message::EndLibraryInspectorResize => {
            app.library.resizing_library_inspector = false;
            Some(save_app_session_task(app))
        }
        Message::LibrarySidebarTabChanged(tab) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.library_sidebar_tab = *tab;
            Some(save_app_session_task(app))
        }
        _ => None,
    }
}
