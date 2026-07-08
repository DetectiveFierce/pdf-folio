use std::path::{Path, PathBuf};

pub(crate) const BUNDLED_STYLE_FILES: [(&str, &str); 7] = [
    (
        "styles/themes/espresso.kdl",
        include_str!("../../styles/themes/espresso.kdl"),
    ),
    (
        "styles/themes/light.kdl",
        include_str!("../../styles/themes/light.kdl"),
    ),
    (
        "styles/components/core.kdl",
        include_str!("../../styles/components/core.kdl"),
    ),
    (
        "styles/components/library/sidebar.kdl",
        include_str!("../../styles/components/library/sidebar.kdl"),
    ),
    (
        "styles/components/library/library.kdl",
        include_str!("../../styles/components/library/library.kdl"),
    ),
    (
        "styles/components/viewer/viewer.kdl",
        include_str!("../../styles/components/viewer/viewer.kdl"),
    ),
    (
        "styles/application.kdl",
        include_str!("../../styles/application.kdl"),
    ),
];

pub(crate) fn user_style_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("pdf-folio").join("styles"))
}

pub(super) fn bundled_style_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("styles")
}

pub(crate) fn style_source_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let bundled = bundled_style_dir();
    if bundled.exists() {
        dirs.push(bundled);
    }
    if let Some(user) = user_style_dir().filter(|path| path.exists()) {
        dirs.push(user);
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

pub(crate) fn bundled_style_sources() -> Result<Vec<(String, String)>, String> {
    let bundled_dir = bundled_style_dir();
    let disk_files = style_files_in_dir(&bundled_dir);
    let mut sources = Vec::new();

    for (relative, fallback) in BUNDLED_STYLE_FILES {
        let relative_path = relative.strip_prefix("styles/").unwrap_or(relative);
        let path = bundled_dir.join(relative_path);
        if path.exists() {
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            sources.push((path.display().to_string(), source));
        } else {
            sources.push((relative.to_owned(), fallback.to_owned()));
        }
    }

    for path in disk_files {
        if bundled_style_relative_path(&bundled_dir, &path).is_some_and(|relative| {
            BUNDLED_STYLE_FILES
                .iter()
                .any(|(bundled, _)| bundled.strip_prefix("styles/").unwrap_or(bundled) == relative)
        }) {
            continue;
        }

        let source = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        sources.push((path.display().to_string(), source));
    }

    Ok(sources)
}

pub(crate) fn user_style_files(dir: &Path) -> Vec<PathBuf> {
    style_files_in_dir(dir)
}

pub(super) fn style_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_kdl_files(dir, &mut files);
    files.sort();
    files.sort_by_key(|path| style_file_order_key(dir, path));
    files
}

fn collect_kdl_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kdl"))
        {
            files.push(path.to_path_buf());
        }
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        collect_kdl_files(&entry.path(), files);
    }
}

fn style_file_order_key(root: &Path, path: &Path) -> (u8, PathBuf) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let first_component = relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str());
    let file_stem = relative.file_stem().and_then(|stem| stem.to_str());
    let group = match (first_component, file_stem) {
        (Some("themes"), _) | (_, Some("theme" | "themes")) => 0,
        (Some("components"), _) | (_, Some("component" | "components")) => 1,
        (Some("application.kdl"), _) | (_, Some("application")) => 2,
        _ => 3,
    };
    (group, relative.to_path_buf())
}

fn bundled_style_relative_path<'a>(root: &'a Path, path: &'a Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .and_then(|path| path.to_str())
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
}
