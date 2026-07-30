//! # High-level library intents
//!
//! Imperative methods on `PDFolioApp` (plus small helpers on history/clipboard
//! types) that implement library UX: selection, folder navigation, clipboard,
//! drag/drop lifecycle, smart counts, and breadcrumbs.
//!
//! ## Role vs `update` / `tasks` / components
//!
//! - **`update`** matches messages and decides *when* to call these methods.
//! - **This module** owns multi-step state changes that must stay consistent
//!   (e.g. selection + details editor sync, drag target + autoscroll).
//! - **`tasks`** performs durable Db work; actions often finish by returning or
//!   scheduling those tasks.
//! - **`components::library::{drag, selection}`** supply pure helpers (reorder
//!   math, dwell timing); actions hold the live `LibraryDragState` / selection
//!   sets on the app.
//!
//! Prefer adding a method here when a gesture needs more than one field update
//! or must share logic between keyboard shortcuts, menus, and pointer events.

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
    /// Whether the current history cursor has a parent action that can be undone.
    pub(crate) fn can_undo(&self) -> bool {
        self.nodes
            .get(self.current)
            .is_some_and(|node| node.parent.is_some())
    }

    /// Whether the current node has a child branch that can be redone.
    pub(crate) fn can_redo(&self) -> bool {
        self.nodes
            .get(self.current)
            .is_some_and(|node| !node.children.is_empty())
    }

    /// Parent index and action to apply when undoing from the current cursor.
    pub(crate) fn undo_target(&self) -> Option<(usize, LibraryHistoryAction)> {
        let node = self.nodes.get(self.current)?;
        let parent = node.parent?;
        Some((parent, node.action.clone()?))
    }

    /// Child index and action to apply when redoing from the current cursor.
    pub(crate) fn redo_target(&self) -> Option<(usize, LibraryHistoryAction)> {
        let node = self.nodes.get(self.current)?;
        let child = node.children.last().copied()?;
        Some((child, self.nodes.get(child)?.action.clone()?))
    }

    /// Append a new history node as a child of the current cursor.
    ///
    /// No-ops when `before == after` so empty organization diffs never pollute
    /// the undo stack. Moves `current` to the new node (linear redo fork).
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

    /// Move the history cursor to `index` if it is in range (after undo/redo).
    pub(crate) fn set_current(&mut self, index: usize) {
        if index < self.nodes.len() {
            self.current = index;
        }
    }
}

impl LibraryClipboard {
    /// Short noun phrase describing what was cut/copied (menus, status).
    pub(crate) fn label(&self) -> &'static str {
        match (&self.mode, &self.target) {
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Entries(_)) => "Cut PDFs",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Entries(_)) => "Copy PDFs",
            (LibraryClipboardMode::Cut, LibraryClipboardTarget::Folder(_)) => "Cut Folder",
            (LibraryClipboardMode::Copy, LibraryClipboardTarget::Folder(_)) => "Copy Folder",
        }
    }

    /// Verb phrase for the paste/move action that would consume this clipboard.
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
    /// Whether cut/copy commands should be enabled for the current library selection.
    pub(crate) fn can_cut_or_copy_library_selection(&self) -> bool {
        self.mode == AppMode::Library
            && (!self.library.selected_library_entries.is_empty()
                || self.library.details_folder_id.is_some())
    }

    /// Whether paste is valid given clipboard mode, target, and current folder.
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

    /// Capture the current entry or details-folder selection into the clipboard.
    ///
    /// Returns `false` when there is nothing to cut/copy. Entry ids are sorted
    /// for stable clipboard payloads.
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
    /// Live or trash folder list depending on `trash_view_active`.
    pub(crate) fn active_library_folders(&self) -> &Vec<Folder> {
        if self.library.trash_view_active {
            &self.library.library_trash_folders
        } else {
            &self.library.library_folders
        }
    }

    /// Immediate children of the selected folder (empty under a tag filter).
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

    /// Cached total / in-progress / missing counts for a folder subtree in the active view.
    pub(crate) fn folder_smart_counts(&self, folder_id: Option<&FolderId>) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, self.library.trash_view_active)
    }

    /// Smart counts for the non-trash library (ignores trash view flag).
    pub(crate) fn normal_folder_smart_counts(
        &self,
        folder_id: Option<&FolderId>,
    ) -> FolderSmartCounts {
        self.folder_smart_counts_for(folder_id, false)
    }

    /// Smart counts for `folder_id` (or library root when `None`) in live or trash data.
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
            folder_ids.as_ref().is_none_or(|folder_ids| {
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

    /// Recompute the full folder smart-count cache after library/folder reloads.
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

    /// Folder id plus all descendants in the active (live or trash) tree.
    pub(crate) fn folder_subtree_ids(&self, folder_id: &FolderId) -> HashSet<FolderId> {
        self.folder_subtree_ids_for(folder_id, self.library.trash_view_active)
    }

    /// Folder id plus all descendants, choosing live or trash folder lists.
    pub(crate) fn folder_subtree_ids_for(
        &self,
        folder_id: &FolderId,
        trash: bool,
    ) -> HashSet<FolderId> {
        let mut folder_ids = HashSet::new();
        self.collect_folder_subtree_ids_for(folder_id, trash, &mut folder_ids);
        folder_ids
    }

    /// Folder ids currently expanded in the sidebar (used by the move-picker tree).
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

    /// Recursively insert `folder_id` and descendants into `folder_ids`.
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

    /// Display name of the navigated folder, if any.
    pub(crate) fn selected_folder_name(&self) -> Option<String> {
        self.selected_folder().map(|folder| folder.name.clone())
    }

    /// Folder currently open in the main library view (`selected_folder`).
    pub(crate) fn selected_folder(&self) -> Option<&Folder> {
        self.library.selected_folder.as_ref().and_then(|selected| {
            self.active_library_folders()
                .iter()
                .find(|folder| &folder.id == selected)
        })
    }

    /// Folder shown in the details/inspector pane (`details_folder_id`).
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

    /// Parent id, sibling folder ids in manual order, and index of the details folder.
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

    /// Sibling order after moving the details folder by `direction` (-1 up / +1 down).
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

    /// Sibling order after dropping `folder_id` before `target_id` under a shared parent.
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

    /// Copy the details folder name into the rename text field.
    pub(crate) fn sync_folder_rename_input(&mut self) {
        self.library.folder_rename_input = self
            .details_folder()
            .map_or_else(String::new, |folder| folder.name.clone());
    }

    /// Root-to-current path labels for the toolbar breadcrumb row.
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
    /// Apply click selection with shift-range / ctrl-toggle modifiers, then sync details.
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

    /// Toggle one entry in the multi-selection set and refresh the details editor.
    pub(crate) fn toggle_library_entry_selection(&mut self, entry_id: EntryId) {
        toggle_selection_entry_id(&mut self.library.selected_library_entries, entry_id.clone());
        self.library.library_selection_anchor = Some(entry_id);
        let visible_entries = self.visible_library_entries();
        self.prune_selection_to_visible_entries(&visible_entries);
        self.sync_details_editor_to_selection();
    }

    /// None / Partial / All for the visible-entry master checkbox.
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

    /// Select the inclusive range from the selection anchor to `entry_id`.
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

    /// Select every entry currently shown under filters/search.
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

    /// Clear multi-selection and reset details editor linkage.
    pub(crate) fn clear_library_selection(&mut self) {
        self.library.selected_library_entries.clear();
        self.library.library_selection_anchor = None;
        if self.library.trash_view_active {
            self.library.details_folder_id = None;
            self.library.folder_details_sidebar_open = false;
        }
        self.sync_details_editor_to_selection();
    }

    /// Clear details folder/entry focus used by the navigation sidebar panels.
    pub(crate) fn clear_library_sidebar_details(&mut self) {
        self.clear_library_selection();
        self.library.details_folder_id = None;
        self.library.folder_details_sidebar_open = false;
        self.library.folder_rename_input.clear();
    }

    /// Focus a folder in the details pane without changing navigation.
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

    /// Select a folder in the tree for details; may expand ancestors as needed.
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

    /// Navigate into a folder (sets `selected_folder`) and clear entry selection.
    pub(crate) fn open_folder_from_tree(&mut self, folder_id: Option<FolderId>) {
        self.library.trash_view_active = false;
        self.library.selected_folder = folder_id.clone();
        self.library.active_recently_opened_filter = false;
        self.library.previous_tag_pill_view = None;
        self.select_folder_in_tree(folder_id);
        self.library.library_drag = None;
        self.library.library_scroll_offset = 0.0;
    }

    /// Drop selected ids that are no longer in the visible list after filter/search changes.
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

    /// Full `LibraryEntry` records for the current multi-selection (stable order).
    pub(crate) fn selected_entries(&self) -> Vec<LibraryEntry> {
        self.active_library_entries()
            .iter()
            .filter(|entry| self.library.selected_library_entries.contains(&entry.id))
            .cloned()
            .collect()
    }

    /// Preferred single entry for details/inspector (anchor or sole selection).
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

    /// Push title/author fields from the primary selected entry into editor inputs.
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

fn rect_distance_squared(px: f32, py: f32, x: f32, y: f32, width: f32, height: f32) -> f32 {
    let closest_x = px.clamp(x, x + width.max(0.0));
    let closest_y = py.clamp(y, y + height.max(0.0));
    let dx = px - closest_x;
    let dy = py - closest_y;
    dx * dx + dy * dy
}

impl PDFolioApp {
    /// Whether manual-order drag is allowed in the current sort/filter context.
    pub(crate) fn can_drag_reorder_library(&self) -> bool {
        if self.library.trash_view_active {
            return false;
        }
        can_drag_reorder_library_for_state(
            self.library.library_sort_mode,
            &self.library.search_query,
            self.library.search_results.is_some(),
            self.library.active_tag_filter.is_some(),
            self.library.selected_folder.is_some(),
        )
    }

    /// Start an entry drag (single or multi-selection) from pointer press on `entry_id`.
    pub(crate) fn begin_library_drag(&mut self, entry_id: EntryId) {
        self.library.folder_drag = None;
        let visible_entries = self.visible_library_entries();
        let Some(source_index) = visible_entries
            .iter()
            .position(|entry| entry.id == entry_id)
        else {
            return;
        };
        let multi = self.library.selected_library_entries.len() > 1
            && self.library.selected_library_entries.contains(&entry_id);
        let entry_ids = if multi {
            visible_entries
                .iter()
                .filter(|entry| self.library.selected_library_entries.contains(&entry.id))
                .map(|entry| entry.id.clone())
                .collect()
        } else {
            vec![entry_id.clone()]
        };

        self.library.library_drag = Some(LibraryDragState::new(
            entry_id,
            entry_ids,
            source_index,
            multi,
        ));
        self.adjust_scroll_for_parent_directory_drop_box(true);
    }

    /// Start a folder-card drag in the main content area.
    pub(crate) fn begin_folder_drag(&mut self, folder_id: FolderId) {
        if !self
            .library
            .library_folders
            .iter()
            .any(|folder| folder.id == folder_id)
        {
            return;
        }

        self.library.library_drag = None;
        self.library.folder_drag = Some(FolderDragState::new(folder_id));
        self.library.folder_drag_started_in_tree = false;
        self.adjust_scroll_for_parent_directory_drop_box(true);
    }

    /// Start a folder drag originating from the sidebar tree.
    pub(crate) fn begin_folder_tree_drag(&mut self, folder_id: FolderId) {
        self.begin_folder_drag(folder_id);
        if self.library.folder_drag.is_some() {
            self.library.folder_drag_started_in_tree = true;
        }
    }

    /// Recompute reorder index and folder drop targets from the pointer position.
    pub(crate) fn update_library_drag_target(&mut self, cursor: Point) {
        if self.library.library_drag.is_none() {
            return;
        }

        let can_drag_reorder = self.can_drag_reorder_library();
        if let Some(drag) = &mut self.library.library_drag {
            if drag.update_cursor(cursor) && !can_drag_reorder && drag.drop_target.is_none() {
                self.library.library_status = Some(String::from(
                    "Drop on a folder, or switch to unfiltered Manual sort to reorder PDFs.",
                ));
            }
        }

        if self
            .library
            .library_drag
            .as_ref()
            .is_some_and(|drag| drag.active)
        {
            if self
                .library
                .library_drag
                .as_ref()
                .is_some_and(|drag| drag.parent_drop_target)
            {
                return;
            }

            self.update_library_drag_target_from_cursor();
            if let Some(target) = self.library_folder_card_target_at_cursor(cursor) {
                self.set_library_drag_card_target(Some(target), Instant::now());
            }
        }
    }

    /// Update pending/active folder drop target and dwell-based tree expansion.
    pub(crate) fn set_folder_drop_hover_target(
        &mut self,
        folder_id: Option<FolderId>,
        now: Instant,
    ) {
        if self.library.library_drag.is_none() && self.library.folder_drag.is_none() {
            return;
        };
        let library_drag_card_target = if folder_id.is_none() {
            self.library
                .library_drag
                .as_ref()
                .filter(|drag| drag.active)
                .and_then(|drag| {
                    drag.cursor
                        .and_then(|cursor| self.library_folder_card_target_at_cursor(cursor))
                })
        } else {
            None
        };
        let folder_drag_card_target = if folder_id.is_none() {
            self.library
                .folder_drag
                .as_ref()
                .filter(|drag| drag.active)
                .and_then(|drag| {
                    drag.cursor.and_then(|cursor| {
                        self.folder_card_target_at_cursor(cursor, &drag.folder_id)
                    })
                })
        } else {
            None
        };

        if let Some(drag) = &mut self.library.library_drag {
            let target = folder_id.clone().or(library_drag_card_target);
            drag.set_pending_folder_target(target, now);
        }

        if let Some(drag) = &mut self.library.folder_drag {
            let target = folder_id.or(folder_drag_card_target).filter(|target| {
                folder_can_move_into(&self.library.library_folders, &drag.folder_id, target)
            });
            drag.set_drop_target(target, now, true);
        }
    }

    /// Promote a pending folder hover to an active drop target after the dwell timeout.
    pub(crate) fn update_folder_drop_target_dwell(&mut self, now: Instant) {
        let library_target = self
            .library
            .library_drag
            .as_ref()
            .and_then(|drag| drag.pending_target_ready(now));

        let folder_target = self
            .library
            .folder_drag
            .as_ref()
            .and_then(|drag| drag.pending_target_ready(now));

        let Some(folder_id) = library_target.or(folder_target) else {
            return;
        };

        let should_expand = self.folder_has_children(&folder_id)
            && self
                .library
                .collapsed_library_tree_folders
                .contains(&folder_id);
        if should_expand {
            self.library
                .collapsed_library_tree_folders
                .remove(&folder_id);
        }

        if let Some(drag) = &mut self.library.library_drag {
            drag.drop_target = Some(folder_id.clone());
            if should_expand {
                drag.expanded_during_drag.insert(folder_id.clone());
            }
        }
        if let Some(drag) = &mut self.library.folder_drag {
            drag.drop_target = Some(folder_id.clone());
            if should_expand {
                drag.expanded_during_drag.insert(folder_id);
            }
        }
    }

    /// Recompute folder-drag drop target (sibling reorder vs nest) from the cursor.
    pub(crate) fn update_folder_drag_target(&mut self, cursor: Point) {
        let Some(drag) = &mut self.library.folder_drag else {
            return;
        };

        if !drag.update_cursor(cursor) {
            return;
        }

        if drag.parent_drop_target {
            return;
        }

        let dragged_folder_id = drag.folder_id.clone();
        if let Some(target) = self.folder_card_target_at_cursor(cursor, &dragged_folder_id) {
            self.set_folder_drag_card_target(Some(target));
        }
    }

    /// Set the folder-card highlight target while dragging a folder.
    pub(crate) fn set_folder_drag_card_target(&mut self, folder_id: Option<FolderId>) {
        let Some(drag) = &mut self.library.folder_drag else {
            return;
        };
        let target = folder_id.filter(|target| {
            folder_can_move_into(&self.library.library_folders, &drag.folder_id, target)
        });
        drag.set_drop_target(target, Instant::now(), true);
    }

    /// Set the entry-list insertion index while dragging library entries.
    pub(crate) fn set_library_drag_card_target(
        &mut self,
        folder_id: Option<FolderId>,
        now: Instant,
    ) {
        let Some(drag) = &mut self.library.library_drag else {
            return;
        };
        drag.set_pending_folder_target(folder_id, now);
    }

    /// Folder id that will receive a drop if the pointer is released now.
    pub(crate) fn active_folder_drop_target(&self) -> Option<&FolderId> {
        active_folder_drop_target(
            self.library.library_drag.as_ref(),
            self.library.folder_drag.as_ref(),
        )
    }

    /// Whether the “move to parent” drop strip should be shown during a drag.
    pub(crate) fn parent_directory_drop_box_visible(&self) -> bool {
        self.library.active_tag_filter.is_none()
            && self.library.selected_folder.is_some()
            && (self.library.library_drag.is_some() || self.library.folder_drag.is_some())
    }

    /// Whether the parent-directory strip is the active drop target.
    pub(crate) fn parent_directory_drop_target_active(&self) -> bool {
        self.library
            .library_drag
            .as_ref()
            .is_some_and(|drag| drag.parent_drop_target)
            || self
                .library
                .folder_drag
                .as_ref()
                .is_some_and(|drag| drag.parent_drop_target)
    }

    /// Parent of the currently open folder (destination for the parent drop strip).
    pub(crate) fn parent_directory_folder_id(&self) -> Option<FolderId> {
        let selected_folder = self.library.selected_folder.as_ref()?;
        self.library
            .library_folders
            .iter()
            .find(|folder| &folder.id == selected_folder)
            .and_then(|folder| folder.parent_id.clone())
    }

    /// Toggle parent-strip hover and adjust scroll padding when the strip appears.
    pub(crate) fn set_parent_directory_drop_hover_target(&mut self, active: bool) {
        if let Some(drag) = &mut self.library.library_drag {
            if drag.active {
                drag.set_parent_drop_target(active);
            }
        }

        if let Some(drag) = &mut self.library.folder_drag {
            if drag.active {
                drag.set_parent_drop_target(active);
            }
        }
    }

    /// Preserve visual position when the parent drop strip is inserted or removed.
    pub(crate) fn adjust_scroll_for_parent_directory_drop_box(&mut self, visible: bool) {
        if self.library.selected_folder.is_none() {
            return;
        }

        let height = parent_directory_drop_box_height(self) + Spacing::MD;
        if visible {
            if self.library.library_scroll_offset > 0.0
                && !self.library.parent_directory_drop_scroll_adjusted
            {
                self.library.library_scroll_offset += height;
                self.library.parent_directory_drop_scroll_adjusted = true;
            }
        } else if self.library.parent_directory_drop_scroll_adjusted {
            self.library.library_scroll_offset =
                (self.library.library_scroll_offset - height).max(0.0);
            self.library.parent_directory_drop_scroll_adjusted = false;
        }
    }

    /// Hit-test folder cards under the cursor for drop targeting.
    pub(crate) fn folder_card_target_at_cursor(
        &self,
        cursor: Point,
        dragged_folder_id: &FolderId,
    ) -> Option<FolderId> {
        let child_folders = self.child_folders();
        let folder_section_top = if self.parent_directory_drop_box_visible() {
            parent_directory_drop_box_height(self) + Spacing::MD
        } else {
            0.0
        };
        folder_card_target_at_cursor(
            cursor,
            &child_folders,
            dragged_folder_id,
            self.library.library_viewport_x,
            self.library.library_viewport_y + folder_section_top,
            self.library.library_scroll_offset,
            self.library_grid_card_width(),
            self.layout().library_folder_grid_row_height,
            self.library_grid_column_gap(),
            Spacing::SM,
            folder_cards_per_row(self),
        )
    }

    /// Hit-test main-content folder cards under the cursor during entry or folder drag.
    pub(crate) fn library_folder_card_target_at_cursor(&self, cursor: Point) -> Option<FolderId> {
        let child_folders = self.child_folders();
        let dragged_folder_sentinel = FolderId::new("__pdf_folio_core_drag__");
        let folder_section_top = if self.parent_directory_drop_box_visible() {
            parent_directory_drop_box_height(self) + Spacing::MD
        } else {
            0.0
        };
        folder_card_target_at_cursor(
            cursor,
            &child_folders,
            &dragged_folder_sentinel,
            self.library.library_viewport_x,
            self.library.library_viewport_y + folder_section_top,
            self.library.library_scroll_offset,
            self.library_grid_card_width(),
            self.layout().library_folder_grid_row_height,
            self.library_grid_column_gap(),
            Spacing::SM,
            folder_cards_per_row(self),
        )
    }

    /// Collapse sidebar folders that were auto-expanded only for the current drag.
    pub(crate) fn collapse_drag_expanded_folders(&mut self, folders: HashSet<FolderId>) {
        for folder_id in folders {
            self.library
                .collapsed_library_tree_folders
                .insert(folder_id);
        }
    }

    /// Whether `folder_id` has at least one child folder in the active tree.
    pub(crate) fn folder_has_children(&self, folder_id: &FolderId) -> bool {
        self.library
            .library_folders
            .iter()
            .any(|folder| folder.parent_id.as_ref() == Some(folder_id))
    }

    /// Convenience wrapper that re-targets using the last known pointer position.
    pub(crate) fn update_library_drag_target_from_cursor(&mut self) {
        let entries = self.visible_library_entries();
        let entries_len = entries.len();
        if entries_len == 0 {
            return;
        }

        let Some(cursor) = self
            .library
            .library_drag
            .as_ref()
            .and_then(|drag| drag.cursor)
        else {
            return;
        };

        let dragged_ids = self
            .library
            .library_drag
            .as_ref()
            .map(|drag| drag.entry_ids.iter().cloned().collect::<HashSet<_>>())
            .unwrap_or_default();
        let compact_entries = entries
            .iter()
            .filter(|entry| !dragged_ids.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();
        let compact_len = compact_entries.len();
        let folder_section_height = folder_cards_section_height(self, self.child_folders().len());
        let content_y = (cursor.y - self.library.library_viewport_y
            + self.library.library_scroll_offset
            - folder_section_height)
            .max(0.0);
        let index = if self.library.compact_view_mode {
            let row_height = self.library_row_height();
            let row_pitch = (row_height + Spacing::SM).max(1.0);
            let row = ((content_y + row_height / 2.0) / row_pitch)
                .floor()
                .max(0.0) as usize;
            row.saturating_mul(self.library_entries_per_row())
        } else {
            let content_x = (cursor.x - self.library.library_viewport_x).max(0.0);
            let per_row = self.library_entries_per_row().max(1);
            let column_step =
                (self.library_grid_card_width() + self.library_grid_column_gap()).max(1.0);
            let column = (content_x / column_step)
                .floor()
                .clamp(0.0, per_row.saturating_sub(1) as f32) as usize;
            let compact_layout = self.library_masonry_layout(&compact_entries);
            self.library_grid_drag_target_index_from_compact_layout(
                &entries,
                &compact_layout,
                column,
                content_x,
                content_y,
            )
            .unwrap_or_else(|| {
                masonry_target_index(&compact_layout, column, content_y).unwrap_or(compact_len)
            })
        };

        let target_index = index.min(compact_len);
        if let Some(drag) = &mut self.library.library_drag {
            drag.target_index = target_index;
        }
    }

    fn library_grid_drag_target_index_from_compact_layout(
        &self,
        entries: &[LibraryEntry],
        compact_layout: &LibraryMasonryLayout,
        column: usize,
        content_x: f32,
        content_y: f32,
    ) -> Option<usize> {
        let drag = self.library.library_drag.as_ref()?;
        if drag.drop_target.is_some() || entries.is_empty() {
            return None;
        }

        let dragged_ids = drag.entry_ids.iter().cloned().collect::<HashSet<_>>();
        let compact_len = entries
            .iter()
            .filter(|entry| !dragged_ids.contains(&entry.id))
            .count();
        if compact_len == 0 {
            return Some(0);
        }

        let card_width = self.library_grid_card_width();
        let column_step = (card_width + self.library_grid_column_gap()).max(1.0);
        if !drag.multi {
            let original_layout = self.library_masonry_layout(entries);
            let entry_index = entries.iter().position(|entry| entry.id == drag.entry_id)?;
            for (column_index, column_items) in original_layout.columns.iter().enumerate() {
                let x = column_index as f32 * column_step;
                for item in column_items {
                    if item.index == entry_index
                        && rect_distance_squared(
                            content_x,
                            content_y,
                            x,
                            item.top,
                            card_width,
                            item.height,
                        ) == 0.0
                    {
                        return Some(drag.source_index.min(compact_len));
                    }
                }
            }
        }

        let current_target_index = drag.target_index.min(compact_len);
        let mut current_drag = drag.clone();
        current_drag.target_index = current_target_index;
        let current_items =
            crate::library::view::library_render_items_for_drag(entries, &current_drag);
        let current_layout = self.library_render_item_masonry_layout(&current_items);
        for (column_index, column_items) in current_layout.columns.iter().enumerate() {
            let x = column_index as f32 * column_step;
            for item in column_items {
                if matches!(
                    current_items.get(item.index),
                    Some(LibraryRenderItem::DropZone(_))
                ) && rect_distance_squared(
                    content_x,
                    content_y,
                    x,
                    item.top,
                    card_width,
                    item.height,
                ) == 0.0
                {
                    return Some(current_target_index);
                }
            }
        }

        let column_items = compact_layout.columns.get(column)?;
        if column_items.is_empty() {
            return Some(compact_len);
        }

        let x = column as f32 * column_step;
        if content_x < x || content_x > x + card_width {
            return masonry_target_index(compact_layout, column, content_y);
        }

        for (position, item) in column_items.iter().enumerate() {
            if content_y < item.top {
                if position == 0 {
                    return Some(item.index);
                }

                let previous = &column_items[position - 1];
                let gap_midpoint = (previous.top + previous.height + item.top) / 2.0;
                return Some(if content_y < gap_midpoint {
                    item.index
                } else {
                    previous.index.saturating_add(1)
                });
            }

            if content_y <= item.top + item.height {
                return Some(if content_y < item.top + item.height / 2.0 {
                    item.index
                } else {
                    item.index.saturating_add(1)
                });
            }
        }

        column_items
            .last()
            .map(|item| item.index.saturating_add(1).min(compact_len))
    }

    /// Estimated scrollable content height for `entries_len` entries in the current layout.
    pub(crate) fn library_content_height_for_len(&self, entries_len: usize) -> f32 {
        if entries_len == 0 {
            return 0.0;
        }

        if !self.library.compact_view_mode {
            return self
                .library_masonry_layout(&self.visible_library_entries())
                .content_height;
        }

        let rows = entries_len.div_ceil(self.library_entries_per_row());
        let row_gap = if self.library.compact_view_mode {
            Spacing::SM
        } else {
            Spacing::MD
        };
        rows as f32 * self.library_row_height() + rows.saturating_sub(1) as f32 * row_gap
    }

    /// Maximum valid `library_scroll_offset` for the current content and viewport.
    pub(crate) fn max_library_scroll_offset(&self) -> f32 {
        let content_height = self
            .library_content_height_for_len(self.visible_library_entries().len())
            + folder_cards_section_height(self, self.child_folders().len());
        (content_height - self.library.library_viewport_height.max(1.0)).max(0.0)
    }

    /// Pixels-per-second autoscroll velocity from edge proximity during drag.
    pub(crate) fn library_drag_auto_scroll_velocity(&self) -> f32 {
        let Some(cursor) = self
            .library
            .library_drag
            .as_ref()
            .and_then(|drag| drag.cursor)
        else {
            return 0.0;
        };

        if !self
            .library
            .library_drag
            .as_ref()
            .is_some_and(|drag| drag.active)
        {
            return 0.0;
        }

        if self.library.library_viewport_height <= 1.0 {
            return 0.0;
        }

        drag_auto_scroll_velocity(
            cursor.y,
            self.library.library_viewport_y,
            self.library.library_viewport_height,
        )
    }

    /// Apply one autoscroll tick during drag and return any scroll/target update tasks.
    pub(crate) fn auto_scroll_library_drag(&mut self, tick: Instant) -> Task<Message> {
        if self.library.library_drag.is_none() && self.library.folder_drag.is_none() {
            return Task::none();
        }

        self.update_folder_drop_target_dwell(tick);

        if self.library.library_drag.is_none() {
            return Task::none();
        }

        let last_tick = self
            .library
            .library_drag
            .as_ref()
            .and_then(|drag| drag.last_auto_scroll_tick)
            .unwrap_or(tick);
        if let Some(drag) = &mut self.library.library_drag {
            drag.last_auto_scroll_tick = Some(tick);
        }

        let dt = tick
            .checked_duration_since(last_tick)
            .map_or(1.0 / 60.0, |duration| {
                duration
                    .as_secs_f32()
                    .clamp(1.0 / 120.0, LIBRARY_DRAG_AUTOSCROLL_MAX_DT)
            });
        let velocity = self.library_drag_auto_scroll_velocity();
        if velocity == 0.0 {
            return Task::none();
        }

        let previous_offset = self.library.library_scroll_offset;
        let next_offset =
            (previous_offset + velocity * dt).clamp(0.0, self.max_library_scroll_offset());
        let delta = next_offset - previous_offset;
        if delta.abs() < 0.5 {
            return Task::none();
        }

        self.library.library_scroll_offset = next_offset;
        self.update_library_drag_target_from_cursor();

        Task::batch([
            scroll_library_to_offset_task(next_offset),
            self.request_visible_thumbnails(),
        ])
    }

    /// Complete an entry drag: drop into folder, reorder manually, or cancel.
    ///
    /// Persists manual order via tasks when the drop changes organization; restores
    /// drag-expanded tree folders and scroll position afterward.
    pub(crate) fn finish_library_drag(&mut self) -> Task<Message> {
        let Some(drag) = self.library.library_drag.take() else {
            return Task::none();
        };
        self.adjust_scroll_for_parent_directory_drop_box(false);

        if !drag.active {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::done(Message::LibraryEntryClicked(drag.entry_id));
        }

        if drag.parent_drop_target {
            let Some(current_folder_id) = self.library.selected_folder.clone() else {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                return scroll_library_to_offset_task(self.library.library_scroll_offset);
            };
            let entry_ids = drag.entry_ids.clone();
            if entry_ids.is_empty() {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                return Task::none();
            }
            if let Some(parent_id) = self.parent_directory_folder_id() {
                self.library.library_status = Some(format!(
                    "Moving {} to parent folder...",
                    format_count(entry_ids.len(), "PDF")
                ));
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                return Task::batch([
                    move_entries_to_folder_task(Arc::clone(&self.db), entry_ids, Some(parent_id)),
                    scroll_library_to_offset_task(self.library.library_scroll_offset),
                ]);
            }

            self.library.library_status = Some(format!(
                "Moving {} to library root...",
                format_count(entry_ids.len(), "PDF")
            ));
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::batch([
                bulk_operation_task(
                    Arc::clone(&self.db),
                    entry_ids,
                    String::from("Moved to parent directory"),
                    String::from("Move PDFs to Parent"),
                    move |db, entry_id| db.remove_entry_from_folder(entry_id, &current_folder_id),
                ),
                scroll_library_to_offset_task(self.library.library_scroll_offset),
            ]);
        }

        if let Some(folder_id) = drag.drop_target.clone() {
            if self.library.selected_folder.as_ref() == Some(&folder_id) {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                return scroll_library_to_offset_task(self.library.library_scroll_offset);
            }
            let entry_ids = drag.entry_ids.clone();
            if entry_ids.is_empty() {
                return Task::none();
            }
            self.library.library_status = Some(format!(
                "Adding {} to folder...",
                format_count(entry_ids.len(), "PDF")
            ));
            return Task::batch([
                add_entries_to_folder_task(Arc::clone(&self.db), entry_ids, folder_id),
                scroll_library_to_offset_task(self.library.library_scroll_offset),
            ]);
        }

        if !self.can_drag_reorder_library() {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return scroll_library_to_offset_task(self.library.library_scroll_offset);
        }

        let entries = self.visible_library_entries();
        let entry_ids: Vec<EntryId> = entries.iter().map(|entry| entry.id.clone()).collect();
        let next_order = reorder_entry_ids_for_drag(&entry_ids, &drag.entry_ids, drag.target_index);
        if next_order == entry_ids {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return scroll_library_to_offset_task(self.library.library_scroll_offset);
        }
        if next_order.len() != entries.len() {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        }

        let entries_by_id = entries
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect::<HashMap<_, _>>();
        let entries = next_order
            .iter()
            .filter_map(|entry_id| entries_by_id.get(entry_id).cloned())
            .collect::<Vec<_>>();

        self.library.library_entries = entries;
        self.collapse_drag_expanded_folders(drag.expanded_during_drag);
        self.library.library_status = Some(if drag.multi {
            format!("Saving manual order for {} PDFs...", drag.entry_ids.len())
        } else {
            String::from("Saving manual PDF order...")
        });
        let persist_order = if let Some(folder_id) = self.library.selected_folder.clone() {
            persist_manual_folder_entry_order_task(Arc::clone(&self.db), folder_id, next_order)
        } else {
            persist_manual_entry_order_task(Arc::clone(&self.db), next_order)
        };
        Task::batch([
            persist_order,
            scroll_library_to_offset_task(self.library.library_scroll_offset),
        ])
    }

    /// Complete a folder drag: nest into target, move to parent, reorder siblings, or treat as click.
    pub(crate) fn finish_folder_drag(&mut self) -> Task<Message> {
        let Some(drag) = self.library.folder_drag.take() else {
            return Task::none();
        };
        self.adjust_scroll_for_parent_directory_drop_box(false);
        let started_in_tree = self.library.folder_drag_started_in_tree;
        self.library.folder_drag_started_in_tree = false;

        if !drag.active {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            let click_message = if started_in_tree {
                Message::FolderTreeClicked(Some(drag.folder_id))
            } else {
                Message::FolderClicked(Some(drag.folder_id))
            };
            return Task::done(click_message);
        }

        if drag.parent_drop_target {
            let parent_id = self.parent_directory_folder_id();
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            self.library.library_status = Some(String::from("Moving folder..."));
            return Task::batch([
                move_folder_task(Arc::clone(&self.db), drag.folder_id, parent_id),
                scroll_library_to_offset_task(self.library.library_scroll_offset),
            ]);
        }

        if let Some(target_id) = drag.drop_target.clone() {
            if !folder_can_move_into(&self.library.library_folders, &drag.folder_id, &target_id) {
                self.collapse_drag_expanded_folders(drag.expanded_during_drag);
                self.library.library_error =
                    Some(String::from("That folder cannot be moved there."));
                return Task::none();
            }

            self.library.library_status = Some(String::from("Moving folder..."));
            self.start_folder_drop_flash(target_id.clone(), Instant::now());
            return Task::batch([
                move_folder_task(Arc::clone(&self.db), drag.folder_id, Some(target_id)),
                scroll_library_to_offset_task(self.library.library_scroll_offset),
            ]);
        }

        let Some(target_id) = drag.pending_drop_target.clone() else {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        };

        let Some((parent_id, next_order)) =
            self.folder_drag_manual_reorder(&drag.folder_id, &target_id)
        else {
            self.collapse_drag_expanded_folders(drag.expanded_during_drag);
            return Task::none();
        };

        self.collapse_drag_expanded_folders(drag.expanded_during_drag);
        self.library.library_status = Some(String::from("Saving manual folder order..."));
        Task::batch([
            persist_manual_folder_order_task(Arc::clone(&self.db), parent_id, next_order),
            scroll_library_to_offset_task(self.library.library_scroll_offset),
        ])
    }
}
