use super::*;

impl PDFolioApp {
    pub(super) fn select_library_entry(&mut self, entry_id: EntryId) {
        let visible_entries = self.visible_library_entries();
        if self.viewer.modifiers.shift() {
            self.select_library_range(entry_id, &visible_entries);
        } else if self.viewer.modifiers.control() {
            if !self
                .library
                .selected_library_entries
                .insert(entry_id.clone())
            {
                self.library.selected_library_entries.remove(&entry_id);
            }
            self.library.library_selection_anchor = Some(entry_id);
        } else {
            self.library.selected_library_entries.clear();
            self.library
                .selected_library_entries
                .insert(entry_id.clone());
            self.library.library_selection_anchor = Some(entry_id);
        }

        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    pub(super) fn toggle_library_entry_selection(&mut self, entry_id: EntryId) {
        toggle_selection_entry_id(&mut self.library.selected_library_entries, entry_id.clone());
        self.library.library_selection_anchor = Some(entry_id);
        let visible_entries = self.visible_library_entries();
        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    pub(super) fn master_checkbox_state(&self) -> MasterCheckboxState {
        let visible_entries = self.visible_library_entries();
        if visible_entries.is_empty() {
            return MasterCheckboxState::None;
        }

        let selected_visible = visible_entries
            .iter()
            .filter(|entry| self.library.selected_library_entries.contains(&entry.id))
            .count();

        master_checkbox_state_for_counts(selected_visible, visible_entries.len())
    }

    pub(super) fn select_library_range(
        &mut self,
        entry_id: EntryId,
        visible_entries: &[LibraryEntry],
    ) {
        let anchor = self
            .library
            .library_selection_anchor
            .clone()
            .or_else(|| self.library.selected_library_entries.iter().next().cloned())
            .unwrap_or_else(|| entry_id.clone());
        let Some(anchor_index) = visible_entries.iter().position(|entry| entry.id == anchor) else {
            self.library.selected_library_entries.clear();
            self.library
                .selected_library_entries
                .insert(entry_id.clone());
            self.library.library_selection_anchor = Some(entry_id);
            return;
        };
        let Some(entry_index) = visible_entries
            .iter()
            .position(|entry| entry.id == entry_id)
        else {
            return;
        };

        self.library.selected_library_entries.clear();
        let visible_ids = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        self.library
            .selected_library_entries
            .extend(range_selection_ids(anchor_index, entry_index, &visible_ids));
        self.library.library_selection_anchor = Some(anchor);
    }

    pub(super) fn select_all_visible_library_entries(&mut self) {
        let visible_entries = self.visible_library_entries();
        self.library.selected_library_entries = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.library.library_selection_anchor =
            visible_entries.first().map(|entry| entry.id.clone());
        self.sync_details_editor_to_selection();
    }

    pub(super) fn clear_library_selection(&mut self) {
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        self.chrome.open_selection_menu = None;
        if self.library.trash_view_active {
            self.library.details_folder_id = None;
            self.library.folder_details_sidebar_open = false;
        }
        self.sync_details_editor_to_selection();
    }

    pub(super) fn clear_library_sidebar_details(&mut self) {
        self.clear_library_selection();
        self.library.details_folder_id = None;
        self.library.folder_details_sidebar_open = false;
        self.library.folder_rename_input.clear();
    }

    pub(super) fn select_folder_for_details(&mut self, folder_id: Option<FolderId>) {
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        self.library.details_entry_id = None;
        self.library.details_title_input.clear();
        self.library.details_author_input.clear();
        self.library.details_folder_id = folder_id;
        self.library.folder_details_sidebar_open = true;
        self.sync_folder_rename_input();
    }

    pub(super) fn select_folder_in_tree(&mut self, folder_id: Option<FolderId>) {
        self.library.trash_view_active = false;
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        self.library.details_entry_id = None;
        self.library.details_title_input.clear();
        self.library.details_author_input.clear();
        self.library.details_folder_id = folder_id;
        self.library.folder_details_sidebar_open = false;
        self.sync_folder_rename_input();
    }

    pub(super) fn open_folder_from_tree(&mut self, folder_id: Option<FolderId>) {
        self.library.trash_view_active = false;
        self.library.selected_folder = folder_id.clone();
        self.library.previous_tag_pill_view = None;
        self.select_folder_in_tree(folder_id);
        self.library.library_drag = None;
        self.library.library_scroll_offset = 0.0;
    }

    pub(super) fn prune_selection_to_visible_entries(&mut self, visible_entries: &[LibraryEntry]) {
        let visible_ids = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.library
            .selected_library_entries
            .retain(|entry_id| visible_ids.contains(entry_id));
        if self
            .library
            .library_selection_anchor
            .as_ref()
            .is_some_and(|anchor| !visible_ids.contains(anchor))
        {
            self.library.library_selection_anchor =
                self.library.selected_library_entries.iter().next().cloned();
        }
        self.sync_details_editor_to_selection();
    }

    pub(super) fn selected_entries(&self) -> Vec<LibraryEntry> {
        self.active_library_entries()
            .iter()
            .filter(|entry| self.library.selected_library_entries.contains(&entry.id))
            .cloned()
            .collect()
    }

    pub(super) fn primary_selected_entry(&self) -> Option<LibraryEntry> {
        if self.library.selected_library_entries.len() != 1 {
            return None;
        }

        let entry_id = self.library.selected_library_entries.iter().next()?;
        self.active_library_entries()
            .iter()
            .find(|entry| &entry.id == entry_id)
            .cloned()
    }

    pub(super) fn sync_details_editor_to_selection(&mut self) {
        let Some(entry) = self.primary_selected_entry() else {
            self.library.details_entry_id = None;
            self.library.details_title_input.clear();
            self.library.details_author_input.clear();
            return;
        };

        if self.library.details_entry_id.as_ref() == Some(&entry.id) {
            return;
        }

        self.library.details_title_input = entry_title(&entry);
        self.library.details_author_input = entry
            .display_author
            .clone()
            .or_else(|| entry.author.clone())
            .unwrap_or_default();
        self.library.details_entry_id = Some(entry.id);
    }
}
