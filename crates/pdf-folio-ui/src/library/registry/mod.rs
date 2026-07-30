//! # Multi-library registry
//!
//! Users can keep several independent vaults (each with its own SQLite
//! file). This submodule owns profiles, the switcher UI state, persistence
//! (`libraries.json`), cover previews, and cloud sync of library rows.
//!
//! ## Submodules
//!
//! - [`state`] — `LibraryProfile`, `LibraryRegistryRuntime`, dialogs
//! - [`session`] — load/create/rename/delete + paths under app data
//! - [`preview`] — switcher card thumbnails from each vault DB
//! - [`tasks`] — merge remote sync rows into local registry
//!
//! ## App integration
//!
//! Methods on `PDFolioApp` here open the switcher, swap `app.db`, reset
//! library runtime state, and refresh folders/entries for the new vault.

/// Switcher cover previews loaded from each vault database.
pub(crate) mod preview;
/// Load/create/rename/delete vaults and `libraries.json` paths under app data.
pub(crate) mod session;
/// `LibraryProfile`, `LibraryRegistryRuntime`, name dialogs, and preview payloads.
pub(crate) mod state;
/// Merge remote sync library rows into the local multi-library registry.
pub(crate) mod tasks;

pub(crate) use preview::*;
pub(crate) use session::*;
pub(crate) use state::*;
pub(crate) use tasks::*;

use crate::*;
impl PDFolioApp {
    /// Display name of the active vault profile (falls back to the default label).
    pub(crate) fn active_library_name(&self) -> &str {
        self.libraries
            .active_profile()
            .map_or(DEFAULT_LIBRARY_NAME, |profile| profile.name.as_str())
    }

    /// Enter library-switcher mode and clear transient library interactions.
    pub(crate) fn open_library_switcher(&mut self) {
        self.mode = AppMode::LibrarySwitcher;
        self.clear_library_transient_interactions();
        self.chrome.open_context_menu = None;
        self.viewer.zoom_menu_open = false;
        self.libraries.open_menu_library_id = None;
    }

    /// Open `library_id` as the active vault: swap `Db`, reset runtime, refresh data.
    pub(crate) fn select_library(&mut self, library_id: String) -> anyhow::Result<Task<Message>> {
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

    /// Replace runtime registry state after create/rename/delete/sync; reselect if active id changed.
    pub(crate) fn apply_library_registry(
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
        self.viewer.document_open_started_at = None;

        self.library.compact_view_mode = matches!(preferences.layout_mode, LibraryLayoutMode::List);
        self.library.library_grid_zoom = LibraryPreferences::default()
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
        self.library.library_trash_entries.clear();
        self.library.library_folders.clear();
        self.library.library_trash_folders.clear();
        self.library.trash_view_active = false;
        self.library.thumbnails.clear();
        self.library.pending_thumbnails.clear();
        self.library.active_tag_filter = None;
        self.library.active_reading_filter = None;
        self.library.active_recently_opened_filter = false;
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

    /// Refresh the active switcher card count from the in-memory entry list.
    ///
    /// Cover decoding is deliberately excluded because this method runs on
    /// library-load update paths. Covers are refreshed asynchronously when the
    /// switcher opens.
    pub(crate) fn set_active_library_preview_from_entries(&mut self) {
        self.libraries
            .previews
            .entry(self.libraries.active_library_id.clone())
            .or_default()
            .total_entries = self.library.library_entries.len();
    }
}
