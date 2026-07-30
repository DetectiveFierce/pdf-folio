//! # Library-domain `update` handler
//!
//! Central match arm for library-mode messages: view mode, sort/zoom, search,
//! sidebar/inspector chrome, selection and details editing, folder/tag dialogs,
//! drag completion, import/export/Raindrop, trash, undo/redo, and bulk ops.
//!
//! ## Contract
//!
//! [`update`] is invoked from the shell message router. It returns:
//! - `Some(task)` when the message was handled (even if the task is `Task::none()`)
//! - `None` when the message is not a library concern (caller continues routing)
//!
//! Handlers typically mutate `app.library` (and sometimes chrome/session), then
//! spawn work via [`crate::library::tasks`] or refresh helpers on `PDFolioApp`
//! (`refresh_library`, `request_visible_thumbnails`, preference saves).
//!
//! ## Relationship to other modules
//!
//! - Imperative multi-step intents (select range, finish drag, open folder) live
//!   in [`crate::library::actions`]; this file mostly dispatches and wires tasks.
//! - Registry switch/create messages may be handled here or in shell depending
//!   on message type; multi-library persistence is in [`crate::library::registry`].
//! - Presentation is never built here — only state and tasks.

use crate::shell::tasks::start_auto_sync_now;
use crate::*;

/// Handle a library-domain [`Message`], mutating `app` and returning follow-up work.
///
/// Returns `None` if `message` is not owned by the library domain so the shell
/// can try viewer/shell handlers. Prefer calling high-level helpers on
/// `PDFolioApp` (from [`crate::library::actions`]) rather than duplicating
/// selection or drag logic inside match arms.
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
        Message::LibraryLoaded {
            entries,
            trash_entries,
        } => {
            app.library.library_entries = entries.clone();
            app.library.library_trash_entries = trash_entries.clone();
            app.rebuild_folder_smart_count_cache();
            app.library.library_history_restore_started_at = None;
            app.set_active_library_preview_from_entries();
            app.library.library_startup_loading = false;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_error = None;
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            app.sync_details_editor_to_selection();
            app.library.library_status = Some(format!(
                "{} PDFs in {}",
                app.active_library_entries().len(),
                if app.library.trash_view_active {
                    "trash"
                } else {
                    "library"
                }
            ));
            let restore_task = app.apply_pending_session_to_loaded_library();
            if !app.library.search_query.trim().is_empty() {
                return Some(Task::batch([
                    restore_task,
                    Task::done(Message::SearchDebounced(app.library.search_query.clone())),
                ]));
            }
            Some(Task::batch([
                restore_task,
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(app.library.library_scroll_offset),
            ]))
        }
        Message::LibraryFoldersLoaded(folders) => {
            app.library.library_folders = folders.clone();
            app.rebuild_folder_smart_count_cache();
            if !app.library.trash_view_active
                && app
                    .library
                    .selected_folder
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.selected_folder = None;
                app.sync_folder_rename_input();
                return Some(save_library_preferences_task(app));
            }
            if !app.library.trash_view_active
                && app
                    .library
                    .details_folder_id
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.details_folder_id = None;
            }
            app.sync_folder_rename_input();
            Some(Task::none())
        }
        Message::LibraryTrashFoldersLoaded(folders) => {
            app.library.library_trash_folders = folders.clone();
            app.rebuild_folder_smart_count_cache();
            if app.library.trash_view_active
                && app
                    .library
                    .selected_folder
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_trash_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.selected_folder = None;
                app.sync_folder_rename_input();
            }
            if app.library.trash_view_active
                && app
                    .library
                    .details_folder_id
                    .as_ref()
                    .is_some_and(|selected| {
                        !app.library
                            .library_trash_folders
                            .iter()
                            .any(|folder| &folder.id == selected)
                    })
            {
                app.library.details_folder_id = None;
            }
            app.sync_folder_rename_input();
            Some(Task::none())
        }
        Message::PendingRaindropRollbackChecked(status) => {
            if let Some(status) = status {
                app.library.library_startup_loading = true;
                app.library.raindrop_rollback_recovery_active = true;
                app.library.raindrop_rollback_recovery_status = Some(status.clone());
                match load_pending_raindrop_rollback() {
                    Ok(Some(rollback)) => {
                        return Some(rollback_pending_raindrop_import_task(
                            Arc::clone(&app.db),
                            rollback,
                        ));
                    }
                    Ok(None) => {
                        app.library.raindrop_rollback_recovery_active = false;
                        return Some(Task::none());
                    }
                    Err(error) => {
                        return Some(Task::done(Message::LibraryError(error.to_string())))
                    }
                }
            }
            Some(Task::none())
        }
        Message::PendingRaindropRollbackFinished { removed, errors } => {
            app.library.raindrop_rollback_recovery_status = Some(format!(
                "Finished interrupted Raindrop cleanup and removed {}.",
                format_count(*removed, "PDF")
            ));
            if errors.is_empty() {
                app.library.library_error = None;
            } else {
                app.library.library_error = Some(errors.join("\n"));
            }
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                attribute_pending_metadata_task(Arc::clone(&app.db)),
            ]))
        }
        Message::LibraryRefresh => Some(app.refresh_library()),
        Message::LibraryError(error) => {
            app.library.library_startup_loading = false;
            app.library.library_history_restore_started_at = None;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_status = Some(String::from("Library operation failed."));
            if !app.library.dismissed_library_errors.contains(error) {
                app.library.library_error = Some(error.clone());
            }
            app.library.raindrop_import_progress = None;
            app.library.bulk_operation_progress = None;
            app.library.pending_thumbnails.clear();
            Some(Task::none())
        }
        Message::DismissLibraryError => {
            if let Some(error) = app.library.library_error.take() {
                app.library.dismissed_library_errors.insert(error);
            }
            Some(scroll_library_to_offset_task(
                app.library.library_scroll_offset,
            ))
        }
        Message::LibraryStatus(status) => {
            app.library.library_status = Some(status.clone());
            app.library.library_error = None;
            Some(Task::none())
        }
        Message::OpenImportMenu => {
            app.library.import_menu_open = true;
            app.chrome.open_context_menu = None;
            Some(Task::none())
        }
        Message::CloseImportMenu => {
            app.library.import_menu_open = false;
            Some(Task::none())
        }
        Message::ImportFolderDialog => {
            app.library.import_menu_open = false;
            Some(import_folder_dialog_task())
        }
        Message::ImportFolderSelected(path) => {
            app.library.library_status = Some(format!("Importing {}...", path.display()));
            let db = Arc::clone(&app.db);
            let path = path.clone();
            app.settings.watch_directories.push(path.clone());
            app.settings.watch_directories.sort();
            app.settings.watch_directories.dedup();
            Some(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || import_folder_with_index(&db, &path))
                        .await?
                },
                |result| match result {
                    Ok(summary) => Message::ImportFinished(summary),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::ImportPdfDialog => {
            app.library.import_menu_open = false;
            Some(import_pdf_dialog_task())
        }
        Message::ImportPdfSelected(path) => {
            app.library.library_status = Some(format!("Importing {}...", path.display()));
            let db = Arc::clone(&app.db);
            let path = path.clone();
            let destination_folder = app.library.selected_folder.clone();
            Some(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        import_pdf_with_index(&db, path).and_then(|entry| {
                            if let Some(folder_id) = destination_folder.as_ref() {
                                db.add_entry_to_folder(&entry.id, folder_id)?;
                            }
                            Ok(pdf_folio_core::ImportSummary {
                                entries: vec![entry],
                                errors: Vec::new(),
                            })
                        })
                    })
                    .await?
                },
                |result| match result {
                    Ok(summary) => Message::ImportFinished(summary),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::ImportFinished(summary) => {
            let destination_label = app
                .library
                .selected_folder
                .as_ref()
                .and_then(|folder_id| {
                    app.library
                        .library_folders
                        .iter()
                        .find(|folder| &folder.id == folder_id)
                        .map(|folder| folder.name.clone())
                })
                .unwrap_or_else(|| String::from("Library root"));
            app.library.import_review = Some(import_review_from_summary(
                String::from("Import review"),
                summary,
                destination_label,
                Vec::new(),
            ));
            app.library.library_status = Some(format!(
                "Imported {} PDFs{}",
                summary.entries.len(),
                if summary.errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({} skipped)", summary.errors.len())
                }
            ));
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::AuthorAttributionFinished => Some(Task::batch([
            app.refresh_library(),
            start_auto_sync_now(app),
        ])),
        Message::ImportRaindrop => {
            app.library.import_menu_open = false;
            app.library.library_error = None;
            app.library.raindrop_import_preview = None;
            app.library.raindrop_pdf_thumbnails.clear();
            app.library.selected_raindrop_pdf_ids.clear();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
            if !pdf_folio_cloud::raindrop::can_import_without_prompt() {
                app.library.raindrop_connect_dialog_open = true;
                app.library.raindrop_callback_copied = false;
                app.library.library_status =
                    Some(String::from("Connect Raindrop.io to import PDFs."));
                return Some(scroll_library_to_offset_task(
                    app.library.library_scroll_offset,
                ));
            }
            app.library.raindrop_import_dialog_open = true;
            app.library.library_status = Some(String::from("Loading Raindrop PDFs..."));
            Some(Task::perform(
                async move { pdf_folio_cloud::raindrop::import_preview().await },
                |result| match result {
                    Ok(preview) => Message::RaindropImportPreviewLoaded(preview),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::RaindropImportPreviewLoaded(preview) => {
            let thumbnail_pdfs = preview.pdfs.clone();
            app.library.selected_raindrop_pdf_ids = preview
                .pdfs
                .iter()
                .map(|pdf| pdf.id)
                .collect::<HashSet<_>>();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_preview = Some(preview.clone());
            app.library.raindrop_import_dialog_open = true;
            app.library.raindrop_connect_dialog_open = false;
            app.library.library_error = None;
            app.library.library_status = Some(String::from("Choose Raindrop PDFs to import."));
            Some(Task::batch([
                scroll_library_to_offset_task(app.library.library_scroll_offset),
                raindrop_thumbnail_task(thumbnail_pdfs),
            ]))
        }
        Message::RaindropPdfThumbnailsLoaded(thumbnails) => {
            for (id, bytes) in thumbnails {
                app.library
                    .raindrop_pdf_thumbnails
                    .insert(*id, image::Handle::from_bytes(bytes.clone()));
            }
            Some(Task::none())
        }
        Message::RaindropPdfToggled(id, selected) => {
            if *selected {
                app.library.selected_raindrop_pdf_ids.insert(*id);
            } else {
                app.library.selected_raindrop_pdf_ids.remove(id);
            }
            Some(Task::none())
        }
        Message::SelectAllRaindropPdfs => {
            if let Some(preview) = app.library.raindrop_import_preview.as_ref() {
                app.library.selected_raindrop_pdf_ids = preview
                    .pdfs
                    .iter()
                    .map(|pdf| pdf.id)
                    .collect::<HashSet<_>>();
            }
            Some(Task::none())
        }
        Message::ClearAllRaindropPdfs => {
            app.library.selected_raindrop_pdf_ids.clear();
            Some(Task::none())
        }
        Message::RaindropDestinationChanged(destination) => {
            app.library.raindrop_import_destination = destination.clone();
            Some(Task::none())
        }
        Message::RaindropPreserveFolderStructureToggled(preserve_structure) => {
            let root_folder = raindrop_import_root_folder(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(*preserve_structure, root_folder);
            Some(Task::none())
        }
        Message::ToggleRaindropImportLocationMenu => {
            app.library.raindrop_import_location_menu_open =
                !app.library.raindrop_import_location_menu_open;
            Some(Task::none())
        }
        Message::RaindropImportRootChanged(folder_id) => {
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(preserve_structure, folder_id.clone());
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
            Some(Task::none())
        }
        Message::ToggleRaindropImportLocationFolder(folder_id) => {
            if !app
                .library
                .expanded_raindrop_import_location_folders
                .insert(folder_id.clone())
            {
                app.library
                    .expanded_raindrop_import_location_folders
                    .remove(folder_id);
            }
            Some(Task::none())
        }
        Message::StartNewRaindropImportFolder => {
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            app.library.raindrop_import_destination =
                raindrop_import_destination(preserve_structure, None);
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = true;
            app.library.raindrop_import_new_folder_name.clear();
            Some(Task::none())
        }
        Message::RaindropImportNewFolderNameChanged(value) => {
            app.library.raindrop_import_new_folder_name = value.clone();
            Some(Task::none())
        }
        Message::ImportSelectedRaindropPdfs => {
            let selected_ids = app.library.selected_raindrop_pdf_ids.clone();
            if selected_ids.is_empty() {
                return Some(Task::none());
            }
            let Some(preview) = app.library.raindrop_import_preview.as_ref() else {
                app.library.library_error = Some(String::from(
                    "Raindrop import metadata is still loading. Try again once the list appears.",
                ));
                return Some(Task::none());
            };
            let selected_pdfs = preview
                .pdfs
                .iter()
                .filter(|pdf| selected_ids.contains(&pdf.id))
                .cloned()
                .collect::<Vec<_>>();
            if selected_pdfs.is_empty() {
                return Some(Task::none());
            }
            let selected_preview = pdf_folio_cloud::raindrop::RaindropImportPreview {
                account_id: preview.account_id.clone(),
                account_label: preview.account_label.clone(),
                pdfs: selected_pdfs,
            };
            if app.library.raindrop_import_new_folder_active
                && app
                    .library
                    .raindrop_import_new_folder_name
                    .trim()
                    .is_empty()
            {
                app.library.library_error = Some(String::from(
                    "Enter a new folder name before importing to a new folder.",
                ));
                return Some(Task::none());
            }
            app.library.raindrop_import_dialog_open = false;
            app.library.raindrop_import_progress = Some(RaindropImportProgressView {
                completed: 0,
                total: selected_preview.pdfs.len(),
                current_title: String::from("Preparing import..."),
                phase: pdf_folio_cloud::raindrop::RaindropImportPhase::PreparingImports,
                progress_basis_points: None,
                failed: false,
                started_at: Instant::now(),
                imported_entries: Vec::new(),
                created_folders: Vec::new(),
                task_handle: None,
            });
            app.library.library_error = None;
            app.library.library_status = Some(format!(
                "Importing {} Raindrop PDFs...",
                selected_preview.pdfs.len()
            ));
            let db = Arc::clone(&app.db);
            let preserve_structure =
                raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
            let root_folder = raindrop_import_root_folder(&app.library.raindrop_import_destination);
            let new_folder_name = app
                .library
                .raindrop_import_new_folder_active
                .then(|| {
                    app.library
                        .raindrop_import_new_folder_name
                        .trim()
                        .to_owned()
                })
                .filter(|name| !name.is_empty());
            let (task, handle) = raindrop_import_task(
                db,
                selected_preview,
                preserve_structure,
                root_folder,
                new_folder_name,
            );
            if let Some(progress) = app.library.raindrop_import_progress.as_mut() {
                progress.task_handle = Some(handle);
            }
            Some(task)
        }
        Message::RaindropImportProgressUpdated(progress) => {
            let mut imported_entries = app
                .library
                .raindrop_import_progress
                .as_ref()
                .map_or_else(Vec::new, |progress| progress.imported_entries.clone());
            let mut created_folders = app
                .library
                .raindrop_import_progress
                .as_ref()
                .map_or_else(Vec::new, |progress| progress.created_folders.clone());
            if let Some(entry) = progress.entry.clone() {
                if !imported_entries
                    .iter()
                    .any(|existing| existing.path == entry.path)
                {
                    imported_entries.push(entry);
                }
            }
            for folder_id in &progress.created_folders {
                if !created_folders.contains(folder_id) {
                    created_folders.push(folder_id.clone());
                }
            }
            let pending_rollback = PendingRaindropRollback::from_progress(
                imported_entries.clone(),
                created_folders.clone(),
            );
            if !pending_rollback.is_empty() {
                if let Err(error) = save_pending_raindrop_rollback(&pending_rollback) {
                    app.library.library_error = Some(error.to_string());
                }
            }
            let task_handle = app
                .library
                .raindrop_import_progress
                .as_ref()
                .and_then(|progress| progress.task_handle.clone());
            app.library.raindrop_import_progress = Some(RaindropImportProgressView {
                completed: progress.completed,
                total: progress.total,
                current_title: progress.current_title.clone(),
                phase: progress.phase,
                progress_basis_points: progress.progress_basis_points,
                failed: progress.failed,
                started_at: app
                    .library
                    .raindrop_import_progress
                    .as_ref()
                    .map_or_else(Instant::now, |progress| progress.started_at),
                imported_entries,
                created_folders,
                task_handle,
            });
            Some(Task::none())
        }
        Message::RaindropImportCreatedFolder(folder_id) => {
            if let Some(progress) = app.library.raindrop_import_progress.as_mut() {
                if !progress.created_folders.contains(folder_id) {
                    progress.created_folders.push(folder_id.clone());
                }
            }
            Some(Task::none())
        }
        Message::CancelRaindropImport => {
            let Some(progress) = app.library.raindrop_import_progress.take() else {
                return Some(Task::none());
            };
            if let Some(handle) = progress.task_handle {
                handle.abort();
            }
            let pending_rollback = PendingRaindropRollback::from_progress(
                progress.imported_entries,
                progress.created_folders,
            );
            if pending_rollback.is_empty() {
                app.library.library_status = Some(String::from("Cancelled Raindrop import."));
                return Some(Task::none());
            }
            if let Err(error) = save_pending_raindrop_rollback(&pending_rollback) {
                app.library.library_error = Some(error.to_string());
            }

            app.library.library_startup_loading = false;
            app.library.raindrop_rollback_recovery_active = true;
            app.library.raindrop_rollback_recovery_status =
                Some(String::from("Undoing imported Raindrop PDFs..."));
            app.library.library_status = Some(String::from("Undoing cancelled Raindrop import..."));
            Some(rollback_pending_raindrop_import_task(
                Arc::clone(&app.db),
                pending_rollback,
            ))
        }
        Message::RaindropImportRollbackFinished { removed, errors } => {
            app.library.raindrop_import_progress = None;
            app.library.raindrop_rollback_recovery_active = false;
            app.library.raindrop_rollback_recovery_status = None;
            app.library.library_status = Some(format!(
                "Cancelled Raindrop import and removed {}.",
                format_count(*removed, "PDF")
            ));
            if errors.is_empty() {
                app.library.library_error = None;
            } else {
                app.library.library_error = Some(errors.join("\n"));
            }
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                attribute_pending_metadata_task(Arc::clone(&app.db)),
            ]))
        }
        Message::OpenRaindropIntegrations => {
            app.library.library_status = Some(String::from("Opening Raindrop.io integrations..."));
            Some(Task::perform(
                async {
                    webbrowser::open("https://app.raindrop.io/settings/integrations")?;
                    Ok::<_, anyhow::Error>(())
                },
                |result| match result {
                    Ok(()) => Message::LibraryStatus(String::from(
                        "Raindrop.io integrations opened in your browser.",
                    )),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::CopyRaindropCallbackUrl => {
            app.library.raindrop_callback_copied = true;
            app.library.library_status = Some(String::from("Callback url copied to clipboard!"));
            Some(clipboard::write(String::from(
                pdf_folio_cloud::raindrop::OAUTH_CALLBACK_URL,
            )))
        }
        Message::RaindropClientIdChanged(value) => {
            app.library.raindrop_callback_copied = false;
            app.library.raindrop_client_id_input = value.clone();
            Some(Task::none())
        }
        Message::RaindropClientSecretChanged(value) => {
            app.library.raindrop_callback_copied = false;
            app.library.raindrop_client_secret_input = value.clone();
            Some(Task::none())
        }
        Message::SubmitRaindropSignIn => {
            let client_id = app.library.raindrop_client_id_input.trim().to_owned();
            let client_secret = app.library.raindrop_client_secret_input.trim().to_owned();
            if client_id.is_empty() || client_secret.is_empty() {
                app.library.library_error = Some(String::from(
                    "Enter a Raindrop OAuth client ID and client secret before signing in.",
                ));
                return Some(Task::none());
            }
            app.library.raindrop_connect_dialog_open = false;
            app.library.raindrop_import_dialog_open = true;
            app.library.raindrop_import_preview = None;
            app.library.raindrop_pdf_thumbnails.clear();
            app.library.selected_raindrop_pdf_ids.clear();
            app.library.raindrop_import_location_menu_open = false;
            app.library.raindrop_import_new_folder_active = false;
            app.library.raindrop_import_new_folder_name.clear();
            app.library.library_error = None;
            app.library.library_status = Some(String::from(
                "Opening Raindrop.io in your browser for sign-in...",
            ));
            let oauth_config = pdf_folio_cloud::raindrop::RaindropOAuthConfig {
                client_id,
                client_secret,
            };
            Some(Task::perform(
                async move {
                    pdf_folio_cloud::raindrop::import_preview_with_auth(Some(oauth_config)).await
                },
                |result| match result {
                    Ok(preview) => Message::RaindropImportPreviewLoaded(preview),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::RaindropImportFinished(summary) => {
            if let Err(error) = clear_pending_raindrop_rollback() {
                app.library.library_error = Some(error.to_string());
            }
            app.library.raindrop_import_progress = None;
            app.library.import_review = Some(import_review_from_summary(
                format!("Raindrop import from {}", summary.account_label),
                &summary.import,
                String::from("Raindrop destination"),
                Vec::new(),
            ));
            app.library.library_status = Some(format!(
                "Imported {} Raindrop PDFs from {}{}",
                summary.import.entries.len(),
                summary.account_label,
                if summary.import.errors.is_empty() {
                    String::new()
                } else {
                    format!(" ({} skipped)", summary.import.errors.len())
                }
            ));
            if !summary.import.errors.is_empty() {
                app.library.library_error = Some(summary.import.errors.join("\n"));
            }
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::OpenLibraryEntry(entry_id) => {
            if let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            {
                app.viewer.pending_document_open = true;
                app.viewer.document_open_started_at = Some(Instant::now());
                return Some(open_library_document_task(entry.id, entry.path));
            }
            Some(Task::none())
        }
        Message::LibraryEntryClicked(entry_id) => {
            if app.library.library_drag.is_some() {
                return Some(Task::none());
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.details_folder_id = None;
            app.select_library_entry(entry_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_library_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == entry_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });
            app.library.last_library_click = Some((entry_id.clone(), now));
            if is_double_click {
                return Some(Task::done(Message::OpenLibraryEntry(entry_id.clone())));
            }
            Some(save_app_session_task(app))
        }
        Message::FolderClicked(folder_id) => {
            if app.library.folder_drag.is_some() {
                return Some(Task::none());
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.select_folder_for_details(folder_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_folder_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == folder_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });
            app.library.last_folder_click = Some((folder_id.clone(), now));
            if is_double_click {
                return Some(Task::done(Message::FolderSelected(folder_id.clone())));
            }
            Some(Task::none())
        }
        Message::FolderTreeClicked(folder_id) => {
            if app.library.folder_drag.is_some() {
                return Some(Task::none());
            }
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.select_folder_in_tree(folder_id.clone());
            let now = Instant::now();
            let is_double_click =
                app.library
                    .last_folder_click
                    .as_ref()
                    .is_some_and(|(last_id, last_click)| {
                        last_id == folder_id
                            && now.duration_since(*last_click) <= Duration::from_millis(500)
                    });
            app.library.last_folder_click = Some((folder_id.clone(), now));
            if is_double_click {
                return Some(Task::done(Message::FolderTreeFolderOpened(
                    folder_id.clone(),
                )));
            }
            Some(Task::none())
        }
        Message::FolderTreeFolderOpened(folder_id) => {
            app.open_folder_from_tree(folder_id.clone());
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                save_library_preferences_task(app),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
            ]))
        }
        Message::OpenTrashCan => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.library.trash_view_active = true;
            app.library.selected_folder = None;
            app.library.details_folder_id = None;
            app.library.folder_details_sidebar_open = false;
            app.library.search_query.clear();
            app.library.search_results = None;
            app.library.search_hit_pages.clear();
            app.library.active_tag_filter = None;
            app.library.active_reading_filter = None;
            app.library.active_recently_opened_filter = false;
            app.library.missing_filter_active = false;
            app.library.previous_tag_pill_view = None;
            app.library.library_drag = None;
            app.library.library_scroll_offset = 0.0;
            app.clear_library_selection();
            let visible_entries = app.visible_library_entries();
            app.prune_selection_to_visible_entries(&visible_entries);
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                scroll_library_to_offset_task(0.0),
                save_app_session_task(app),
            ]))
        }
        Message::EntryCheckboxToggled(entry_id) => {
            app.toggle_library_entry_selection(entry_id.clone());
            Some(Task::none())
        }
        Message::MasterCheckboxClicked => {
            match app.master_checkbox_state() {
                MasterCheckboxState::All => app.clear_library_selection(),
                MasterCheckboxState::None | MasterCheckboxState::Partial => {
                    app.select_all_visible_library_entries();
                }
            }
            Some(Task::none())
        }
        Message::LibraryEntryHoverChanged(entry_id, hovered) => {
            app.set_library_card_hover(entry_id.clone(), *hovered);
            Some(Task::none())
        }
        Message::AnimationFrame(now) => {
            app.tick_animations(*now);
            Some(Task::none())
        }
        Message::BeginLibraryEntryDrag(entry_id) => {
            app.begin_library_drag(entry_id.clone());
            Some(scroll_library_to_offset_task(
                app.library.library_scroll_offset,
            ))
        }
        Message::BeginFolderDrag(folder_id) => {
            app.begin_folder_drag(folder_id.clone());
            Some(scroll_library_to_offset_task(
                app.library.library_scroll_offset,
            ))
        }
        Message::BeginFolderTreeDrag(folder_id) => {
            app.begin_folder_tree_drag(folder_id.clone());
            Some(scroll_library_to_offset_task(
                app.library.library_scroll_offset,
            ))
        }
        Message::ClearLibrarySelection => {
            app.clear_library_selection();
            Some(Task::none())
        }
        Message::SelectAllVisibleLibraryEntries => {
            app.select_all_visible_library_entries();
            Some(Task::none())
        }
        Message::CutLibrarySelection => {
            if app.set_library_clipboard(LibraryClipboardMode::Cut) {
                app.library.library_status = app
                    .library
                    .clipboard
                    .as_ref()
                    .map(|clipboard| format!("{} ready to paste.", clipboard.label()));
            }
            Some(Task::none())
        }
        Message::CopyLibrarySelection => {
            if app.set_library_clipboard(LibraryClipboardMode::Copy) {
                app.library.library_status = app
                    .library
                    .clipboard
                    .as_ref()
                    .map(|clipboard| format!("{} ready to paste.", clipboard.label()));
            }
            Some(Task::none())
        }
        Message::PasteLibraryClipboard => {
            let Some(clipboard) = app.library.clipboard.clone() else {
                app.library.library_status = Some(String::from("Nothing to paste."));
                return Some(Task::none());
            };
            if !app.can_paste_library_clipboard() {
                app.library.library_status = Some(String::from(
                    "Choose a valid destination before pasting library items.",
                ));
                return Some(Task::none());
            }
            app.library.library_status = Some(format!("{}...", clipboard.paste_label()));
            Some(paste_library_clipboard_task(
                Arc::clone(&app.db),
                clipboard,
                app.library.selected_folder.clone(),
            ))
        }
        Message::LibraryClipboardPasteFinished {
            action,
            clipboard,
            updated,
            errors,
        } => {
            if action.before != action.after {
                app.library.history.push(action.clone());
            }
            if clipboard.mode == LibraryClipboardMode::Cut && errors.is_empty() {
                app.library.clipboard = None;
            }
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "{} {} item{}.",
                    clipboard.paste_label(),
                    updated,
                    if *updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "{} {} item{}; {} failed.",
                    clipboard.paste_label(),
                    updated,
                    if *updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::LibraryHistoryActionFinished {
            action,
            label,
            updated,
            errors,
        } => {
            app.library.bulk_operation_progress = None;
            if action.before != action.after {
                app.library.history.push(action.clone());
            }
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "{label} {updated} item{}.",
                    if *updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "{label} {updated} item{}; {} failed.",
                    if *updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::UndoLibraryAction => {
            if app.library.library_history_restore_started_at.is_some() {
                return Some(Task::none());
            }
            let Some((target_index, action)) = app.library.history.undo_target() else {
                app.library.library_status = Some(String::from("Nothing to undo."));
                return Some(Task::none());
            };
            let search_changed_entry_ids = action.after.search_changed_entry_ids(&action.before);
            app.library.library_history_restore_started_at = Some(Instant::now());
            app.library.library_status = Some(format!("Undoing {}...", action.label));
            Some(restore_library_history_snapshot_task(
                Arc::clone(&app.db),
                action.before,
                target_index,
                format!("Undid {}.", action.label),
                search_changed_entry_ids,
            ))
        }
        Message::RedoLibraryAction => {
            if app.library.library_history_restore_started_at.is_some() {
                return Some(Task::none());
            }
            let Some((target_index, action)) = app.library.history.redo_target() else {
                app.library.library_status = Some(String::from("Nothing to redo."));
                return Some(Task::none());
            };
            let search_changed_entry_ids = action.before.search_changed_entry_ids(&action.after);
            app.library.library_history_restore_started_at = Some(Instant::now());
            app.library.library_status = Some(format!("Redoing {}...", action.label));
            Some(restore_library_history_snapshot_task(
                Arc::clone(&app.db),
                action.after,
                target_index,
                format!("Redid {}.", action.label),
                search_changed_entry_ids,
            ))
        }
        Message::LibraryHistoryRestoreFinished {
            target_index,
            status,
        } => {
            app.library.history.set_current(*target_index);
            app.library.library_status = Some(status.clone());
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::LibraryEntryDragMoved(position) => {
            app.update_library_drag_target(*position);
            Some(Task::none())
        }
        Message::FolderDragMoved(position) => {
            app.update_folder_drag_target(*position);
            Some(Task::none())
        }
        Message::FolderDropTargetChanged(folder_id) => {
            app.set_folder_drop_hover_target(folder_id.clone(), Instant::now());
            Some(Task::none())
        }
        Message::ParentDirectoryDropTargetChanged(active) => {
            app.set_parent_directory_drop_hover_target(*active);
            Some(Task::none())
        }
        Message::LibraryAutoScrollTick(tick) => Some(app.auto_scroll_library_drag(*tick)),
        Message::EndLibraryEntryDrag => Some(app.finish_library_drag()),
        Message::EndFolderDrag => Some(app.finish_folder_drag()),
        Message::ManualEntryOrderSaved => {
            app.library.library_status = Some(String::from("Manual PDF order saved."));
            Some(Task::batch([
                app.refresh_library(),
                scroll_library_to_offset_task(app.library.library_scroll_offset),
                start_auto_sync_now(app),
            ]))
        }
        Message::LibraryWatchEvent(event) => {
            let db = Arc::clone(&app.db);
            let event = event.clone();
            app.library.library_status = Some(match &event {
                LibraryWatchEvent::PdfCreated(path) => format!("Importing {}...", path.display()),
                LibraryWatchEvent::PdfRemoved(path) => {
                    format!("Marking missing: {}", path.display())
                }
            });
            Some(Task::perform(
                async move { tokio::task::spawn_blocking(move || apply_watch_event(&db, event)).await? },
                |result| match result {
                    Ok(()) => Message::LibraryWatchEventApplied(Ok(())),
                    Err(error) => Message::LibraryWatchEventApplied(Err(error.to_string())),
                },
            ))
        }
        Message::LibraryWatchEventApplied(result) => Some(match result {
            Ok(()) => Task::batch([app.refresh_library(), start_auto_sync_now(app)]),
            Err(error) => Task::batch([
                Task::done(Message::LibraryError(error.clone())),
                start_auto_sync_now(app),
            ]),
        }),
        Message::StartTagEntry(entry_id) => {
            app.library.tag_entry_id = Some(entry_id.clone());
            app.library.tag_input.clear();
            Some(Task::none())
        }
        Message::TagInputChanged(value) => {
            app.library.tag_input = value.clone();
            Some(Task::none())
        }
        Message::SubmitTag => {
            if let Some(entry_id) = app.library.tag_entry_id.clone() {
                let tag = app.library.tag_input.trim().to_owned();
                app.library.tag_entry_id = None;
                app.library.tag_input.clear();
                if !tag.is_empty() {
                    let db = Arc::clone(&app.db);
                    return Some(Task::perform(
                        async move {
                            let saved_entry_id = entry_id.clone();
                            let saved_tag = tag.clone();
                            tokio::task::spawn_blocking(move || {
                                db.add_tag(&saved_entry_id, &saved_tag)
                            })
                            .await??;
                            Ok::<_, anyhow::Error>((entry_id, tag))
                        },
                        |result| match result {
                            Ok((id, tag)) => Message::EntryTagged { id, tag },
                            Err(error) => Message::LibraryError(error.to_string()),
                        },
                    ));
                }
            }
            Some(Task::none())
        }
        Message::StartTagRename(tag) => {
            app.library.tag_manager_open = false;
            app.library.library_sidebar_tab = LibrarySidebarTab::Tags;
            app.library.renaming_tag = Some(tag.clone());
            app.library.tag_rename_input = tag.clone();
            Some(operation::focus(Id::new(LIBRARY_TAG_RENAME_INPUT_ID)))
        }
        Message::TagRenameInputChanged(value) => {
            app.library.tag_rename_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
            Some(Task::none())
        }
        Message::SubmitTagRename => {
            let Some(old_tag) = app.library.renaming_tag.take() else {
                return Some(Task::none());
            };
            let new_tag = app.library.tag_rename_input.trim().to_owned();
            app.library.tag_rename_input.clear();
            if new_tag.is_empty() || new_tag == old_tag {
                return Some(Task::none());
            }
            if app.all_tags().iter().any(|tag| tag == &new_tag) {
                app.library.library_error = Some(format!("The tag \"{new_tag}\" already exists."));
                return Some(Task::none());
            }
            if app.library.active_tag_filter.as_ref() == Some(&old_tag) {
                app.library.active_tag_filter = Some(new_tag.clone());
            }
            app.library.library_status = Some(format!("Renaming tag \"{old_tag}\"..."));
            Some(rename_tag_task(Arc::clone(&app.db), old_tag, new_tag))
        }
        Message::CancelTagRename => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            Some(Task::none())
        }
        Message::DeleteTag(tag) => {
            if app.library.active_tag_filter.as_ref() == Some(tag) {
                app.library.active_tag_filter = None;
            }
            if app.library.renaming_tag.as_ref() == Some(tag) {
                app.library.renaming_tag = None;
                app.library.tag_rename_input.clear();
            }
            app.library.library_status = Some(format!("Deleting tag \"{tag}\"..."));
            Some(delete_tag_task(Arc::clone(&app.db), tag.clone()))
        }
        Message::EntryTagged { .. } | Message::EntryUntagged { .. } | Message::EntryDeleted(_) => {
            Some(Task::batch([
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::RequestConfirmation(action) => {
            if let ConfirmationAction::DeleteFolder(folder_id) = action {
                if app.chrome.folder_delete_warning_suppressed {
                    return Some(Task::done(Message::DeleteFolder(folder_id.clone())));
                }
                app.chrome.folder_delete_skip_warning_checked = false;
            }
            app.chrome.pending_confirmation = Some(action.clone());
            Some(Task::none())
        }
        Message::CancelConfirmation => {
            app.chrome.pending_confirmation = None;
            app.chrome.folder_delete_skip_warning_checked = false;
            Some(Task::none())
        }
        Message::FolderDeleteWarningSuppressionToggled(checked) => {
            app.chrome.folder_delete_skip_warning_checked = *checked;
            Some(Task::none())
        }
        Message::ConfirmPendingAction => {
            let Some(action) = app.chrome.pending_confirmation.take() else {
                return Some(Task::none());
            };
            if matches!(action, ConfirmationAction::DeleteFolder(_))
                && app.chrome.folder_delete_skip_warning_checked
            {
                app.chrome.folder_delete_warning_suppressed = true;
            }
            app.chrome.folder_delete_skip_warning_checked = false;
            Some(Task::done(match action {
                ConfirmationAction::BulkResetDisplayMetadata => Message::BulkResetDisplayMetadata,
                ConfirmationAction::BulkDeleteFromLibrary => Message::BulkDeleteFromLibrary,
                ConfirmationAction::PermanentlyDeleteFromTrash => {
                    Message::PermanentlyDeleteSelectedFromTrash
                }
                ConfirmationAction::PermanentlyDeleteFolderFromTrash(folder_id) => {
                    Message::PermanentlyDeleteSelectedFolderFromTrash(folder_id)
                }
                ConfirmationAction::ResetDetailsMetadata(entry_id) => {
                    Message::ResetDetailsMetadata(entry_id)
                }
                ConfirmationAction::DeleteFolder(folder_id) => Message::DeleteFolder(folder_id),
                ConfirmationAction::DeleteTag(tag) => Message::DeleteTag(tag),
                ConfirmationAction::DeleteLibrary(library_id) => Message::DeleteLibrary(library_id),
            }))
        }
        Message::DetailsTitleChanged(value) => {
            app.library.details_title_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
            Some(Task::none())
        }
        Message::DetailsAuthorChanged(value) => {
            app.library.details_author_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(240)
                .collect();
            Some(Task::none())
        }
        Message::SaveDetailsMetadata => {
            let Some(entry_id) = app.library.details_entry_id.clone() else {
                return Some(Task::none());
            };
            let Some(mut entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            entry.display_title = clean_metadata_input(&app.library.details_title_input);
            entry.display_author = clean_metadata_input(&app.library.details_author_input);
            entry.metadata_locked = true;
            app.library.library_status =
                Some(format!("Saving metadata for {}...", entry_title(&entry)));
            Some(edit_metadata_task(
                Arc::clone(&app.db),
                entry,
                app.library.details_title_input.clone(),
                app.library.details_author_input.clone(),
            ))
        }
        Message::ResetDetailsMetadata(entry_id) => {
            let Some(mut entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            entry.display_title = None;
            entry.display_author = None;
            entry.metadata_locked = false;
            app.library.library_status =
                Some(format!("Resetting metadata for {}...", entry_title(&entry)));
            Some(reset_metadata_task(Arc::clone(&app.db), entry))
        }
        Message::RevealEntryInFileManager(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            app.library.library_status = Some(format!("Revealing {}...", entry_title(&entry)));
            Some(open_file_manager_task(entry.path, true))
        }
        Message::OpenEntryContainingFolder(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            app.library.library_status =
                Some(format!("Opening folder for {}...", entry_title(&entry)));
            Some(open_file_manager_task(entry.path, false))
        }
        Message::CopyEntryFilePath(entry_id) => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            app.library.library_status = Some(String::from("File path copied."));
            Some(clipboard::write(entry.path.display().to_string()))
        }
        Message::RelinkMissingEntry(entry_id) => Some(relink_file_dialog_task(entry_id.clone())),
        Message::RelinkFileSelected { entry_id, path } => {
            let Some(entry) = app
                .active_library_entries()
                .iter()
                .find(|entry| &entry.id == entry_id)
                .cloned()
            else {
                return Some(Task::none());
            };
            app.library.library_status = Some(format!("Relinking {}...", entry_title(&entry)));
            Some(relink_entry_task(
                Arc::clone(&app.db),
                entry_id.clone(),
                path.clone(),
            ))
        }
        Message::RelinkFinished { entry_id: _, path } => {
            app.library.library_status = Some(format!("Relinked PDF to {}.", path.display()));
            app.library.library_error = None;
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]))
        }
        Message::MetadataEditFinished {
            entry_id: _,
            action,
            label,
            errors,
        } => {
            if action.before != action.after {
                app.library.history.push(action.clone());
            }
            app.library.library_status = Some(if errors.is_empty() {
                label.clone()
            } else {
                format!("{label}; {} indexing errors.", errors.len())
            });
            app.library.details_entry_id = None;
            Some(Task::batch([
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::BulkTagInputChanged(value) => {
            app.library.bulk_tag_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
            Some(Task::none())
        }
        Message::InspectorTagInputChanged(value) => {
            app.library.inspector_tag_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
            app.library.inspector_tag_suggestions_open =
                !app.library.inspector_tag_input.trim().is_empty();
            app.library.inspector_tag_highlighted_index = 0;
            Some(Task::none())
        }
        Message::InspectorApplyTag(tag) => {
            let tag = tag.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Some(Task::none());
            }
            app.library.inspector_tag_input.clear();
            app.library.inspector_tag_suggestions_open = false;
            app.library.inspector_tag_highlighted_index = 0;
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tag to", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tag"),
                move |db, entry_id| db.add_tag(entry_id, &tag),
            ))
        }
        Message::InspectorAddTag => {
            let tags = app
                .library
                .inspector_tag_input
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if tags.is_empty() || app.library.selected_library_entries.is_empty() {
                return Some(Task::none());
            }
            app.library.inspector_tag_input.clear();
            app.library.inspector_tag_suggestions_open = false;
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tags to", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tags"),
                move |db, entry_id| {
                    for tag in &tags {
                        db.add_tag(entry_id, tag)?;
                    }
                    Ok(())
                },
            ))
        }
        Message::InspectorRemoveTag { entry_id, tag } => {
            let tag = tag.clone();
            app.start_bulk_operation_progress("Removing tag from", 1);
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                vec![entry_id.clone()],
                String::from("Removed tag from"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            ))
        }
        Message::InspectorRemoveTagFromSelection(tag) => {
            let tag = tag.clone();
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Removing tag from", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Removed tag from"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            ))
        }
        Message::OpenTagManager => {
            app.library.tag_manager_open = true;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
            Some(Task::none())
        }
        Message::CloseTagManager => {
            app.library.tag_manager_open = false;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
            Some(Task::none())
        }
        Message::TagManagerFilterChanged(value) => {
            app.library.tag_manager_filter = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
            Some(Task::none())
        }
        Message::TagManagerMergeDestinationChanged(value) => {
            app.library.tag_manager_merge_destination = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect();
            Some(Task::none())
        }
        Message::MergeTag {
            source,
            destination,
        } => {
            let destination = destination.trim().to_owned();
            if source.trim().is_empty() || destination.is_empty() || source == &destination {
                return Some(Task::none());
            }
            app.library.tag_manager_open = false;
            app.library.tag_manager_filter.clear();
            app.library.tag_manager_merge_destination.clear();
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            Some(rename_tag_task(
                Arc::clone(&app.db),
                source.clone(),
                destination,
            ))
        }
        Message::BulkAddTag => {
            let tag = app.library.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Some(Task::none());
            }
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Adding tag to", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Tagged"),
                String::from("Add Tag"),
                move |db, entry_id| db.add_tag(entry_id, &tag),
            ))
        }
        Message::BulkRemoveTag => {
            let tag = app.library.bulk_tag_input.trim().to_owned();
            if tag.is_empty() || app.library.selected_library_entries.is_empty() {
                return Some(Task::none());
            }
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            app.start_bulk_operation_progress("Removing tag from", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Untagged"),
                String::from("Remove Tag"),
                move |db, entry_id| db.remove_tag(entry_id, &tag),
            ))
        }
        Message::BulkAddToCurrentFolder => {
            let Some(folder_id) = app.library.selected_folder.clone() else {
                app.library.library_status =
                    Some(String::from("Open a folder before adding PDFs to it."));
                return Some(Task::none());
            };
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Adding to folder", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Added to folder"),
                String::from("Add PDFs to Folder"),
                move |db, entry_id| db.add_entry_to_folder(entry_id, &folder_id),
            ))
        }
        Message::BulkRemoveFromCurrentFolder => {
            let Some(folder_id) = app.library.selected_folder.clone() else {
                app.library.library_status =
                    Some(String::from("Open a folder before removing PDFs from it."));
                return Some(Task::none());
            };
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Removing from folder", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Removed from folder"),
                String::from("Remove PDFs from Folder"),
                move |db, entry_id| db.remove_entry_from_folder(entry_id, &folder_id),
            ))
        }
        Message::BulkResetDisplayMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Resetting metadata for", entries.len());
            Some(bulk_reset_metadata_task(Arc::clone(&app.db), entries))
        }
        Message::BulkApplyTitleSortCleanup => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Cleaning title sort keys for", entry_ids.len());
            Some(bulk_operation_task(
                Arc::clone(&app.db),
                entry_ids,
                String::from("Cleaned title sort for"),
                String::from("Clean Title Sort"),
                |db, entry_id| db.apply_title_sort_cleanup(entry_id),
            ))
        }
        Message::BulkRefreshPdfMetadata => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Refreshing metadata for", entries.len());
            Some(bulk_refresh_metadata_task(Arc::clone(&app.db), entries))
        }
        Message::BulkRebuildThumbnails => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Some(Task::none());
            }
            for entry in &entries {
                app.library
                    .thumbnails
                    .retain(|key, _| key.entry_id != entry.id);
                app.library
                    .pending_thumbnails
                    .retain(|key| key.entry_id != entry.id);
            }
            app.start_bulk_operation_progress("Rebuilding thumbnails for", entries.len());
            Some(bulk_thumbnail_task(entries))
        }
        Message::BulkReindex => {
            let entries = app.selected_entries();
            if entries.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Reindexing", entries.len());
            Some(bulk_reindex_task(entries))
        }
        Message::BulkDeleteFromLibrary => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Moving to trash", entry_ids.len());
            Some(bulk_delete_metadata_task(Arc::clone(&app.db), entry_ids))
        }
        Message::RestoreSelectedFromTrash => {
            let entries = app.selected_entries();
            let folder_id = app
                .library
                .trash_view_active
                .then(|| app.library.details_folder_id.clone())
                .flatten();
            if entries.is_empty() && folder_id.is_none() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress(
                "Restoring",
                entries.len() + usize::from(folder_id.is_some()),
            );
            Some(bulk_restore_trash_items_task(
                Arc::clone(&app.db),
                entries,
                folder_id,
            ))
        }
        Message::PermanentlyDeleteSelectedFromTrash => {
            let entry_ids = app
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if entry_ids.is_empty() {
                return Some(Task::none());
            }
            app.start_bulk_operation_progress("Permanently deleting", entry_ids.len());
            Some(bulk_permanently_delete_entries_task(
                Arc::clone(&app.db),
                entry_ids,
            ))
        }
        Message::PermanentlyDeleteSelectedFolderFromTrash(folder_id) => {
            app.start_bulk_operation_progress("Permanently deleting", 1);
            Some(permanently_delete_folder_from_trash_task(
                Arc::clone(&app.db),
                folder_id.clone(),
            ))
        }
        Message::TrashFolderPermanentlyDeleted { updated, errors } => {
            app.library.bulk_operation_progress = None;
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!(
                    "Permanently deleted {updated} item{}.",
                    if *updated == 1 { "" } else { "s" }
                )
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!(
                    "Permanently deleted {updated} item{}; {} failed.",
                    if *updated == 1 { "" } else { "s" },
                    errors.len()
                )
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]))
        }
        Message::BulkOperationFinished {
            label,
            updated,
            errors,
        } => {
            app.library.bulk_operation_progress = None;
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                format!("{label} {updated} PDFs.")
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]))
        }
        Message::OpenExportDialog(source) => {
            app.library.export_dialog = Some(LibraryExportDialog::new(source.clone()));
            app.library.last_export_summary = None;
            Some(Task::none())
        }
        Message::CloseExportDialog => {
            app.library.export_dialog = None;
            app.library.export_progress = None;
            app.library.last_export_summary = None;
            Some(Task::none())
        }
        Message::ExportDestinationSelected(path) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.destination = Some(path.clone());
            }
            Some(Task::none())
        }
        Message::ChooseExportDestination => Some(export_destination_dialog_task()),
        Message::ExportModeChanged(mode) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.mode = *mode;
            }
            Some(Task::none())
        }
        Message::ExportFilenameTemplateChanged(template) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.filename_template = *template;
            }
            Some(Task::none())
        }
        Message::ExportMetadataCsvToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_metadata_csv = *value;
            }
            Some(Task::none())
        }
        Message::ExportMetadataJsonToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_metadata_json = *value;
            }
            Some(Task::none())
        }
        Message::ExportTagsToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_tags = *value;
            }
            Some(Task::none())
        }
        Message::ExportReadingProgressToggled(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.include_reading_progress = *value;
            }
            Some(Task::none())
        }
        Message::ExportConflictBehaviorChanged(value) => {
            if let Some(dialog) = app.library.export_dialog.as_mut() {
                dialog.conflict_behavior = *value;
            }
            Some(Task::none())
        }
        Message::StartExport => {
            let Some(dialog) = app.library.export_dialog.clone() else {
                return Some(Task::none());
            };
            if dialog.destination.is_none() {
                return Some(export_destination_dialog_task());
            }
            let entries = export_entries_for_source(app, &dialog.source);
            if entries.is_empty() {
                app.library.library_error =
                    Some(String::from("There are no PDFs available for this export."));
                return Some(Task::none());
            }
            app.library.export_progress = Some(LibraryExportProgress {
                label: String::from("Exporting PDFs"),
                total: entries.len(),
                started_at: Instant::now(),
            });
            Some(export_library_entries_task(entries, dialog))
        }
        Message::ExportFinished(result) => {
            app.library.export_progress = None;
            match result {
                Ok(summary) => {
                    app.library.library_status = Some(format!(
                        "Exported {} PDFs to {}{}",
                        summary.exported,
                        summary.destination.display(),
                        if summary.skipped == 0 {
                            String::new()
                        } else {
                            format!(" ({} skipped)", summary.skipped)
                        }
                    ));
                    if summary.errors.is_empty() {
                        app.library.library_error = None;
                    } else {
                        app.library.library_error = Some(summary.errors.join("\n"));
                    }
                    app.library.last_export_summary = Some(summary.clone());
                }
                Err(error) => {
                    app.library.library_error = Some(error.clone());
                    app.library.library_status = Some(String::from("Export failed."));
                }
            }
            Some(Task::none())
        }
        Message::RevealExportedFolder => {
            if let Some(summary) = app.library.last_export_summary.as_ref() {
                return Some(open_file_manager_task(summary.destination.clone(), false));
            }
            Some(Task::none())
        }
        Message::CopyExportPath => {
            if let Some(summary) = app.library.last_export_summary.as_ref() {
                app.library.library_status = Some(String::from("Export path copied."));
                return Some(clipboard::write(summary.destination.display().to_string()));
            }
            Some(Task::none())
        }
        Message::FolderAssignmentFinished {
            folder_id,
            label,
            updated,
            errors,
        } => {
            app.library.library_status = Some(if errors.is_empty() {
                app.library.library_error = None;
                if *updated > 0 {
                    if let Some(folder_id) = folder_id {
                        app.start_folder_drop_flash(folder_id.clone(), Instant::now());
                    }
                }
                format!("{label} {updated} PDFs.")
            } else {
                app.library.library_error = Some(errors.join("\n"));
                format!("{label} {updated} PDFs; {} failed.", errors.len())
            });
            app.clear_library_selection();
            app.library.pending_thumbnails.clear();
            Some(Task::batch([
                app.refresh_library(),
                app.request_visible_thumbnails(),
                start_auto_sync_now(app),
            ]))
        }
        Message::ThumbnailReady {
            entry_id,
            size,
            data,
            width,
            height,
        } => {
            let key = ThumbnailCacheKey {
                entry_id: entry_id.clone(),
                size: *size,
            };
            app.library.pending_thumbnails.remove(&key);
            let handle =
                image::Handle::from_rgba(u32::from(*width), u32::from(*height), data.clone());
            app.library.thumbnails.insert(
                key,
                ThumbnailView {
                    width: *width,
                    height: *height,
                    handle,
                },
            );
            Some(Task::none())
        }
        Message::ThumbnailFailed { key, error } => {
            app.library.pending_thumbnails.remove(key);
            tracing::debug!(%error, entry_id = %key.entry_id.as_str(), "Thumbnail load failed");
            Some(Task::none())
        }
        Message::ThumbnailSnapshotMiss { key } => {
            app.library.pending_thumbnails.remove(key);
            Some(if app.startup_background_ready {
                app.request_visible_thumbnails()
            } else {
                Task::none()
            })
        }
        Message::ProgressUpdated { entry_id, page } => {
            let db = Arc::clone(&app.db);
            let entry_id = entry_id.clone();
            let page = *page;
            Some(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || db.update_last_page(&entry_id, page))
                        .await??;
                    Ok::<_, anyhow::Error>(())
                },
                |result| match result {
                    Ok(()) => Message::ProgressSaved,
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ))
        }
        Message::ProgressSaved => Some(start_auto_sync_now(app)),
        Message::NewFolderNameChanged(value) => {
            app.library.new_folder_name = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
            Some(Task::none())
        }
        Message::FolderRenameInputChanged(value) => {
            app.library.folder_rename_input = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
            Some(Task::none())
        }
        Message::OpenCreateFolderDialog => {
            app.library.create_folder_dialog_open = true;
            Some(operation::focus(Id::new(LIBRARY_CREATE_FOLDER_INPUT_ID)))
        }
        Message::CreateFolder => {
            let name = app.library.new_folder_name.trim().to_owned();
            if name.is_empty() {
                return Some(Task::none());
            }
            let db = Arc::clone(&app.db);
            let parent_id = app.library.selected_folder.clone();
            app.library.library_status = Some(format!("Creating folder {name}..."));
            app.library.new_folder_name.clear();
            app.library.create_folder_dialog_open = false;
            Some(create_folder_task(db, name, parent_id))
        }
        Message::RenameSelectedFolder => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Some(Task::none());
            };
            let name = app.library.folder_rename_input.trim().to_owned();
            if name.is_empty() {
                return Some(Task::none());
            }
            app.library.library_status = Some(format!("Renaming folder to {name}..."));
            Some(rename_folder_task(Arc::clone(&app.db), folder_id, name))
        }
        Message::MoveSelectedFolderToRoot => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Some(Task::none());
            };
            app.library.library_status = Some(String::from("Moving folder to library root..."));
            Some(move_folder_task(Arc::clone(&app.db), folder_id, None))
        }
        Message::MoveSelectedFolderUp => {
            let Some(folder) = app.details_folder().cloned() else {
                return Some(Task::none());
            };
            let Some(parent_id) = folder.parent_id.as_ref() else {
                return Some(Task::none());
            };
            let grandparent_id = app
                .library
                .library_folders
                .iter()
                .find(|candidate| &candidate.id == parent_id)
                .and_then(|parent| parent.parent_id.clone());
            app.library.library_status = Some(String::from("Moving folder up one level..."));
            Some(move_folder_task(
                Arc::clone(&app.db),
                folder.id,
                grandparent_id,
            ))
        }
        Message::MoveSelectedFolderEarlier => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(-1) else {
                return Some(Task::none());
            };
            app.library.library_status = Some(String::from("Moving folder earlier..."));
            Some(persist_manual_folder_order_task(
                Arc::clone(&app.db),
                parent_id,
                folder_ids,
            ))
        }
        Message::MoveSelectedFolderLater => {
            let Some((parent_id, folder_ids)) = app.selected_folder_manual_reorder(1) else {
                return Some(Task::none());
            };
            app.library.library_status = Some(String::from("Moving folder later..."));
            Some(persist_manual_folder_order_task(
                Arc::clone(&app.db),
                parent_id,
                folder_ids,
            ))
        }
        Message::OpenMoveSelectionDialog => {
            if app.library.selected_library_entries.is_empty() {
                return Some(Task::none());
            }
            app.chrome.open_context_menu = None;
            app.library.move_picker = Some(LibraryMovePicker {
                target: LibraryMoveTarget::SelectedEntries,
                selected_destination: app.library.selected_folder.clone(),
                expanded_folders: app.move_picker_expanded_folders(),
            });
            Some(Task::none())
        }
        Message::OpenMoveSelectedFolderDialog => {
            let Some(folder_id) = app.library.details_folder_id.clone() else {
                return Some(Task::none());
            };
            let selected_destination = app
                .library
                .library_folders
                .iter()
                .find(|folder| folder.id == folder_id)
                .and_then(|folder| folder.parent_id.clone());
            app.chrome.open_context_menu = None;
            app.library.move_picker = Some(LibraryMovePicker {
                target: LibraryMoveTarget::Folder(folder_id),
                selected_destination,
                expanded_folders: app.move_picker_expanded_folders(),
            });
            Some(Task::none())
        }
        Message::MovePickerDestinationSelected(destination) => {
            let Some(picker) = app.library.move_picker.as_mut() else {
                return Some(Task::none());
            };
            if let LibraryMoveTarget::Folder(folder_id) = &picker.target {
                if destination.as_ref() == Some(folder_id)
                    || destination.as_ref().is_some_and(|destination| {
                        !folder_can_move_into(&app.library.library_folders, folder_id, destination)
                    })
                {
                    return Some(Task::none());
                }
            }
            picker.selected_destination = destination.clone();
            Some(Task::none())
        }
        Message::ToggleMovePickerFolder(folder_id) => {
            let Some(picker) = app.library.move_picker.as_mut() else {
                return Some(Task::none());
            };
            if !picker.expanded_folders.insert(folder_id.clone()) {
                picker.expanded_folders.remove(folder_id);
            }
            Some(Task::none())
        }
        Message::CancelMovePicker => {
            app.library.move_picker = None;
            Some(Task::none())
        }
        Message::ConfirmMovePicker => {
            let Some(picker) = app.library.move_picker.take() else {
                return Some(Task::none());
            };
            match picker.target {
                LibraryMoveTarget::SelectedEntries => {
                    let entry_ids = app
                        .library
                        .selected_library_entries
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    if entry_ids.is_empty() {
                        return Some(Task::none());
                    }
                    app.start_bulk_operation_progress("Moving", entry_ids.len());
                    Some(move_entries_to_folder_task(
                        Arc::clone(&app.db),
                        entry_ids,
                        picker.selected_destination,
                    ))
                }
                LibraryMoveTarget::Folder(folder_id) => {
                    app.library.library_status = Some(String::from("Moving folder..."));
                    Some(move_folder_task(
                        Arc::clone(&app.db),
                        folder_id,
                        picker.selected_destination,
                    ))
                }
            }
        }
        Message::RequestDeleteSelectedFolder => {
            if let Some(folder_id) = app.library.details_folder_id.clone() {
                if app.chrome.folder_delete_warning_suppressed {
                    return Some(Task::done(Message::DeleteFolder(folder_id)));
                }
                app.chrome.folder_delete_skip_warning_checked = false;
                app.chrome.pending_confirmation = Some(ConfirmationAction::DeleteFolder(folder_id));
            }
            Some(Task::none())
        }
        Message::DeleteFolder(folder_id) => {
            app.library.library_status = Some(String::from("Moving folder to trash..."));
            Some(delete_folder_task(Arc::clone(&app.db), folder_id.clone()))
        }
        Message::FolderUpdated => {
            app.library.library_status = Some(String::from("Folder updated."));
            Some(Task::batch([
                app.refresh_folders(),
                app.refresh_library(),
                start_auto_sync_now(app),
            ]))
        }
        Message::FolderCreated { folder_id, action } => {
            if action.before != action.after {
                app.library.history.push(action.clone());
            }
            app.library.library_status = Some(String::from("Folder created."));
            app.library.selected_folder = Some(folder_id.clone());
            app.library.details_folder_id = app.library.selected_folder.clone();
            app.sync_folder_rename_input();
            app.library.library_scroll_offset = 0.0;
            Some(Task::batch([
                save_library_preferences_task(app),
                app.refresh_folders(),
                app.refresh_library(),
                scroll_library_to_offset_task(0.0),
                start_auto_sync_now(app),
            ]))
        }
        _ => None,
    }
}
