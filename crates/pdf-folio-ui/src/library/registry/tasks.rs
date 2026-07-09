use crate::library::registry::preview::load_library_previews;
use crate::library::registry::session::{
    app_data_dir, current_unix_timestamp, file_modified_unix_timestamp, library_db_path,
    registry_path, remove_library_storage, save_library_registry,
};
use crate::library::registry::state::DEFAULT_LIBRARY_ID;
use crate::*;
use anyhow::Context;
use pdf_folio_cloud::sync::SyncLibraryRow;

pub(crate) fn sync_library_registry_profiles(
    registry: LibraryRegistryRuntime,
    remote_libraries: Vec<SyncLibraryRow>,
) -> anyhow::Result<(LibraryRegistryRuntime, Vec<String>)> {
    let mut registry = registry;
    let data_dir = app_data_dir()?;
    let mut added_library_ids = Vec::new();

    for remote in remote_libraries {
        if remote.deleted_at.is_some() {
            registry.deleted_library_ids.insert(remote.id.clone());
            if let Some(index) = registry
                .profiles
                .iter()
                .position(|profile| profile.id == remote.id)
            {
                let removed = registry.profiles.remove(index);
                registry.rename_inputs.remove(&remote.id);
                let _ = remove_library_storage(&removed.db_path);
                if registry.active_library_id == remote.id {
                    registry.active_library_id = registry
                        .profiles
                        .first()
                        .map(|profile| profile.id.clone())
                        .unwrap_or_else(|| DEFAULT_LIBRARY_ID.to_owned());
                }
            }
            continue;
        }

        registry.deleted_library_ids.remove(&remote.id);

        if let Some(profile) = registry
            .profiles
            .iter_mut()
            .find(|profile| profile.id == remote.id)
        {
            if profile.name != remote.name {
                profile.name = remote.name.clone();
            }
            registry
                .rename_inputs
                .insert(profile.id.clone(), profile.name.clone());
            continue;
        }

        let db_path = library_db_path(&data_dir, &remote.id);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Could not create {}.", parent.display()))?;
        }
        Db::open(&db_path)?;
        registry.profiles.push(LibraryProfile {
            id: remote.id.clone(),
            name: remote.name.clone(),
            db_path,
        });
        registry
            .rename_inputs
            .insert(remote.id.clone(), remote.name);
        added_library_ids.push(remote.id);
    }

    registry.profiles.sort_by(|left, right| {
        (left.id != DEFAULT_LIBRARY_ID)
            .cmp(&(right.id != DEFAULT_LIBRARY_ID))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    registry.previews = load_library_previews(&registry.profiles);
    save_library_registry(&registry)?;
    Ok((registry, added_library_ids))
}

pub(crate) fn sync_library_rows_for_registry(
    registry: &LibraryRegistryRuntime,
) -> Vec<SyncLibraryRow> {
    let registry_updated_at = registry_path()
        .ok()
        .and_then(|path| file_modified_unix_timestamp(&path))
        .unwrap_or_else(current_unix_timestamp);
    registry
        .profiles
        .iter()
        .map(|profile| SyncLibraryRow {
            id: profile.id.clone(),
            name: profile.name.clone(),
            updated_at: file_modified_unix_timestamp(&profile.db_path)
                .unwrap_or(registry_updated_at),
            deleted_at: None,
        })
        .chain(
            registry
                .deleted_library_ids
                .iter()
                .map(|library_id| SyncLibraryRow {
                    id: library_id.clone(),
                    name: library_id.clone(),
                    updated_at: registry_updated_at,
                    deleted_at: Some(registry_updated_at),
                }),
        )
        .collect()
}
