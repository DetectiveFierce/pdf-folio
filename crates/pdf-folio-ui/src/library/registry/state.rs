use crate::*;

pub(crate) const DEFAULT_LIBRARY_ID: &str = "default";
pub(crate) const DEFAULT_LIBRARY_NAME: &str = "Default Library";
pub(crate) const LIBRARY_SWITCHER_PREVIEW_LIMIT: usize = 12;

/// A discrete user library backed by its own SQLite database.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LibraryProfile {
    pub id: String,
    pub name: String,
    pub db_path: PathBuf,
}

/// Runtime state for the library switcher and registry-backed libraries.
#[derive(Debug, Clone)]
pub struct LibraryRegistryRuntime {
    pub profiles: Vec<LibraryProfile>,
    pub active_library_id: String,
    pub deleted_library_ids: HashSet<String>,
    pub previews: HashMap<String, LibraryPreview>,
    pub new_library_name: String,
    pub rename_inputs: HashMap<String, String>,
    pub open_menu_library_id: Option<String>,
    pub name_dialog: Option<LibraryNameDialog>,
}

/// Small preview payload used by the library switcher cards.
#[derive(Debug, Clone, Default)]
pub struct LibraryPreview {
    pub total_entries: usize,
    pub thumbnails: Vec<LibraryPreviewThumbnail>,
}

/// One real PDF cover thumbnail shown in a library switcher preview.
#[derive(Debug, Clone)]
pub struct LibraryPreviewThumbnail {
    pub title: String,
    pub width: u16,
    pub height: u16,
    pub handle: image::Handle,
}

/// Modal mode for creating or renaming a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryNameDialog {
    Create,
    Rename(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct StoredLibraryRegistry {
    pub(super) active_library_id: String,
    pub(super) libraries: Vec<LibraryProfile>,
    #[serde(default)]
    pub(super) deleted_library_ids: Vec<String>,
}

impl LibraryRegistryRuntime {
    pub fn active_profile(&self) -> Option<&LibraryProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_library_id)
    }

    pub(super) fn from_stored(
        stored: StoredLibraryRegistry,
        preferred_active_id: Option<&str>,
    ) -> Self {
        let active_library_id = preferred_active_id
            .filter(|id| stored.libraries.iter().any(|profile| profile.id == *id))
            .or_else(|| {
                stored
                    .libraries
                    .iter()
                    .any(|profile| profile.id == stored.active_library_id)
                    .then_some(stored.active_library_id.as_str())
            })
            .or_else(|| stored.libraries.first().map(|profile| profile.id.as_str()))
            .unwrap_or(DEFAULT_LIBRARY_ID)
            .to_owned();
        let rename_inputs = stored
            .libraries
            .iter()
            .map(|profile| (profile.id.clone(), profile.name.clone()))
            .collect();

        Self {
            profiles: stored.libraries,
            active_library_id,
            deleted_library_ids: stored.deleted_library_ids.into_iter().collect(),
            previews: HashMap::new(),
            new_library_name: String::new(),
            rename_inputs,
            open_menu_library_id: None,
            name_dialog: None,
        }
    }
}
