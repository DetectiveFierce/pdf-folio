//! Keyboard shortcut mapping for the application shell.

use iced::{event, keyboard, Event};

use crate::messages::{Message, Shortcut};

pub(crate) fn keyboard_event_message(event: Event, status: event::Status) -> Option<Message> {
    if status == event::Status::Captured {
        return None;
    }

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
        }) => match (key, text.as_deref()) {
            (_, Some("t") | Some("T")) if modifiers.control() && modifiers.shift() => {
                Some(Message::ShortcutPressed(Shortcut::ToggleTheme))
            }
            (_, Some("r") | Some("R")) if modifiers.control() && modifiers.shift() => {
                Some(Message::ShortcutPressed(Shortcut::ReloadStyles))
            }
            (_, Some("g") | Some("G")) if modifiers.control() => {
                Some(Message::ShortcutPressed(Shortcut::Jump))
            }
            (_, Some("f") | Some("F")) if modifiers.control() => {
                Some(Message::ShortcutPressed(Shortcut::FocusSearch))
            }
            (_, Some("a") | Some("A")) if modifiers.control() => {
                Some(Message::ShortcutPressed(Shortcut::SelectAll))
            }
            (_, Some("+") | Some("=")) => Some(Message::ShortcutPressed(Shortcut::In)),
            (_, Some("-")) => Some(Message::ShortcutPressed(Shortcut::Out)),
            (keyboard::Key::Named(keyboard::key::Named::Enter), _) => {
                Some(Message::ShortcutPressed(Shortcut::OpenSelected))
            }
            (keyboard::Key::Named(keyboard::key::Named::Delete), _) => {
                Some(Message::ShortcutPressed(Shortcut::DeleteSelected))
            }
            (keyboard::Key::Named(keyboard::key::Named::F2), _) => {
                Some(Message::ShortcutPressed(Shortcut::RenameSelected))
            }
            (keyboard::Key::Character(value), _) if value.as_str() == "0" => {
                Some(Message::ShortcutPressed(Shortcut::Reset))
            }
            (keyboard::Key::Named(keyboard::key::Named::Space), _) if modifiers.shift() => {
                Some(Message::ShortcutPressed(Shortcut::PageUp))
            }
            (keyboard::Key::Named(keyboard::key::Named::Space), _) => {
                Some(Message::ShortcutPressed(Shortcut::PageDown))
            }
            (keyboard::Key::Named(keyboard::key::Named::ArrowDown), _) => {
                Some(Message::ShortcutPressed(Shortcut::FineScroll(64)))
            }
            (keyboard::Key::Named(keyboard::key::Named::ArrowUp), _) => {
                Some(Message::ShortcutPressed(Shortcut::FineScroll(-64)))
            }
            (keyboard::Key::Named(keyboard::key::Named::ArrowRight), _) => {
                Some(Message::ShortcutPressed(Shortcut::HorizontalPan(96)))
            }
            (keyboard::Key::Named(keyboard::key::Named::ArrowLeft), _) => {
                Some(Message::ShortcutPressed(Shortcut::HorizontalPan(-96)))
            }
            (keyboard::Key::Named(keyboard::key::Named::Escape), _) => {
                Some(Message::ShortcutPressed(Shortcut::Escape))
            }
            _ => None,
        },
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::ModifiersChanged(modifiers))
        }
        _ => None,
    }
}
