use super::*;

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
