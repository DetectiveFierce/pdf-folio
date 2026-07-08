use super::*;

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
