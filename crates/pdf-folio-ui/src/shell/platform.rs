//! Platform integration helpers used by the UI.

use std::path::Path;

#[cfg(test)]
pub(crate) fn file_manager_command(path: &Path, reveal: bool) -> Option<(String, Vec<String>)> {
    file_manager_commands(path, reveal).into_iter().next()
}

pub(crate) fn file_manager_commands(path: &Path, reveal: bool) -> Vec<(String, Vec<String>)> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    if cfg!(target_os = "windows") {
        if reveal {
            vec![(
                String::from("explorer"),
                vec![format!("/select,{}", path.display())],
            )]
        } else {
            vec![(String::from("explorer"), vec![parent.display().to_string()])]
        }
    } else if cfg!(target_os = "macos") {
        if reveal {
            vec![(
                String::from("open"),
                vec![String::from("-R"), path.display().to_string()],
            )]
        } else {
            vec![(String::from("open"), vec![parent.display().to_string()])]
        }
    } else if reveal {
        vec![
            (
                String::from("dbus-send"),
                vec![
                    String::from("--session"),
                    String::from("--dest=org.freedesktop.FileManager1"),
                    String::from("--type=method_call"),
                    String::from("/org/freedesktop/FileManager1"),
                    String::from("org.freedesktop.FileManager1.ShowItems"),
                    format!("array:string:{}", file_uri(path)),
                    String::from("string:"),
                ],
            ),
            (
                String::from("nautilus"),
                vec![String::from("--select"), path.display().to_string()],
            ),
            (
                String::from("dolphin"),
                vec![String::from("--select"), path.display().to_string()],
            ),
            (String::from("xdg-open"), vec![parent.display().to_string()]),
        ]
    } else {
        vec![(String::from("xdg-open"), vec![parent.display().to_string()])]
    }
}

pub(crate) fn file_uri(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut uri = String::from("file://");
    for byte in path.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(char::from(*byte))
            }
            byte => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}
