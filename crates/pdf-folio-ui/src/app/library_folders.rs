use super::*;

impl PDFolioApp {
    pub(super) fn active_library_folders(&self) -> &Vec<Folder> {
        if self.library.trash_view_active {
            &self.library.library_trash_folders
        } else {
            &self.library.library_folders
        }
    }

    pub(super) fn child_folders(&self) -> Vec<Folder> {
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

    pub(super) fn folder_smart_counts(&self, folder_id: Option<&FolderId>) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, self.library.trash_view_active)
    }

    pub(super) fn normal_folder_smart_counts(
        &self,
        folder_id: Option<&FolderId>,
    ) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, false)
    }

    pub(super) fn folder_smart_counts_for(
        &self,
        folder_id: Option<&FolderId>,
        trash: bool,
    ) -> FolderSmartCounts {
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

    pub(super) fn folder_subtree_ids(&self, folder_id: &FolderId) -> HashSet<FolderId> {
        self.folder_subtree_ids_for(folder_id, self.library.trash_view_active)
    }

    pub(super) fn folder_subtree_ids_for(
        &self,
        folder_id: &FolderId,
        trash: bool,
    ) -> HashSet<FolderId> {
        let mut folder_ids = HashSet::new();
        self.collect_folder_subtree_ids_for(folder_id, trash, &mut folder_ids);
        folder_ids
    }

    pub(super) fn move_picker_expanded_folders(&self) -> HashSet<FolderId> {
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

    pub(super) fn collect_folder_subtree_ids_for(
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

    pub(super) fn selected_folder_name(&self) -> Option<String> {
        self.selected_folder().map(|folder| folder.name.clone())
    }

    pub(super) fn selected_folder(&self) -> Option<&Folder> {
        self.library.selected_folder.as_ref().and_then(|selected| {
            self.active_library_folders()
                .iter()
                .find(|folder| &folder.id == selected)
        })
    }

    pub(super) fn details_folder(&self) -> Option<&Folder> {
        self.library
            .details_folder_id
            .as_ref()
            .and_then(|selected| {
                self.active_library_folders()
                    .iter()
                    .find(|folder| &folder.id == selected)
            })
    }

    pub(super) fn selected_folder_sibling_order(
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

    pub(super) fn selected_folder_manual_reorder(
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

    pub(super) fn folder_drag_manual_reorder(
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

    pub(super) fn sync_folder_rename_input(&mut self) {
        self.library.folder_rename_input = self
            .details_folder()
            .map_or_else(String::new, |folder| folder.name.clone());
    }

    pub(super) fn folder_breadcrumbs(&self) -> Vec<(String, Option<FolderId>)> {
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
