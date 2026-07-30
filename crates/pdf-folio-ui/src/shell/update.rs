use crate::library::registry::{
    create_library_profile, delete_library_profile, rename_library_profile,
};
use crate::shell::{shortcuts, tasks};
use crate::*;

pub(crate) use tasks::pending_raindrop_rollback_check_task;
use tasks::*;

pub(crate) fn update(app: &mut PDFolioApp, message: Message) -> Task<Message> {
    if let Some(task) = crate::library::update::update(app, &message) {
        return task;
    }
    if let Some(task) = crate::viewer::update::update(app, &message) {
        return task;
    }

    match message {
        Message::StartupResponsivenessProbe {
            launch_started_at,
            probe_started_at,
            emitted_at,
        } => {
            let processed_at = Instant::now();
            let total_ms = processed_at
                .saturating_duration_since(launch_started_at)
                .as_millis();
            let probe_wait_ms = emitted_at
                .saturating_duration_since(probe_started_at)
                .as_millis();
            let update_queue_ms = processed_at
                .saturating_duration_since(emitted_at)
                .as_millis();
            tracing::warn!(
                total_ms,
                probe_wait_ms,
                update_queue_ms,
                "PDF-Folio startup responsiveness probe processed"
            );
            app.library.library_status = Some(format!(
                "Startup probe: update accepted after {total_ms} ms ({update_queue_ms} ms queued)"
            ));
            return Task::none();
        }
        Message::StartupBackgroundReady => {
            app.startup_background_ready = true;
            app.load_cached_visible_thumbnails();
            let thumbnail_task = app.request_visible_thumbnails();
            if app.sync_auth.is_signed_in() {
                return Task::batch([
                    thumbnail_task,
                    sync_library_registry_for_app_task(app, false, true),
                ]);
            }
            return thumbnail_task;
        }
        Message::SyncSignInRequested => {
            app.sync_auth.state = SyncAuthState::SigningIn;
            app.sync_auth.error = None;
            return super::session::sync_sign_in_task(
                app.sync_auth.expected_email.clone(),
                app.sync_auth.server_base_url.clone(),
            );
        }
        Message::SyncSignInFinished(result) => match result {
            Ok(session) => match app.sync_auth.apply_signed_in_session(session) {
                Ok(()) => {
                    app.mode = AppMode::Library;
                    return Task::batch([
                        app.refresh_folders(),
                        app.refresh_library(),
                        pending_raindrop_rollback_check_task(),
                        sync_library_registry_for_app_task(app, false, true),
                    ]);
                }
                Err(error) => {
                    app.sync_auth.error = Some(error.to_string());
                }
            },
            Err(error) => {
                app.sync_auth.state = SyncAuthState::SignedOut;
                app.sync_auth.error = Some(error);
            }
        },
        Message::AutoSyncTick(_tick) => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            if app.sync_queued_libraries.is_empty() {
                return Task::none();
            }
            return start_next_queued_sync(app);
        }
        Message::RemoteSyncAvailable {
            library_id,
            noticed_at,
            remote_sequence,
        } => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            tracing::debug!(
                remote_sequence,
                library_id = %library_id,
                "Live sync watcher detected remote CRDT updates"
            );
            app.last_sync_started_at = Some(noticed_at);
            app.library.library_status =
                Some(String::from("Syncing updates from another device..."));
            return auto_sync_library_task(app, library_id);
        }
        Message::LibraryRegistryRemoteAvailable {
            noticed_at,
            remote_sequence,
        } => {
            if !app.sync_auth.is_signed_in() {
                return Task::none();
            }
            tracing::debug!(
                remote_sequence,
                "Live sync watcher detected remote library registry updates"
            );
            app.last_sync_started_at = Some(noticed_at);
            return sync_library_registry_for_app_task(app, false, false);
        }
        Message::AutoSyncFinished { library_id, result } => {
            if app.sync_in_progress.as_deref() == Some(library_id.as_str()) {
                app.sync_in_progress = None;
            }
            let mut follow_up_tasks = Vec::new();
            let library_is_active = app.libraries.active_library_id == library_id;
            let library_name = app
                .libraries
                .profiles
                .iter()
                .find(|profile| profile.id == library_id)
                .map_or(library_id.as_str(), |profile| profile.name.as_str());
            match result {
                Ok(report) => {
                    app.last_sync_completed_at = Some(std::time::SystemTime::now());
                    let uploads = report.uploads;
                    let crdt = report.crdt;
                    let hydration = report.hydration;
                    let library_changed = uploads.uploaded_blobs > 0
                        || uploads.failed_blobs > 0
                        || crdt.generated_operations > 0
                        || crdt.pushed_operations > 0
                        || crdt.pulled_operations > 0
                        || hydration.hydrated_entries > 0
                        || hydration.relinked_entries > 0
                        || hydration.hydrated_folders > 0
                        || hydration.hydrated_memberships > 0
                        || hydration.missing_blobs > 0;
                    if uploads.uploaded_blobs > 0
                        || uploads.failed_blobs > 0
                        || crdt.generated_operations > 0
                        || crdt.pushed_operations > 0
                        || crdt.pulled_operations > 0
                        || hydration.hydrated_entries > 0
                        || hydration.relinked_entries > 0
                        || hydration.hydrated_folders > 0
                        || hydration.hydrated_memberships > 0
                        || hydration.missing_blobs > 0
                    {
                        app.library.library_status = Some(format!(
                            "Synced {library_name}: {} PDFs, {} new, {} pushed, {} pulled, {} entries hydrated, {} PDFs healed, {} folders, {} memberships hydrated, {} PDFs missing.",
                            uploads.uploaded_blobs,
                            crdt.generated_operations,
                            crdt.pushed_operations,
                            crdt.pulled_operations,
                            hydration.hydrated_entries,
                            hydration.relinked_entries,
                            hydration.hydrated_folders,
                            hydration.hydrated_memberships,
                            hydration.missing_blobs
                        ));
                    }
                    if library_changed {
                        follow_up_tasks.push(refresh_library_preview_by_id_task(app, &library_id));
                    }
                    if library_is_active
                        && (crdt.pulled_operations > 0
                            || hydration.hydrated_entries > 0
                            || hydration.relinked_entries > 0
                            || hydration.hydrated_folders > 0
                            || hydration.hydrated_memberships > 0
                            || hydration.missing_blobs > 0)
                    {
                        follow_up_tasks.push(Task::batch([
                            app.refresh_folders(),
                            app.refresh_library(),
                            app.request_visible_thumbnails(),
                        ]));
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Automatic PDF-Folio sync failed");
                    app.library.library_status = Some(format!("Sync paused: {error}"));
                }
            }
            if !app.sync_queued_libraries.is_empty() {
                follow_up_tasks.push(start_next_queued_sync(app));
            }
            if !follow_up_tasks.is_empty() {
                return Task::batch(follow_up_tasks);
            }
        }
        Message::LibraryRegistrySyncFinished {
            sync_all_after,
            result,
        } => match result {
            Ok((registry, added_library_ids)) => {
                app.last_sync_completed_at = Some(std::time::SystemTime::now());
                let registry_task = match app.apply_library_registry(registry) {
                    Ok(task) => task,
                    Err(error) => return Task::done(Message::LibraryError(error.to_string())),
                };
                for library_id in added_library_ids {
                    app.sync_queued_libraries.insert(library_id);
                }
                let sync_task = if sync_all_after {
                    start_auto_sync_for_all_libraries(app)
                } else {
                    start_next_queued_sync(app)
                };
                return Task::batch([registry_task, sync_task]);
            }
            Err(error) => {
                tracing::debug!(%error, "Automatic PDF-Folio library registry sync paused");
                app.library.library_status = Some(format!("Library sync paused: {error}"));
            }
        },
        Message::LibraryPreviewRefreshed {
            library_id,
            preview,
        } => {
            app.libraries.previews.insert(library_id, preview);
        }
        Message::CursorMoved(position) => {
            app.chrome.cursor_position = position;
        }
        Message::ContextMenuOpened(target) => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.open_context_menu(target);
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::ContextMenuOpenedAt { target, position } => {
            app.library.renaming_tag = None;
            app.library.tag_rename_input.clear();
            app.chrome.cursor_position = position;
            app.open_context_menu(target);
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::ContextMenuClosed => {
            app.chrome.open_context_menu = None;
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::OpenCommandPalette => {
            if app.mode == AppMode::Library || app.mode == AppMode::Viewer {
                app.chrome.command_palette_open = true;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
                app.chrome.open_context_menu = None;
                if app.mode == AppMode::Library {
                    return scroll_library_to_offset_task(app.library.library_scroll_offset);
                }
            }
        }
        Message::CloseCommandPalette => {
            app.chrome.command_palette_open = false;
            app.chrome.command_palette_query.clear();
            app.chrome.command_palette_selected_index = 0;
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::CommandPaletteQueryChanged(query) => {
            app.chrome.command_palette_query = query;
            app.chrome.command_palette_selected_index = 0;
        }
        Message::CommandPaletteMoveSelection(delta) => {
            let visible_count = crate::shell::commands::library_commands(app)
                .into_iter()
                .filter(|command| {
                    command.visible
                        && crate::shell::commands::command_matches(
                            command.spec,
                            &app.chrome.command_palette_query,
                        )
                })
                .count();
            if visible_count > 0 {
                let current = app.chrome.command_palette_selected_index as i32;
                let next = (current + delta).rem_euclid(visible_count as i32) as usize;
                app.chrome.command_palette_selected_index = next;
            }
        }
        Message::CommandPaletteRunSelected => {
            let selected = crate::shell::commands::library_commands(app)
                .into_iter()
                .filter(|command| {
                    command.visible
                        && crate::shell::commands::command_matches(
                            command.spec,
                            &app.chrome.command_palette_query,
                        )
                })
                .nth(app.chrome.command_palette_selected_index)
                .map(|command| command.spec.id);
            if let Some(command_id) = selected {
                return Task::done(Message::CommandPaletteRun(command_id));
            }
        }
        Message::CommandPaletteRun(command_id) => {
            app.chrome.command_palette_open = false;
            app.chrome.command_palette_query.clear();
            app.chrome.command_palette_selected_index = 0;
            if let Some(message) = crate::shell::commands::command_message(app, command_id) {
                return Task::done(message);
            }
        }
        Message::CloseImportReview => {
            app.library.import_review = None;
        }
        Message::SelectImportReviewEntries => {
            if let Some(review) = app.library.import_review.as_ref() {
                let imported_entry_ids = review.imported_entry_ids.clone();
                app.clear_library_selection();
                for entry_id in imported_entry_ids {
                    app.select_library_entry(entry_id);
                }
            }
        }
        Message::ContextMenuActionSelected(action) => {
            if action == ContextMenuAction::SelectOnly {
                if let Some(ContextMenuTarget::LibraryEntry(entry_id)) = app
                    .chrome
                    .open_context_menu
                    .as_ref()
                    .map(|menu| menu.target.clone())
                {
                    app.clear_library_selection();
                    app.select_library_entry(entry_id);
                }
                app.chrome.open_context_menu = None;
                if app.mode == AppMode::Library {
                    return scroll_library_to_offset_task(app.library.library_scroll_offset);
                }
                return Task::none();
            }
            let message = app.context_menu_action_message(action);
            app.chrome.open_context_menu = None;
            if let Some(message) = message {
                return Task::done(message);
            }
            if app.mode == AppMode::Library {
                return scroll_library_to_offset_task(app.library.library_scroll_offset);
            }
        }
        Message::OpenFileDialog => return open_file_dialog_task(),
        Message::FileDialogCanceled => {}
        Message::FileSelected(path) => {
            app.viewer.pending_document_open = true;
            app.viewer.document_open_started_at = Some(Instant::now());
            return open_document_task(path);
        }
        Message::DocumentOpened { path, doc } => {
            let task = app.open_document_with_path(doc, Some(path));
            return with_session_save(task, app);
        }
        Message::LibraryDocumentOpened { entry_id, doc } => {
            let task = app.open_library_document(entry_id, doc);
            return with_session_save(Task::batch([task, mark_entry_opened_task(app)]), app);
        }
        Message::BackToLibrary => return with_session_save(app.return_to_library(), app),
        Message::BackToViewer => return with_session_save(app.return_to_viewer(), app),
        Message::DocumentError(error) => {
            app.viewer.pending_document_open = false;
            app.viewer.document_open_started_at = None;
            if !app.viewer.dismissed_document_errors.contains(&error) {
                app.viewer.document_error = Some(error);
            }
            app.viewer.pending_renders.clear();
            app.viewer.page_fade_started.clear();
        }
        Message::DismissDocumentError => {
            if let Some(error) = app.viewer.document_error.take() {
                app.viewer.dismissed_document_errors.insert(error);
            }
            app.viewer.document_error = None;
            return app.request_visible_pages();
        }
        Message::PageRendered {
            key,
            data,
            width,
            height,
            generation,
        } => {
            if app.viewer.pending_renders.get(&key) == Some(&generation) {
                app.viewer.pending_renders.remove(&key);
            }
            if generation.is_some_and(|generation| generation != app.viewer.zoom_generation) {
                return Task::none();
            }

            let had_fallback = generation.is_some()
                && key.width_px == app.render_width_px()
                && app.fallback_rendered_page_for_draw(key).is_some();
            app.viewer.cache.insert(key, data.clone());
            let handle = image::Handle::from_rgba(u32::from(width), u32::from(height), data);
            app.viewer.rendered_pages.insert(
                key,
                RenderedPageView {
                    width,
                    height,
                    handle,
                },
            );
            if had_fallback {
                app.viewer.page_fade_started.insert(key, Instant::now());
            }

            if key.width_px == app.render_width_px()
                && app.all_visible_pages_rendered_at_current_zoom()
            {
                app.viewer.zoom_preview_width_px = None;
            }
        }
        Message::ThemeToggled => {
            app.appearance.theme = app.appearance.theme.toggled();
            return save_app_session_task(app);
        }
        Message::ReloadStyles => {
            return Task::perform(async { StyleBook::load() }, Message::StylesReloaded);
        }
        Message::StylesReloaded(result) => match result {
            Ok(style_book) => {
                app.appearance.style_book = style_book;
                app.appearance.style_load_error = None;
                app.library.library_status = Some(String::from("Styles reloaded."));
            }
            Err(error) => {
                tracing::warn!(%error, "Failed to reload PDF-Folio styles");
                app.appearance.style_load_error = Some(error.clone());
                app.library.library_status = Some(format!("Style reload failed: {error}"));
            }
        },
        Message::OpenLibrarySwitcher => {
            app.open_library_switcher();
            return save_app_session_task(app);
        }
        Message::CloseLibrarySwitcher => {
            app.libraries.open_menu_library_id = None;
            app.mode = AppMode::Library;
            return save_app_session_task(app);
        }
        Message::SelectLibrary(library_id) => {
            app.libraries.open_menu_library_id = None;
            return match app.select_library(library_id) {
                Ok(task) => task,
                Err(error) => Task::done(Message::LibraryError(error.to_string())),
            };
        }
        Message::ToggleLibraryCardMenu(library_id) => {
            app.libraries.open_menu_library_id = (app.libraries.open_menu_library_id.as_ref()
                != Some(&library_id))
            .then_some(library_id);
        }
        Message::CloseLibraryCardMenu => {
            app.libraries.open_menu_library_id = None;
        }
        Message::OpenCreateLibraryDialog => {
            app.libraries.open_menu_library_id = None;
            app.libraries.new_library_name.clear();
            app.libraries.name_dialog = Some(LibraryNameDialog::Create);
            return operation::focus(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID));
        }
        Message::OpenRenameLibraryDialog(library_id) => {
            let Some(profile) = app
                .libraries
                .profiles
                .iter()
                .find(|profile| profile.id == library_id)
            else {
                return Task::none();
            };
            app.libraries.open_menu_library_id = None;
            app.libraries.new_library_name = profile.name.clone();
            app.libraries.name_dialog = Some(LibraryNameDialog::Rename(library_id));
            return operation::focus(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID));
        }
        Message::CancelLibraryNameDialog => {
            app.libraries.name_dialog = None;
            app.libraries.new_library_name.clear();
        }
        Message::ConfirmLibraryNameDialog => {
            let Some(dialog) = app.libraries.name_dialog.clone() else {
                return Task::none();
            };
            let name = app.libraries.new_library_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            return Task::done(match dialog {
                LibraryNameDialog::Create => Message::CreateLibrary,
                LibraryNameDialog::Rename(library_id) => {
                    app.libraries.rename_inputs.insert(library_id.clone(), name);
                    Message::RenameLibrary(library_id)
                }
            });
        }
        Message::NewLibraryNameChanged(value) => {
            app.libraries.new_library_name = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(80)
                .collect();
        }
        Message::CreateLibrary => {
            let name = app.libraries.new_library_name.trim().to_owned();
            if name.is_empty() {
                return Task::none();
            }
            let registry = app.libraries.clone();
            app.library.library_status = Some(format!("Creating library {name}..."));
            app.libraries.name_dialog = None;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || create_library_profile(registry, name))
                        .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::LibraryRegistryUpdated(registry) => {
            return match app.apply_library_registry(registry) {
                Ok(task) => Task::batch([
                    task,
                    sync_library_registry_for_app_task(app, false, true),
                    start_auto_sync_now(app),
                ]),
                Err(error) => Task::done(Message::LibraryError(error.to_string())),
            };
        }
        Message::LibraryRenameInputChanged { library_id, value } => {
            app.libraries.rename_inputs.insert(
                library_id,
                value
                    .chars()
                    .filter(|ch| !ch.is_control())
                    .take(80)
                    .collect(),
            );
        }
        Message::RenameLibrary(library_id) => {
            let name = app
                .libraries
                .rename_inputs
                .get(&library_id)
                .cloned()
                .unwrap_or_default();
            if name.trim().is_empty() {
                return Task::none();
            }
            let registry = app.libraries.clone();
            app.libraries.name_dialog = None;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        rename_library_profile(registry, library_id, name)
                    })
                    .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::RequestDeleteLibrary(library_id) => {
            app.libraries.open_menu_library_id = None;
            app.chrome.pending_confirmation = Some(ConfirmationAction::DeleteLibrary(library_id));
        }
        Message::DeleteLibrary(library_id) => {
            let registry = app.libraries.clone();
            app.library.library_status = Some(String::from("Deleting library..."));
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        delete_library_profile(registry, library_id)
                    })
                    .await?
                },
                |result| match result {
                    Ok(registry) => Message::LibraryRegistryUpdated(registry),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            );
        }
        Message::LibraryPreferencesSaved | Message::SessionSaved => {}
        Message::CloseOverlay => {
            if app.chrome.command_palette_open {
                app.chrome.command_palette_open = false;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
            } else if app.libraries.name_dialog.is_some() {
                app.libraries.name_dialog = None;
                app.libraries.new_library_name.clear();
            } else if app.libraries.open_menu_library_id.is_some() {
                app.libraries.open_menu_library_id = None;
            } else if app.mode == AppMode::LibrarySwitcher {
                app.mode = AppMode::Library;
                return save_app_session_task(app);
            } else if app.viewer.jump_dialog_open {
                app.viewer.jump_dialog_open = false;
                app.viewer.jump_input.clear();
            } else if app.viewer.page_input_editing {
                app.viewer.page_input_editing = false;
                app.viewer.jump_input.clear();
            } else if app.viewer.viewer_find.open {
                app.viewer.viewer_find.open = false;
            } else if app.library.create_folder_dialog_open {
                app.library.create_folder_dialog_open = false;
            } else if app.library.move_picker.is_some() {
                app.library.move_picker = None;
            } else if app.library.import_menu_open {
                app.library.import_menu_open = false;
            } else if app.library.raindrop_connect_dialog_open {
                app.library.raindrop_connect_dialog_open = false;
            } else if app.library.raindrop_import_dialog_open {
                app.library.raindrop_import_dialog_open = false;
            } else if app.library.import_review.is_some() {
                app.library.import_review = None;
            } else if app.library.tag_manager_open {
                app.library.tag_manager_open = false;
                app.library.tag_manager_filter.clear();
                app.library.tag_manager_merge_destination.clear();
            } else if app.library.export_dialog.is_some()
                || app.library.export_progress.is_some()
                || app.library.last_export_summary.is_some()
            {
                app.library.export_dialog = None;
                app.library.export_progress = None;
                app.library.last_export_summary = None;
            } else if app.chrome.pending_confirmation.is_some() {
                app.chrome.pending_confirmation = None;
            } else if app.chrome.open_context_menu.is_some() {
                app.chrome.open_context_menu = None;
            } else {
                app.viewer.toc_open = false;
            }
        }
        Message::WindowResized { width, height } => {
            app.viewer.viewport_width = width.max(1.0);
            app.viewer.viewport_height = height.max(1.0);
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
            if app.mode == AppMode::Library {
                app.recalculate_library_viewport_width();
                app.library.library_viewport_height =
                    (app.viewer.viewport_height - Spacing::LG * 2.0).max(1.0);
                return with_session_save(app.request_visible_thumbnails(), app);
            }
            return with_session_save(app.apply_active_dimension_zoom(), app);
        }
        Message::ShortcutPressed(shortcut) => return shortcuts::handle_shortcut(app, shortcut),
        _ => {}
    }

    Task::none()
}
