//! Keyboard shortcut mapping for the application shell.

use iced::{event, keyboard, Event};

use crate::messages::{Message, Shortcut};

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
            if status == event::Status::Captured
                && !is_ctrl_character(&key, text.as_deref(), modifiers, "f")
                && !is_ctrl_character(&key, text.as_deref(), modifiers, "c")
                && !is_escape(&key)
            {
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
                (key, text) if is_ctrl_character(key, text, modifiers, "c") => {
                    Some(Message::ShortcutPressed(Shortcut::Copy))
                }
                (key, text) if is_ctrl_character(key, text, modifiers, "f") => {
                    Some(Message::ShortcutPressed(Shortcut::FocusSearch))
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

fn key_is_character(key: &keyboard::Key, target: &str) -> bool {
    match key {
        keyboard::Key::Character(value) => value.eq_ignore_ascii_case(target),
        _ => false,
    }
}

fn is_escape(key: &keyboard::Key) -> bool {
    matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
}

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
    fn captured_non_ctrl_shortcuts_are_ignored() {
        let message = keyboard_event_message(
            key_event("f", None, keyboard::Modifiers::default()),
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
