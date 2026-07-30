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
        Message::ToggleLibraryTreeRoot => {
            app.library.library_tree_root_expanded = !app.library.library_tree_root_expanded;
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]))
        }
        Message::ToggleLibraryTags => {
            app.library.library_tags_expanded = !app.library.library_tags_expanded;
            Some(save_app_session_task(app))
        }
        Message::ToggleLibraryTreeFolder(folder_id) => {
            if !app
                .library
                .collapsed_library_tree_folders
                .insert(folder_id.clone())
            {
                app.library.collapsed_library_tree_folders.remove(folder_id);
            }
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
            ]))
        }
        Message::TagFilterChanged(tag) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.trash_view_active = false;
            app.library.active_tag_filter = tag.clone();
            app.library.active_recently_opened_filter = false;
            app.library.previous_tag_pill_view = None;
            if app.library.active_tag_filter.is_some() {
                app.library.selected_folder = None;
                app.library.details_folder_id = None;
            }
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]))
        }
        Message::TagTreeClicked(tag) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_tag_click
                    .as_ref()
                    .is_some_and(|(last_tag, last_click)| {
                        last_tag == tag
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });
            app.library.last_tag_click = Some((tag.clone(), now));
            if is_double_click {
                return Some(Task::done(Message::StartTagRename(tag.clone())));
            }
            Some(Task::done(Message::TagFilterChanged(Some(tag.clone()))))
        }
        Message::TagPillClicked(tag) => {
            app.library.previous_tag_pill_view = Some(LibraryViewSnapshot {
                search_query: app.library.search_query.clone(),
                search_results: app.library.search_results.clone(),
                search_hit_pages: app.library.search_hit_pages.clone(),
                active_tag_filter: app.library.active_tag_filter.clone(),
                active_reading_filter: app.library.active_reading_filter,
                active_recently_opened_filter: app.library.active_recently_opened_filter,
                missing_filter_active: app.library.missing_filter_active,
                selected_folder: app.library.selected_folder.clone(),
                details_folder_id: app.library.details_folder_id.clone(),
                library_scroll_offset: app.library.library_scroll_offset,
            });
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = Some(tag.clone());
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]))
        }
        Message::RestoreLibraryViewBeforeTag => {
            if let Some(snapshot) = app.library.previous_tag_pill_view.take() {
                app.library.search_query = snapshot.search_query;
                app.library.search_results = snapshot.search_results;
                app.library.search_hit_pages = snapshot.search_hit_pages;
                app.library.active_tag_filter = snapshot.active_tag_filter;
                app.library.active_reading_filter = snapshot.active_reading_filter;
                app.library.active_recently_opened_filter = snapshot.active_recently_opened_filter;
                app.library.missing_filter_active = snapshot.missing_filter_active;
                app.library.selected_folder = snapshot.selected_folder;
                app.library.details_folder_id = snapshot.details_folder_id;
                app.library.library_drag = None;
                app.library.library_scroll_offset = snapshot.library_scroll_offset.max(0.0);
                app.sync_folder_rename_input();
                let visible_entries = app.visible_library_entries();
                app.prune_selection_to_visible_entries(&visible_entries);
                return Some(Task::batch([
                    app.request_visible_thumbnails(),
                    scroll_library_to_offset_task(app.library.library_scroll_offset),
                    save_app_session_task(app),
                ]));
            }
            Some(Task::none())
        }
        Message::ReadingFilterChanged(filter) => {
            app.library.trash_view_active = false;
            app.library.active_reading_filter = *filter;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.active_tag_filter = None;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]))
        }
        Message::RecentlyOpenedFilterChanged(active) => {
            app.library.trash_view_active = false;
            app.library.active_recently_opened_filter = *active;
            if *active {
                app.library.active_reading_filter = None;
                app.library.missing_filter_active = false;
                app.library.active_tag_filter = None;
                app.library.selected_folder = None;
                app.library.details_folder_id = None;
            }
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]))
        }
        Message::MissingFilterChanged(active) => {
            app.library.trash_view_active = false;
            app.library.missing_filter_active = *active;
            if *active {
                app.library.active_recently_opened_filter = false;
            }
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(with_session_save(app.request_visible_thumbnails(), app))
        }
        Message::FolderSelected(folder_id) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.trash_view_active = false;
            app.library.selected_folder = folder_id.clone();
            app.library.active_recently_opened_filter = false;
            app.library.previous_tag_pill_view = None;
            app.select_folder_for_details(folder_id.clone());
            app.sync_folder_rename_input();
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]))
        }
        Message::ClearLibraryFilters => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.trash_view_active = false;
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = None;
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                save_library_preferences_task(app),
                save_app_session_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]))
        }
        _ => None,
    }
}
