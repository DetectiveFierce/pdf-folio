use super::*;
use anyhow::Context;
use directories::ProjectDirs;
use pdf_folio_db::thumbnail_path;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_LIBRARY_ID: &str = "default";
const DEFAULT_LIBRARY_NAME: &str = "Default Library";
const LIBRARY_SWITCHER_PREVIEW_LIMIT: usize = 12;

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
struct StoredLibraryRegistry {
    active_library_id: String,
    libraries: Vec<LibraryProfile>,
}

impl LibraryRegistryRuntime {
    pub fn active_profile(&self) -> Option<&LibraryProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.active_library_id)
    }

    fn from_stored(stored: StoredLibraryRegistry, preferred_active_id: Option<&str>) -> Self {
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
            previews: HashMap::new(),
            new_library_name: String::new(),
            rename_inputs,
            open_menu_library_id: None,
            name_dialog: None,
        }
    }
}

impl PDFolioApp {
    pub(super) fn active_library_name(&self) -> &str {
        self.libraries
            .active_profile()
            .map_or(DEFAULT_LIBRARY_NAME, |profile| profile.name.as_str())
    }

    pub(super) fn open_library_switcher(&mut self) {
        self.mode = AppMode::LibrarySwitcher;
        self.clear_library_transient_interactions();
        self.chrome.open_app_menu = None;
        self.chrome.open_selection_menu = None;
        self.chrome.open_context_menu = None;
        self.viewer.zoom_menu_open = false;
        self.libraries.open_menu_library_id = None;
    }

    pub(super) fn select_library(&mut self, library_id: String) -> anyhow::Result<Task<Message>> {
        let Some(profile) = self
            .libraries
            .profiles
            .iter()
            .find(|profile| profile.id == library_id)
            .cloned()
        else {
            anyhow::bail!("Library was not found.");
        };

        let db = Arc::new(Db::open(profile.db_path)?);
        self.libraries.active_library_id = profile.id.clone();
        save_library_registry(&self.libraries)?;
        self.db = db;
        self.reset_runtime_for_active_library();
        Ok(Task::batch([
            self.refresh_folders(),
            self.refresh_library(),
            attribute_pending_metadata_task(Arc::clone(&self.db)),
            save_app_session_task(self),
        ]))
    }

    pub(super) fn apply_library_registry(
        &mut self,
        registry: LibraryRegistryRuntime,
    ) -> anyhow::Result<Task<Message>> {
        let active_changed = registry.active_library_id != self.libraries.active_library_id;
        self.libraries = registry;
        if active_changed {
            self.select_library(self.libraries.active_library_id.clone())
        } else {
            Ok(save_app_session_task(self))
        }
    }

    fn reset_runtime_for_active_library(&mut self) {
        let preferences = self.db.library_preferences().unwrap_or_default();
        let sidebar_min_width = self.layout().library_sidebar_min_width;
        let sidebar_max_width = self.layout().library_sidebar_max_width;
        self.mode = AppMode::Library;
        self.pending_session_restore = None;
        self.viewer.doc = None;
        self.viewer.current_entry_id = None;
        self.viewer.current_document_path = None;
        self.viewer.rendered_pages.clear();
        self.viewer.page_aspect_ratios.clear();
        self.viewer.cache.clear();
        self.viewer.pending_renders.clear();
        self.viewer.page_fade_started.clear();
        self.viewer.document_error = None;
        self.viewer.pending_document_open = false;

        self.library.compact_view_mode = matches!(preferences.layout_mode, LibraryLayoutMode::List);
        self.library.library_grid_zoom = preferences
            .grid_zoom
            .clamp(self.library_grid_zoom_min(), self.library_grid_zoom_limit());
        self.library.library_metadata_density =
            LibraryMetadataDensity::from_visible_fields(&preferences.visible_metadata_fields);
        self.library.library_sort_mode = preferences.sort_mode;
        self.library.selected_folder = preferences.selected_folder;
        self.library.details_folder_id = None;
        self.library.search_query.clear();
        self.library.search_results = None;
        self.library.search_hit_pages.clear();
        self.library.library_scroll_offset = 0.0;
        self.library.library_tag_sidebar_width = preferences
            .sidebar_width
            .clamp(sidebar_min_width, sidebar_max_width);
        self.library.library_tree_root_expanded = preferences.library_tree_root_expanded;
        self.library.collapsed_library_tree_folders =
            preferences.collapsed_folder_ids.into_iter().collect();
        self.library.library_entries.clear();
        self.library.library_folders.clear();
        self.library.thumbnails.clear();
        self.library.pending_thumbnails.clear();
        self.library.active_tag_filter = None;
        self.library.active_reading_filter = None;
        self.library.missing_filter_active = false;
        self.library.previous_tag_pill_view = None;
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        self.library.details_entry_id = None;
        self.library.details_title_input.clear();
        self.library.details_author_input.clear();
        self.library.library_error = None;
        self.library.library_startup_loading = true;
        self.library.raindrop_connect_dialog_open = false;
        self.library.raindrop_import_dialog_open = false;
        self.library.raindrop_import_preview = None;
        self.library.raindrop_import_progress = None;
        self.library.bulk_operation_progress = None;
        self.library.move_picker = None;
        self.clear_library_transient_interactions();
        self.library.library_status = Some(format!("Loading {}...", self.active_library_name()));
    }

    pub(super) fn set_active_library_preview_from_entries(&mut self) {
        let preview = LibraryPreview {
            total_entries: self.library.library_entries.len(),
            thumbnails: self
                .library
                .library_entries
                .iter()
                .take(LIBRARY_SWITCHER_PREVIEW_LIMIT)
                .filter_map(library_preview_thumbnail)
                .collect(),
        };
        self.libraries
            .previews
            .insert(self.libraries.active_library_id.clone(), preview);
    }
}

pub(super) fn load_library_registry(
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

pub(super) fn create_library_profile(
    registry: LibraryRegistryRuntime,
    name: String,
) -> anyhow::Result<LibraryRegistryRuntime> {
    let mut registry = registry;
    let name = clean_library_name(&name)?;
    let id = unique_library_id(&registry);
    let db_path = app_data_dir()?
        .join("libraries")
        .join(&id)
        .join("library.db");
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

pub(super) fn rename_library_profile(
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

pub(super) fn delete_library_profile(
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

fn save_library_registry(registry: &LibraryRegistryRuntime) -> anyhow::Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    let stored = StoredLibraryRegistry {
        active_library_id: registry.active_library_id.clone(),
        libraries: registry.profiles.clone(),
    };
    std::fs::write(&path, serde_json::to_vec_pretty(&stored)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

fn default_registry(data_dir: &Path) -> StoredLibraryRegistry {
    StoredLibraryRegistry {
        active_library_id: DEFAULT_LIBRARY_ID.to_owned(),
        libraries: vec![default_profile(data_dir)],
    }
}

fn default_profile(data_dir: &Path) -> LibraryProfile {
    LibraryProfile {
        id: DEFAULT_LIBRARY_ID.to_owned(),
        name: DEFAULT_LIBRARY_NAME.to_owned(),
        db_path: data_dir.join("library.db"),
    }
}

fn app_data_dir() -> anyhow::Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().to_path_buf())
}

fn registry_path() -> anyhow::Result<PathBuf> {
    Ok(app_data_dir()?.join("libraries.json"))
}

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

fn remove_library_storage(db_path: &Path) -> anyhow::Result<()> {
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

fn load_library_previews(profiles: &[LibraryProfile]) -> HashMap<String, LibraryPreview> {
    profiles
        .iter()
        .map(|profile| {
            let preview = Db::open(profile.db_path.clone())
                .and_then(|db| db.get_entries_sorted(LibrarySortMode::RecentlyAdded))
                .map(|entries| LibraryPreview {
                    total_entries: entries.len(),
                    thumbnails: entries
                        .iter()
                        .take(LIBRARY_SWITCHER_PREVIEW_LIMIT)
                        .filter_map(library_preview_thumbnail)
                        .collect(),
                })
                .unwrap_or_default();
            (profile.id.clone(), preview)
        })
        .collect()
}

fn library_preview_title(entry: &LibraryEntry) -> String {
    entry
        .display_title
        .as_deref()
        .or(entry.title.as_deref())
        .or_else(|| entry.path.file_stem().and_then(|stem| stem.to_str()))
        .or_else(|| entry.path.file_name().and_then(|name| name.to_str()))
        .unwrap_or("PDF")
        .to_owned()
}

fn library_preview_thumbnail(entry: &LibraryEntry) -> Option<LibraryPreviewThumbnail> {
    let width = ThumbnailSize::Small.width_px();
    let path = small_thumbnail_path(&entry.id).ok()?;
    let (rgba, height) = if path.exists() {
        let rgba = std::fs::read(&path).ok()?;
        let height = thumbnail_height_from_rgba_len(rgba.len(), width)?;
        (rgba, height)
    } else {
        let doc = PdfDoc::open(&entry.path).ok()?;
        let page = doc.render_page(0, width).ok()?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &page.rgba);
        (page.rgba, page.height)
    };

    Some(LibraryPreviewThumbnail {
        title: library_preview_title(entry),
        width,
        height,
        handle: image::Handle::from_rgba(u32::from(width), u32::from(height), rgba),
    })
}

fn small_thumbnail_path(entry_id: &EntryId) -> anyhow::Result<PathBuf> {
    Ok(thumbnail_path(entry_id)?.with_file_name(format!("{}.small.rgba", entry_id.as_str())))
}

fn thumbnail_height_from_rgba_len(len: usize, width: u16) -> Option<u16> {
    let stride = usize::from(width) * 4;
    if stride == 0 || len < stride {
        return None;
    }
    Some((len / stride).clamp(1, usize::from(u16::MAX)) as u16)
}
