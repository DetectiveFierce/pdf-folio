use crate::*;

impl Default for LibraryHistory {
    fn default() -> Self {
        Self {
            nodes: vec![LibraryHistoryNode {
                parent: None,
                children: Vec::new(),
                action: None,
            }],
            current: 0,
        }
    }
}

impl LibraryHistory {
    pub(crate) fn can_undo(&self) -> bool {
        self.nodes
            .get(self.current)
            .is_some_and(|node| node.parent.is_some())
    }

    pub(crate) fn can_redo(&self) -> bool {
        self.nodes
            .get(self.current)
            .is_some_and(|node| !node.children.is_empty())
    }

    pub(crate) fn undo_target(&self) -> Option<(usize, LibraryHistoryAction)> {
        let node = self.nodes.get(self.current)?;
        let parent = node.parent?;
        Some((parent, node.action.clone()?))
    }

    pub(crate) fn redo_target(&self) -> Option<(usize, LibraryHistoryAction)> {
        let node = self.nodes.get(self.current)?;
        let child = node.children.last().copied()?;
        Some((child, self.nodes.get(child)?.action.clone()?))
    }

    pub(crate) fn push(&mut self, action: LibraryHistoryAction) {
        if action.before == action.after {
            return;
        }

        let index = self.nodes.len();
        self.nodes.push(LibraryHistoryNode {
            parent: Some(self.current),
            children: Vec::new(),
            action: Some(action),
        });
        if let Some(current) = self.nodes.get_mut(self.current) {
            current.children.push(index);
        }
        self.current = index;
    }

    pub(crate) fn set_current(&mut self, index: usize) {
        if index < self.nodes.len() {
            self.current = index;
        }
    }
}

impl LibraryClipboard {
    pub(crate) fn label(&self) -> &'static str {
        match (&self.mode, &self.target) {
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Entries(_)) => "Cut PDFs",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Entries(_)) => "Copy PDFs",
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Folder(_)) => "Cut Folder",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Folder(_)) => "Copy Folder",
        }
    }

    pub(crate) fn paste_label(&self) -> &'static str {
        match (&self.mode, &self.target) {
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Entries(_)) => "Move PDFs",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Entries(_)) => "Paste PDFs",
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Folder(_)) => "Move Folder",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Folder(_)) => "Paste Folder",
        }
    }
}

impl PDFolioApp {
    pub(crate) fn can_cut_or_copy_library_selection(&self) -> bool {
        self.mode == AppMode::Library
            && (!self.library.selected_library_entries.is_empty()
                || self.library.details_folder_id.is_some())
    }

    pub(crate) fn can_paste_library_clipboard(&self) -> bool {
        self.mode == AppMode::Library
            && self.library.clipboard.as_ref().is_some_and(|clipboard| {
                match (&clipboard.mode, &clipboard.target) {
                    (LibraryClipboardMode::Copy, LibraryClipboardTarget::Entries(_)) => {
                        self.library.selected_folder.is_some()
                    }
                    (LibraryClipboardMode::Cut, LibraryClipboardTarget::Folder(folder_id)) => {
                        self.library.selected_folder.as_ref() != Some(folder_id)
                            && self
                                .library
                                .selected_folder
                                .as_ref()
                                .is_none_or(|destination| {
                                    folder_can_move_into(
                                        &self.library.library_folders,
                                        folder_id,
                                        destination,
                                    )
                                })
                    }
                    _ => true,
                }
            })
    }

    pub(crate) fn set_library_clipboard(&mut self, mode: LibraryClipboardMode) -> bool {
        let target = if !self.library.selected_library_entries.is_empty() {
            let mut entry_ids = self
                .library
                .selected_library_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            entry_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
            LibraryClipboardTarget::Entries(entry_ids)
        } else if let Some(folder_id) = self.library.details_folder_id.clone() {
            LibraryClipboardTarget::Folder(folder_id)
        } else {
            return false;
        };

        self.library.clipboard = Some(LibraryClipboard { mode, target });
        true
    }
}

impl PDFolioApp {
    pub(crate) fn active_library_folders(&self) -> &Vec<Folder> {
        if self.library.trash_view_active {
            &self.library.library_trash_folders
        } else {
            &self.library.library_folders
        }
    }

    pub(crate) fn child_folders(&self) -> Vec<Folder> {
        if self.library.active_tag_filter.is_some() {
            return Vec::new();
        }

        let mut folders = self
            .active_library_folders()
            .iter()
            .filter(|folder| folder.parent_id == self.library.selected_folder)
            .cloned()
            .collect::<Vec<_>>();
        folders.sort_by_key(|folder| (folder.manual_order, folder.name.to_lowercase()));
        folders
    }

    pub(crate) fn folder_smart_counts(&self, folder_id: Option<&FolderId>) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, self.library.trash_view_active)
    }

    pub(crate) fn normal_folder_smart_counts(
        &self,
        folder_id: Option<&FolderId>,
    ) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, false)
    }

    pub(crate) fn folder_smart_counts_for(
        &self,
        folder_id: Option<&FolderId>,
        trash: bool,
    ) -> FolderSmartCounts {
        if let Some(counts) = self
            .library
            .folder_smart_count_cache
            .get(&(trash, folder_id.cloned()))
            .copied()
        {
            return counts;
        }

        let folder_ids = folder_id.map(|id| self.folder_subtree_ids_for(id, trash));
        let entries = if trash {
            &self.library.library_trash_entries
        } else {
            &self.library.library_entries
        };
        let entries = entries.iter().filter(|entry| {
            folder_ids.as_ref().map_or(true, |folder_ids| {
                entry
                    .folders
                    .iter()
                    .any(|folder| folder_ids.contains(&folder.id))
            })
        });
        let mut counts = FolderSmartCounts::default();
        for entry in entries {
            counts.total += 1;
            if entry.missing {
                counts.missing += 1;
            }
            if entry.page_count.is_some_and(|page_count| {
                page_count > 0 && entry.last_page.saturating_add(1) < page_count
            }) {
                counts.in_progress += 1;
            }
        }
        counts
    }

    pub(crate) fn rebuild_folder_smart_count_cache(&mut self) {
        let mut cache = HashMap::new();
        cache.extend(Self::build_folder_smart_count_cache_for(
            false,
            &self.library.library_entries,
            &self.library.library_folders,
        ));
        cache.extend(Self::build_folder_smart_count_cache_for(
            true,
            &self.library.library_trash_entries,
            &self.library.library_trash_folders,
        ));
        self.library.folder_smart_count_cache = cache;
    }

    fn build_folder_smart_count_cache_for(
        trash: bool,
        entries: &[LibraryEntry],
        folders: &[Folder],
    ) -> HashMap<(bool, Option<FolderId>), FolderSmartCounts> {
        let parent_by_folder = folders
            .iter()
            .map(|folder| (folder.id.clone(), folder.parent_id.clone()))
            .collect::<HashMap<_, _>>();
        let mut cache = HashMap::<(bool, Option<FolderId>), FolderSmartCounts>::new();

        for entry in entries {
            let contribution = FolderSmartCounts {
                total: 1,
                in_progress: usize::from(entry.page_count.is_some_and(|page_count| {
                    page_count > 0 && entry.last_page.saturating_add(1) < page_count
                })),
                missing: usize::from(entry.missing),
            };
            add_folder_smart_count(&mut cache, trash, None, contribution);

            let mut counted_folders = HashSet::new();
            for folder in &entry.folders {
                let mut cursor = Some(folder.id.clone());
                while let Some(folder_id) = cursor {
                    if !counted_folders.insert(folder_id.clone()) {
                        break;
                    }
                    add_folder_smart_count(
                        &mut cache,
                        trash,
                        Some(folder_id.clone()),
                        contribution,
                    );
                    cursor = parent_by_folder.get(&folder_id).cloned().flatten();
                }
            }
        }

        cache
    }

    pub(crate) fn folder_subtree_ids(&self, folder_id: &FolderId) -> HashSet<FolderId> {
        self.folder_subtree_ids_for(folder_id, self.library.trash_view_active)
    }

    pub(crate) fn folder_subtree_ids_for(
        &self,
        folder_id: &FolderId,
        trash: bool,
    ) -> HashSet<FolderId> {
        let mut folder_ids = HashSet::new();
        self.collect_folder_subtree_ids_for(folder_id, trash, &mut folder_ids);
        folder_ids
    }

    pub(crate) fn move_picker_expanded_folders(&self) -> HashSet<FolderId> {
        self.library
            .library_folders
            .iter()
            .filter(|folder| {
                !self
                    .library
                    .collapsed_library_tree_folders
                    .contains(&folder.id)
            })
            .map(|folder| folder.id.clone())
            .collect()
    }

    pub(crate) fn collect_folder_subtree_ids_for(
        &self,
        folder_id: &FolderId,
        trash: bool,
        folder_ids: &mut HashSet<FolderId>,
    ) {
        if !folder_ids.insert(folder_id.clone()) {
            return;
        }
        let folders = if trash {
            &self.library.library_trash_folders
        } else {
            &self.library.library_folders
        };
        for child in folders
            .iter()
            .filter(|folder| folder.parent_id.as_ref() == Some(folder_id))
        {
            self.collect_folder_subtree_ids_for(&child.id, trash, folder_ids);
        }
    }

    pub(crate) fn selected_folder_name(&self) -> Option<String> {
        self.selected_folder().map(|folder| folder.name.clone())
    }

    pub(crate) fn selected_folder(&self) -> Option<&Folder> {
        self.library.selected_folder.as_ref().and_then(|selected| {
            self.active_library_folders()
                .iter()
                .find(|folder| &folder.id == selected)
        })
    }

    pub(crate) fn details_folder(&self) -> Option<&Folder> {
        self.library
            .details_folder_id
            .as_ref()
            .and_then(|selected| {
                self.active_library_folders()
                    .iter()
                    .find(|folder| &folder.id == selected)
            })
    }

    pub(crate) fn selected_folder_sibling_order(
        &self,
    ) -> Option<(Option<FolderId>, Vec<FolderId>, usize)> {
        let folder = self.details_folder()?;
        let parent_id = folder.parent_id.clone();
        let mut siblings = self
            .active_library_folders()
            .iter()
            .filter(|candidate| candidate.parent_id == parent_id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| (candidate.manual_order, candidate.name.to_lowercase()));
        let folder_ids = siblings
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let index = folder_ids
            .iter()
            .position(|folder_id| folder_id == &folder.id)?;
        Some((parent_id, folder_ids, index))
    }

    pub(crate) fn selected_folder_manual_reorder(
        &self,
        direction: isize,
    ) -> Option<(Option<FolderId>, Vec<FolderId>)> {
        let (parent_id, mut folder_ids, index) = self.selected_folder_sibling_order()?;
        let next_index = index.checked_add_signed(direction)?;
        if next_index >= folder_ids.len() {
            return None;
        }
        folder_ids.swap(index, next_index);
        Some((parent_id, folder_ids))
    }

    pub(crate) fn folder_drag_manual_reorder(
        &self,
        folder_id: &FolderId,
        target_id: &FolderId,
    ) -> Option<(Option<FolderId>, Vec<FolderId>)> {
        let folder = self
            .active_library_folders()
            .iter()
            .find(|folder| &folder.id == folder_id)?;
        let target = self
            .active_library_folders()
            .iter()
            .find(|folder| &folder.id == target_id)?;
        if folder.parent_id != target.parent_id || folder.id == target.id {
            return None;
        }

        let mut siblings = self
            .active_library_folders()
            .iter()
            .filter(|candidate| candidate.parent_id == folder.parent_id)
            .collect::<Vec<_>>();
        siblings.sort_by_key(|candidate| (candidate.manual_order, candidate.name.to_lowercase()));
        let folder_ids = siblings
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>();
        let next_order = reorder_folder_ids_before_target(&folder_ids, folder_id, target_id)?;
        (next_order != folder_ids).then_some((folder.parent_id.clone(), next_order))
    }

    pub(crate) fn sync_folder_rename_input(&mut self) {
        self.library.folder_rename_input = self
            .details_folder()
            .map_or_else(String::new, |folder| folder.name.clone());
    }

    pub(crate) fn folder_breadcrumbs(&self) -> Vec<(String, Option<FolderId>)> {
        let mut breadcrumbs = vec![(
            if self.library.trash_view_active {
                String::from("Trash Can")
            } else {
                String::from("Library")
            },
            None,
        )];
        let mut current = self.library.selected_folder.clone();
        let mut path = Vec::new();
        let mut seen = HashSet::new();

        while let Some(folder_id) = current {
            if !seen.insert(folder_id.clone()) {
                break;
            }

            let Some(folder) = self
                .active_library_folders()
                .iter()
                .find(|folder| folder.id == folder_id)
            else {
                break;
            };

            path.push((folder.name.clone(), Some(folder.id.clone())));
            current = folder.parent_id.clone();
        }

        path.reverse();
        breadcrumbs.extend(path);
        breadcrumbs
    }
}

fn add_folder_smart_count(
    cache: &mut HashMap<(bool, Option<FolderId>), FolderSmartCounts>,
    trash: bool,
    folder_id: Option<FolderId>,
    contribution: FolderSmartCounts,
) {
    let counts = cache.entry((trash, folder_id)).or_default();
    counts.total += contribution.total;
    counts.in_progress += contribution.in_progress;
    counts.missing += contribution.missing;
}

impl PDFolioApp {
    pub(crate) fn select_library_entry(&mut self, entry_id: EntryId) {
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

    pub(crate) fn toggle_library_entry_selection(&mut self, entry_id: EntryId) {
        toggle_selection_entry_id(&mut self.library.selected_library_entries, entry_id.clone());
        self.library.library_selection_anchor = Some(entry_id);
        let visible_entries = self.visible_library_entries();
        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    pub(crate) fn master_checkbox_state(&self) -> MasterCheckboxState {
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

    pub(crate) fn select_library_range(
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

    pub(crate) fn select_all_visible_library_entries(&mut self) {
        let visible_entries = self.visible_library_entries();
        self.library.selected_library_entries = visible_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.library.library_selection_anchor =
            visible_entries.first().map(|entry| entry.id.clone());
        self.sync_details_editor_to_selection();
    }

    pub(crate) fn clear_library_selection(&mut self) {
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        if self.library.trash_view_active {
            self.library.details_folder_id = None;
            self.library.folder_details_sidebar_open = false;
        }
        self.sync_details_editor_to_selection();
    }

    pub(crate) fn clear_library_sidebar_details(&mut self) {
        self.clear_library_selection();
        self.library.details_folder_id = None;
        self.library.folder_details_sidebar_open = false;
        self.library.folder_rename_input.clear();
    }

    pub(crate) fn select_folder_for_details(&mut self, folder_id: Option<FolderId>) {
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        self.library.details_entry_id = None;
        self.library.details_title_input.clear();
        self.library.details_author_input.clear();
        self.library.details_folder_id = folder_id;
        self.library.folder_details_sidebar_open = true;
        self.sync_folder_rename_input();
    }

    pub(crate) fn select_folder_in_tree(&mut self, folder_id: Option<FolderId>) {
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

    pub(crate) fn open_folder_from_tree(&mut self, folder_id: Option<FolderId>) {
        self.library.trash_view_active = false;
        self.library.selected_folder = folder_id.clone();
        self.library.active_recently_opened_filter = false;
        self.library.previous_tag_pill_view = None;
        self.select_folder_in_tree(folder_id);
        self.library.library_drag = None;
        self.library.library_scroll_offset = 0.0;
    }

    pub(crate) fn prune_selection_to_visible_entries(&mut self, visible_entries: &[LibraryEntry]) {
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

    pub(crate) fn selected_entries(&self) -> Vec<LibraryEntry> {
        self.active_library_entries()
            .iter()
            .filter(|entry| self.library.selected_library_entries.contains(&entry.id))
            .cloned()
            .collect()
    }

    pub(crate) fn primary_selected_entry(&self) -> Option<LibraryEntry> {
        if self.library.selected_library_entries.len() != 1 {
            return None;
        }

        let entry_id = self.library.selected_library_entries.iter().next()?;
        self.active_library_entries()
            .iter()
            .find(|entry| &entry.id == entry_id)
            .cloned()
    }

    pub(crate) fn sync_details_editor_to_selection(&mut self) {
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
