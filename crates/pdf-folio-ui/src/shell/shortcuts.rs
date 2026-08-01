//! Keyboard shortcut mapping for the application shell.
//!
//! Translates iced window and keyboard events into [`Message`] values.
//! Window resize becomes `WindowResized`; recognized key chords become
//! [`Message::ShortcutPressed`] with a [`Shortcut`] payload. Global chords
//! such as Ctrl+F / Ctrl+C / Ctrl+K / Escape may fire even when a widget has
//! marked the event as captured.
//!
//! [`handle_shortcut`] applies the semantic action: viewer zoom and scroll,
//! library selection/clipboard, command palette navigation, find, theme
//! toggle, and style reload. Mode checks decide whether library-only actions
//! run.
//!
//! Related: [`super::messages::Shortcut`] for the enum,
//! [`super::subscriptions`] for how keyboard listening is registered,
//! [`super::commands`] for palette intents that overlap some shortcuts.

use crate::*;
use iced::{event, keyboard, Event, Task};

use crate::messages::{Message, Shortcut};

/// Maps a raw iced event to a shell message, or `None` if unhandled.
///
/// Returns `None` for non-keyboard/window events and for captured key
/// presses that are not on the global-shortcut allow-list.
pub(crate) fn keyboard_event_message(event: Event, status: event::Status) -> Option<Message> {
    match event {
        Event::Window(iced::window::Event::Opened { size, .. })
        | Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized {
            width: size.width,
            height: size.height,
        }),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            text,
            modifiers,
            ..
        }) => {
            let captured_global_shortcut = is_ctrl_character(&key, text.as_deref(), modifiers, "f")
                || is_ctrl_character(&key, text.as_deref(), modifiers, "c")
                || is_ctrl_character(&key, text.as_deref(), modifiers, "k")
                || is_escape(&key);
            if status == event::Status::Captured && !captured_global_shortcut {
                return None;
            }

            match (&key, text.as_deref()) {
                (_, Some("t") | Some("T")) if modifiers.control() && modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::ToggleTheme))
                }
                (_, Some("r") | Some("R")) if modifiers.control() && modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::ReloadStyles))
                }
                (_, Some("g") | Some("G")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::Jump))
                }
                (key, text) if is_ctrl_character(key, text, modifiers, "k") => {
                    Some(Message::ShortcutPressed(Shortcut::OpenCommandPalette))
                }
                (key, text) if is_ctrl_character(key, text, modifiers, "c") => {
                    Some(Message::ShortcutPressed(Shortcut::Copy))
                }
                (key, text)
                    if status != event::Status::Captured
                        && is_ctrl_character(key, text, modifiers, "x") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::Cut))
                }
                (key, text)
                    if status != event::Status::Captured
                        && is_ctrl_character(key, text, modifiers, "v") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::Paste))
                }
                (key, text)
                    if status != event::Status::Captured
                        && modifiers.control()
                        && modifiers.shift()
                        && is_ctrl_character(key, text, modifiers, "z") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::Redo))
                }
                (key, text)
                    if status != event::Status::Captured
                        && is_ctrl_character(key, text, modifiers, "z") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::Undo))
                }
                (key, text)
                    if status != event::Status::Captured
                        && is_ctrl_character(key, text, modifiers, "y") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::Redo))
                }
                (key, text) if is_ctrl_character(key, text, modifiers, "f") => {
                    Some(Message::ShortcutPressed(Shortcut::FocusSearch))
                }
                (key, text)
                    if status != event::Status::Captured
                        && modifiers.control()
                        && modifiers.alt()
                        && is_ctrl_character(key, text, modifiers, "m") =>
                {
                    Some(Message::ShortcutPressed(Shortcut::AddAnnotation))
                }
                (&keyboard::Key::Named(keyboard::key::Named::F3), _) if modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::FindPrevious))
                }
                (&keyboard::Key::Named(keyboard::key::Named::F3), _) => {
                    Some(Message::ShortcutPressed(Shortcut::FindNext))
                }
                (_, Some("i") | Some("I")) if status != event::Status::Captured => {
                    Some(Message::ShortcutPressed(Shortcut::ToggleLibraryInspector))
                }
                (_, Some("b") | Some("B")) if status != event::Status::Captured => {
                    Some(Message::ShortcutPressed(Shortcut::ToggleLibrarySidebar))
                }
                (_, Some("a") | Some("A")) if modifiers.control() => {
                    Some(Message::ShortcutPressed(Shortcut::SelectAll))
                }
                (_, Some("+") | Some("=")) => Some(Message::ShortcutPressed(Shortcut::In)),
                (_, Some("-")) => Some(Message::ShortcutPressed(Shortcut::Out)),
                (&keyboard::Key::Named(keyboard::key::Named::Enter), _) => {
                    Some(Message::ShortcutPressed(Shortcut::OpenSelected))
                }
                (&keyboard::Key::Named(keyboard::key::Named::Delete), _) => {
                    Some(Message::ShortcutPressed(Shortcut::DeleteSelected))
                }
                (&keyboard::Key::Named(keyboard::key::Named::F2), _) => {
                    Some(Message::ShortcutPressed(Shortcut::RenameSelected))
                }
                (key, _) if key_is_character(key, "0") => {
                    Some(Message::ShortcutPressed(Shortcut::Reset))
                }
                (&keyboard::Key::Named(keyboard::key::Named::Space), _) if modifiers.shift() => {
                    Some(Message::ShortcutPressed(Shortcut::PageUp))
                }
                (&keyboard::Key::Named(keyboard::key::Named::Space), _) => {
                    Some(Message::ShortcutPressed(Shortcut::PageDown))
                }
                (&keyboard::Key::Named(keyboard::key::Named::PageDown), _) => {
                    Some(Message::ShortcutPressed(Shortcut::PageDown))
                }
                (&keyboard::Key::Named(keyboard::key::Named::PageUp), _) => {
                    Some(Message::ShortcutPressed(Shortcut::PageUp))
                }
                (&keyboard::Key::Named(keyboard::key::Named::Home), _) => {
                    Some(Message::ShortcutPressed(Shortcut::DocumentStart))
                }
                (&keyboard::Key::Named(keyboard::key::Named::End), _) => {
                    Some(Message::ShortcutPressed(Shortcut::DocumentEnd))
                }
                (&keyboard::Key::Named(keyboard::key::Named::ArrowDown), _) => {
                    Some(Message::ShortcutPressed(Shortcut::FineScroll(64)))
                }
                (&keyboard::Key::Named(keyboard::key::Named::ArrowUp), _) => {
                    Some(Message::ShortcutPressed(Shortcut::FineScroll(-64)))
                }
                (&keyboard::Key::Named(keyboard::key::Named::ArrowRight), _) => {
                    Some(Message::ShortcutPressed(Shortcut::HorizontalPan(96)))
                }
                (&keyboard::Key::Named(keyboard::key::Named::ArrowLeft), _) => {
                    Some(Message::ShortcutPressed(Shortcut::HorizontalPan(-96)))
                }
                (&keyboard::Key::Named(keyboard::key::Named::Escape), _) => {
                    Some(Message::ShortcutPressed(Shortcut::Escape))
                }
                _ => None,
            }
        }
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        _ => None,
    }
}

/// True when Control is held and the key/text is the given character (case-insensitive).
fn is_ctrl_character(
    key: &keyboard::Key,
    text: Option<&str>,
    modifiers: keyboard::Modifiers,
    target: &str,
) -> bool {
    modifiers.control()
        && (key_is_character(key, target)
            || text.is_some_and(|text| text.eq_ignore_ascii_case(target)))
}

/// True when `key` is a character key matching `target` (case-insensitive).
fn key_is_character(key: &keyboard::Key, target: &str) -> bool {
    match key {
        keyboard::Key::Character(value) => value.eq_ignore_ascii_case(target),
        _ => false,
    }
}

/// True when `key` is the Escape named key.
fn is_escape(key: &keyboard::Key) -> bool {
    matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
}

// Shortcut action handling lives with keyboard shortcut mapping.

/// Applies a recognized [`Shortcut`] to `app`, returning any follow-up tasks.
///
/// When the command palette is open, arrow-like fine scroll and Enter/Escape
/// navigate the palette instead of the underlying surface. Otherwise handles
/// zoom, theme, page scroll, library selection, clipboard, find, jump, and
/// undo/redo depending on the shortcut and current [`AppMode`].
pub(crate) fn handle_shortcut(app: &mut PDFolioApp, shortcut: Shortcut) -> Task<Message> {
    if app.chrome.command_palette_open {
        return match shortcut {
            Shortcut::FineScroll(delta) if delta > 0 => {
                Task::done(Message::CommandPaletteMoveSelection(1))
            }
            Shortcut::FineScroll(delta) if delta < 0 => {
                Task::done(Message::CommandPaletteMoveSelection(-1))
            }
            Shortcut::OpenSelected => Task::done(Message::CommandPaletteRunSelected),
            Shortcut::Escape => {
                app.chrome.command_palette_open = false;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
                Task::none()
            }
            _ => Task::none(),
        };
    }

    // Mockup: ↑ / ↓ step comments when notes exist (document still scrolls via wheel).
    if app.mode == AppMode::Viewer
        && !app.viewer.annotations.is_empty()
        && !app.viewer.has_annotation_draft()
        && !app.viewer.viewer_find.open
    {
        match shortcut {
            Shortcut::FineScroll(delta) if delta > 0 => {
                return app.annotation_select_next();
            }
            Shortcut::FineScroll(delta) if delta < 0 => {
                return app.annotation_select_previous();
            }
            _ => {}
        }
    }

    match shortcut {
        Shortcut::In => {
            app.viewer.active_zoom_preset = None;
            with_session_save(
                app.zoom_to_width(
                    crate::viewer::rendering::zoom_in_width(app.viewer.zoom_width),
                    None,
                    ZoomRenderPolicy::Immediate,
                ),
                app,
            )
        }
        Shortcut::Out => {
            app.viewer.active_zoom_preset = None;
            with_session_save(
                app.zoom_to_width(
                    crate::viewer::rendering::zoom_out_width(app.viewer.zoom_width),
                    None,
                    ZoomRenderPolicy::Immediate,
                ),
                app,
            )
        }
        Shortcut::Reset => {
            app.viewer.active_zoom_preset = Some(ZoomPreset::Automatic);
            let width = ZoomPreset::Automatic.width_for(app);
            with_session_save(
                app.zoom_to_width(width, None, ZoomRenderPolicy::Immediate),
                app,
            )
        }
        Shortcut::ToggleTheme => {
            app.appearance.theme = app.appearance.theme.toggled();
            Task::none()
        }
        Shortcut::ReloadStyles => Task::done(Message::ReloadStyles),
        Shortcut::PageDown => {
            if app.mode == AppMode::Viewer
                && app.viewer.viewer_scroll_mode == ViewerScrollMode::Page
            {
                let task = app.scroll_page_mode_by(1);
                return Task::batch([
                    with_session_save_debounced(task, app),
                    app.schedule_reading_progress_save(),
                ]);
            }
            let task = app.scroll_by(app.viewer.viewer_viewport_height * 0.86);
            Task::batch([
                with_session_save_debounced(task, app),
                app.schedule_reading_progress_save(),
            ])
        }
        Shortcut::PageUp => {
            if app.mode == AppMode::Viewer
                && app.viewer.viewer_scroll_mode == ViewerScrollMode::Page
            {
                let task = app.scroll_page_mode_by(-1);
                return Task::batch([
                    with_session_save_debounced(task, app),
                    app.schedule_reading_progress_save(),
                ]);
            }
            let task = app.scroll_by(-(app.viewer.viewer_viewport_height * 0.86));
            Task::batch([
                with_session_save_debounced(task, app),
                app.schedule_reading_progress_save(),
            ])
        }
        Shortcut::DocumentStart => {
            if app.mode != AppMode::Viewer || app.viewer.doc.is_none() {
                return Task::none();
            }
            let task = app.jump_to_page(0);
            Task::batch([
                with_session_save(task, app),
                app.schedule_reading_progress_save(),
            ])
        }
        Shortcut::DocumentEnd => {
            let Some(doc) = app.viewer.doc.as_ref() else {
                return Task::none();
            };
            if app.mode != AppMode::Viewer {
                return Task::none();
            }
            let last = doc.page_count().saturating_sub(1);
            let task = app.jump_to_page(last);
            Task::batch([
                with_session_save(task, app),
                app.schedule_reading_progress_save(),
            ])
        }
        Shortcut::FineScroll(delta) => {
            if app.viewer.viewer_scroll_mode == ViewerScrollMode::Horizontal {
                let task = app.pan_horizontally_by(f32::from(delta));
                Task::batch([
                    with_session_save_debounced(task, app),
                    app.schedule_reading_progress_save(),
                ])
            } else {
                let task = app.scroll_by(f32::from(delta));
                Task::batch([
                    with_session_save_debounced(task, app),
                    app.schedule_reading_progress_save(),
                ])
            }
        }
        Shortcut::HorizontalPan(delta) => {
            let task = app.pan_horizontally_by(f32::from(delta));
            Task::batch([
                with_session_save_debounced(task, app),
                app.schedule_reading_progress_save(),
            ])
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
        Shortcut::FindNext => {
            if app.mode != AppMode::Viewer || app.viewer.doc.is_none() {
                return Task::none();
            }
            if !app.viewer.viewer_find.open {
                return app.open_viewer_find();
            }
            app.viewer.viewer_find.select_next();
            app.scroll_to_selected_viewer_find_match()
        }
        Shortcut::FindPrevious => {
            if app.mode != AppMode::Viewer || !app.viewer.viewer_find.open {
                return Task::none();
            }
            app.viewer.viewer_find.select_previous();
            app.scroll_to_selected_viewer_find_match()
        }
        Shortcut::RenameSelected => {
            if app.mode == AppMode::Library && app.library.selected_library_entries.len() == 1 {
                return operation::focus(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID));
            }
            if app.mode == AppMode::Library && app.library.details_folder_id.is_some() {
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
                if let Some(folder_id) = app.library.details_folder_id.clone() {
                    return Task::done(Message::RequestConfirmation(
                        ConfirmationAction::DeleteFolder(folder_id),
                    ));
                }
            }
            Task::none()
        }
        Shortcut::Cut => {
            if app.mode == AppMode::Library {
                Task::done(Message::CutLibrarySelection)
            } else {
                Task::none()
            }
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
            } else if app.mode == AppMode::Library {
                Task::done(Message::CopyLibrarySelection)
            } else {
                Task::none()
            }
        }
        Shortcut::Paste => {
            if app.mode == AppMode::Library {
                Task::done(Message::PasteLibraryClipboard)
            } else {
                Task::none()
            }
        }
        Shortcut::Undo => {
            if app.mode == AppMode::Library {
                Task::done(Message::UndoLibraryAction)
            } else {
                Task::none()
            }
        }
        Shortcut::Redo => {
            if app.mode == AppMode::Library {
                Task::done(Message::RedoLibraryAction)
            } else {
                Task::none()
            }
        }
        Shortcut::OpenCommandPalette => {
            if app.mode == AppMode::Library || app.mode == AppMode::Viewer {
                app.chrome.command_palette_open = true;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
                app.chrome.open_context_menu = None;
            }
            Task::none()
        }
        Shortcut::ToggleLibrarySidebar => {
            if app.mode == AppMode::Library {
                Task::done(Message::ToggleLibrarySidebar)
            } else {
                Task::none()
            }
        }
        Shortcut::ToggleLibraryInspector => {
            if app.mode == AppMode::Library {
                Task::done(Message::ToggleLibraryInspector)
            } else {
                Task::none()
            }
        }
        Shortcut::Escape => {
            if app.chrome.command_palette_open {
                app.chrome.command_palette_open = false;
                app.chrome.command_palette_query.clear();
                app.chrome.command_palette_selected_index = 0;
            } else if app.chrome.pending_confirmation.is_some() {
                app.chrome.pending_confirmation = None;
            } else if app.library.renaming_tag.is_some() {
                app.library.renaming_tag = None;
                app.library.tag_rename_input.clear();
            } else if app.viewer.zoom_menu_open {
                app.viewer.zoom_menu_open = false;
            } else if app.viewer.visibility_menu_open {
                app.viewer.visibility_menu_open = false;
            } else if app.viewer.zoom_editing {
                app.viewer.zoom_editing = false;
                app.viewer.zoom_input = zoom_percent_label(app.viewer.zoom_width);
            } else if app.viewer.page_input_editing {
                app.viewer.page_input_editing = false;
                app.viewer.jump_input.clear();
            } else if app.mode == AppMode::Viewer && app.viewer.has_annotation_draft() {
                app.cancel_annotation_drafts();
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
            } else if app.mode == AppMode::Viewer && app.viewer.toc_open {
                app.viewer.toc_open = false;
                app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
                return with_session_save(app.apply_active_dimension_zoom(), app);
            } else if app.mode == AppMode::Viewer && app.viewer.doc.is_some() {
                return Task::done(Message::BackToLibrary);
            }
            Task::none()
        }
        Shortcut::AddAnnotation => {
            if app.mode == AppMode::Viewer {
                app.start_annotation_compose()
            } else {
                Task::none()
            }
        }
    }
}

/// Unit tests for global shortcut capture and single-key library chords.
#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key;

    #[test]
    fn ctrl_f_opens_search_even_when_event_is_captured() {
        let message = keyboard_event_message(ctrl_key_event("f", None), event::Status::Captured);

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::FocusSearch))
        ));
    }

    #[test]
    fn ctrl_c_copies_even_when_event_is_captured() {
        let message = keyboard_event_message(ctrl_key_event("c", None), event::Status::Captured);

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::Copy))
        ));
    }

    #[test]
    fn ctrl_k_opens_palette_even_when_event_is_captured() {
        let message = keyboard_event_message(ctrl_key_event("k", None), event::Status::Captured);

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::OpenCommandPalette))
        ));
    }

    #[test]
    fn captured_non_ctrl_shortcuts_are_ignored() {
        let message = keyboard_event_message(
            key_event("f", None, keyboard::Modifiers::default()),
            event::Status::Captured,
        );

        assert!(message.is_none());
    }

    #[test]
    fn i_toggles_library_inspector_when_event_is_not_captured() {
        let message = keyboard_event_message(
            key_event("i", Some("i"), keyboard::Modifiers::default()),
            event::Status::Ignored,
        );

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::ToggleLibraryInspector))
        ));
    }

    #[test]
    fn i_does_not_toggle_library_inspector_when_event_is_captured() {
        let message = keyboard_event_message(
            key_event("i", Some("i"), keyboard::Modifiers::default()),
            event::Status::Captured,
        );

        assert!(message.is_none());
    }

    #[test]
    fn b_toggles_library_sidebar_when_event_is_not_captured() {
        let message = keyboard_event_message(
            key_event("b", Some("b"), keyboard::Modifiers::default()),
            event::Status::Ignored,
        );

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::ToggleLibrarySidebar))
        ));
    }

    #[test]
    fn b_does_not_toggle_library_sidebar_when_event_is_captured() {
        let message = keyboard_event_message(
            key_event("b", Some("b"), keyboard::Modifiers::default()),
            event::Status::Captured,
        );

        assert!(message.is_none());
    }

    #[test]
    fn escape_closes_overlays_even_when_event_is_captured() {
        let message = keyboard_event_message(escape_event(), event::Status::Captured);

        assert!(matches!(
            message,
            Some(Message::ShortcutPressed(Shortcut::Escape))
        ));
    }

    fn ctrl_key_event(character: &'static str, text: Option<&'static str>) -> Event {
        key_event(character, text, keyboard::Modifiers::CTRL)
    }

    fn key_event(
        character: &'static str,
        text: Option<&'static str>,
        modifiers: keyboard::Modifiers,
    ) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(character.into()),
            modified_key: keyboard::Key::Character(character.into()),
            physical_key: key::Physical::Code(key::Code::KeyF),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        })
    }

    fn escape_event() -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: key::Physical::Code(key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text: None,
            repeat: false,
        })
    }
}
