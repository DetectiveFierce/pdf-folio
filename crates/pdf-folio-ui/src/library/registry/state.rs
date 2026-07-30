//! # Registry runtime types
//!
//! Data structures for multi-library profiles, switcher previews, and name
//! dialogs. No I/O — persistence lives in [`super::session`].
//!
//! [`LibraryRegistryRuntime`] is stored on `PDFolioApp` and updated after
//! every create/rename/delete/sync. [`StoredLibraryRegistry`] is the serde
//! shape written to disk (without UI-only fields like open menus).

use crate::*;

/// Stable id for the built-in default vault (`library.db` at data root).
pub(crate) const DEFAULT_LIBRARY_ID: &str = "default";
/// Default display name shown for the built-in vault.
pub(crate) const DEFAULT_LIBRARY_NAME: &str = "Default Library";
/// Max cover thumbnails fetched per library for switcher cards.
pub(crate) const LIBRARY_SWITCHER_PREVIEW_LIMIT: usize = 12;

/// A discrete user library backed by its own SQLite database.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LibraryProfile {
    /// Stable vault id (`"default"` or a generated slug). Persisted in `libraries.json`.
    pub id: String,
    /// User-visible display name shown in the switcher and header.
    pub name: String,
    /// Absolute path to this vault's SQLite file (`library.db` under the data dir).
    pub db_path: PathBuf,
}

/// Runtime state for the library switcher and registry-backed libraries.
///
/// Mounted on `PDFolioApp::libraries`. UI-only fields (`open_menu_library_id`,
/// `name_dialog`, draft rename inputs) are not written to disk — persistence
/// uses [`StoredLibraryRegistry`].
#[derive(Debug, Clone)]
pub struct LibraryRegistryRuntime {
    /// Known vault profiles, sorted with the default library first then by name.
    pub profiles: Vec<LibraryProfile>,
    /// Id of the vault currently open in the app (`Db` path matches this profile).
    pub active_library_id: String,
    /// Tombstone ids for vaults deleted locally or via sync (not re-created from cloud).
    pub deleted_library_ids: HashSet<String>,
    /// Switcher card payloads keyed by library id (entry count + cover strip).
    pub previews: HashMap<String, LibraryPreview>,
    /// Draft text for the “new library” name field before create commits.
    pub new_library_name: String,
    /// Per-profile rename drafts keyed by library id (seeded from `profiles`).
    pub rename_inputs: HashMap<String, String>,
    /// Library whose overflow/context menu is open in the switcher, if any.
    pub open_menu_library_id: Option<String>,
    /// Create/rename name dialog currently shown over the switcher, if any.
    pub name_dialog: Option<LibraryNameDialog>,
}

/// Small preview payload used by the library switcher cards.
#[derive(Debug, Clone, Default)]
pub struct LibraryPreview {
    /// Total PDF count in the vault (live or last-loaded listing).
    pub total_entries: usize,
    /// Up to [`LIBRARY_SWITCHER_PREVIEW_LIMIT`] cover thumbs for the card strip.
    pub thumbnails: Vec<LibraryPreviewThumbnail>,
}

/// One real PDF cover thumbnail shown in a library switcher preview.
#[derive(Debug, Clone)]
pub struct LibraryPreviewThumbnail {
    /// Display title of the source entry (fallback: file stem).
    pub title: String,
    /// Raster width in pixels (typically the Small thumbnail tier).
    pub width: u16,
    /// Raster height in pixels matching the rendered cover aspect.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels for the cover.
    pub handle: image::Handle,
}

/// Modal mode for creating or renaming a library in the switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryNameDialog {
    /// Prompt for a name of a brand-new vault (uses `new_library_name`).
    Create,
    /// Prompt to rename an existing vault; payload is the profile id being renamed.
    Rename(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// On-disk serde shape for `libraries.json` (profiles + deleted tombstones).
pub(super) struct StoredLibraryRegistry {
    /// Last active library id written to disk (restored on startup when still valid).
    pub(super) active_library_id: String,
    /// Full profile list serialized to `libraries.json`.
    pub(super) libraries: Vec<LibraryProfile>,
    /// Tombstone list of deleted library ids (defaults to empty for older files).
    #[serde(default)]
    pub(super) deleted_library_ids: Vec<String>,
}

impl LibraryRegistryRuntime {
    /// Returns the currently active library profile, if any.
    pub fn active_profile(&self) -> Option<&LibraryProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_library_id)
    }

    /// Build runtime state from disk JSON, honoring an optional preferred active id.
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
