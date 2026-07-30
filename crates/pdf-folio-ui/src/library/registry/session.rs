//! # Registry session persistence
//!
//! Reads and writes `libraries.json` under the app data directory, creates
//! per-library SQLite files, and removes storage when a vault is deleted.
//!
//! ## Ownership
//!
//! Pure filesystem + serde relative to a `LibraryRegistryRuntime` value.
//! Does not touch iced widgets. Callers (UI update path, startup, sync
//! tasks) pass ownership of the runtime in and receive an updated value.
//!
//! Default library id uses `library.db` at the data root; additional
//! libraries live under `libraries/<id>/library.db`.

use crate::library::registry::preview::load_library_previews;
use crate::library::registry::state::{
    StoredLibraryRegistry, DEFAULT_LIBRARY_ID, DEFAULT_LIBRARY_NAME,
};
use crate::*;
use anyhow::Context;
use directories::ProjectDirs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Load or initialize `libraries.json`, ensure a default profile, and attach previews.
pub(crate) fn load_library_registry(
    preferred_active_id: Option<&str>,
) -> anyhow::Result<LibraryRegistryRuntime> {
    let data_dir = app_data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("Could not create {}.", data_dir.display()))?;
    let path = registry_path()?;
    let stored = if path.exists() {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("Could not read {}.", path.display()))?;
        serde_json::from_str::<StoredLibraryRegistry>(&json)
            .with_context(|| format!("Could not parse {}.", path.display()))?
    } else {
        default_registry(&data_dir)
    };
    let mut registry = LibraryRegistryRuntime::from_stored(stored, preferred_active_id);
    if registry.profiles.is_empty() {
        registry.profiles.push(default_profile(&data_dir));
        registry.active_library_id = DEFAULT_LIBRARY_ID.to_owned();
    }
    registry.profiles.sort_by(|left, right| {
        (left.id != DEFAULT_LIBRARY_ID)
            .cmp(&(right.id != DEFAULT_LIBRARY_ID))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    registry.rename_inputs = registry
        .profiles
        .iter()
        .map(|profile| (profile.id.clone(), profile.name.clone()))
        .collect();
    registry.previews = load_library_previews(&registry.profiles);
    save_library_registry(&registry)?;
    Ok(registry)
}

/// Create a new vault SQLite file and profile, make it active, and save the registry.
pub(crate) fn create_library_profile(
    registry: LibraryRegistryRuntime,
    name: String,
) -> anyhow::Result<LibraryRegistryRuntime> {
    let mut registry = registry;
    let name = clean_library_name(&name)?;
    let id = unique_library_id(&registry);
    let db_path = library_db_path(&app_data_dir()?, &id);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    Db::open(&db_path)?;
    registry.profiles.push(LibraryProfile {
        id: id.clone(),
        name: name.clone(),
        db_path,
    });
    registry.active_library_id = id.clone();
    registry.new_library_name.clear();
    registry.rename_inputs.insert(id, name);
    registry.open_menu_library_id = None;
    registry.name_dialog = None;
    registry.previews = load_library_previews(&registry.profiles);
    save_library_registry(&registry)?;
    Ok(registry)
}

/// Rename a profile in the registry (does not move the database file).
pub(crate) fn rename_library_profile(
    registry: LibraryRegistryRuntime,
    library_id: String,
    name: String,
) -> anyhow::Result<LibraryRegistryRuntime> {
    let mut registry = registry;
    let name = clean_library_name(&name)?;
    let Some(profile) = registry
        .profiles
        .iter_mut()
        .find(|profile| profile.id == library_id)
    else {
        anyhow::bail!("Library was not found.");
    };
    profile.name = name.clone();
    registry.rename_inputs.insert(library_id, name);
    registry.open_menu_library_id = None;
    registry.name_dialog = None;
    registry.previews = load_library_previews(&registry.profiles);
    save_library_registry(&registry)?;
    Ok(registry)
}

/// Remove a profile (keeping at least one), delete its SQLite storage, and save.
pub(crate) fn delete_library_profile(
    registry: LibraryRegistryRuntime,
    library_id: String,
) -> anyhow::Result<LibraryRegistryRuntime> {
    let mut registry = registry;
    if registry.profiles.len() <= 1 {
        anyhow::bail!("At least one library must remain.");
    }
    let Some(index) = registry
        .profiles
        .iter()
        .position(|profile| profile.id == library_id)
    else {
        anyhow::bail!("Library was not found.");
    };
    let removed = registry.profiles.remove(index);
    registry.deleted_library_ids.insert(library_id.clone());
    registry.rename_inputs.remove(&library_id);
    if registry.active_library_id == library_id {
        registry.active_library_id = registry
            .profiles
            .first()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| DEFAULT_LIBRARY_ID.to_owned());
    }
    registry.open_menu_library_id = None;
    registry.name_dialog = None;
    registry.previews = load_library_previews(&registry.profiles);
    save_library_registry(&registry)?;
    remove_library_storage(&removed.db_path)?;
    Ok(registry)
}

/// Serialize the registry runtime to `libraries.json`.
pub(super) fn save_library_registry(registry: &LibraryRegistryRuntime) -> anyhow::Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    let stored = StoredLibraryRegistry {
        active_library_id: registry.active_library_id.clone(),
        libraries: registry.profiles.clone(),
        deleted_library_ids: registry.deleted_library_ids.iter().cloned().collect(),
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&stored)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

/// Fresh on-disk registry with only the default library profile under `data_dir`.
fn default_registry(data_dir: &Path) -> StoredLibraryRegistry {
    StoredLibraryRegistry {
        active_library_id: DEFAULT_LIBRARY_ID.to_owned(),
        libraries: vec![default_profile(data_dir)],
        deleted_library_ids: Vec::new(),
    }
}

/// Built-in default vault profile (`DEFAULT_LIBRARY_ID` / `DEFAULT_LIBRARY_NAME`).
fn default_profile(data_dir: &Path) -> LibraryProfile {
    LibraryProfile {
        id: DEFAULT_LIBRARY_ID.to_owned(),
        name: DEFAULT_LIBRARY_NAME.to_owned(),
        db_path: library_db_path(data_dir, DEFAULT_LIBRARY_ID),
    }
}

/// Whole seconds since UNIX epoch for sync metadata.
pub(super) fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

/// mtime of `path` as UNIX seconds, if available.
pub(super) fn file_modified_unix_timestamp(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
}

/// Resolve the SQLite path for a library id under `data_dir`.
pub(super) fn library_db_path(data_dir: &Path, library_id: &str) -> PathBuf {
    if library_id == DEFAULT_LIBRARY_ID {
        data_dir.join("library.db")
    } else {
        data_dir
            .join("libraries")
            .join(library_id)
            .join("library.db")
    }
}

/// Platform app data directory for PDF-Folio (`directories` crate).
pub(super) fn app_data_dir() -> anyhow::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().to_path_buf())
}

/// Full path to `libraries.json`.
pub(super) fn registry_path() -> anyhow::Result<PathBuf> {
    Ok(app_data_dir()?.join("libraries.json"))
}

/// Trim, strip controls, and cap a library display name at 80 characters (errors if empty).
fn clean_library_name(name: &str) -> anyhow::Result<String> {
    let name = name
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_owned();
    if name.is_empty() {
        anyhow::bail!("Library name cannot be empty.");
    }
    Ok(name.chars().take(80).collect())
}

/// Generate a unique `library-{millis}` id not already present in `registry.profiles`.
fn unique_library_id(registry: &LibraryRegistryRuntime) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let mut id = format!("library-{millis}");
    let mut suffix = 1;
    while registry.profiles.iter().any(|profile| profile.id == id) {
        id = format!("library-{millis}-{suffix}");
        suffix += 1;
    }
    id
}

/// Delete a library database file and its empty parent under `libraries/`.
pub(super) fn remove_library_storage(db_path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(db_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("Could not remove {}.", db_path.display()));
        }
    }

    let data_dir = app_data_dir()?;
    if let Some(parent) = db_path.parent() {
        if parent.starts_with(data_dir.join("libraries")) {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
    Ok(())
}
