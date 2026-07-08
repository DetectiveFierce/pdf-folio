use super::*;

fn rect_distance_squared(px: f32, py: f32, x: f32, y: f32, width: f32, height: f32) -> f32 {
    let closest_x = px.clamp(x, x + width.max(0.0));
    let closest_y = py.clamp(y, y + height.max(0.0));
    let dx = px - closest_x;
    let dy = py - closest_y;
    dx * dx + dy * dy
}

impl PDFolioApp {
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

    pub(crate) fn begin_folder_tree_drag(&mut self, folder_id: FolderId) {
        self.begin_folder_drag(folder_id);
        if self.library.folder_drag.is_some() {
            self.library.folder_drag_started_in_tree = true;
        }
    }

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

    pub(crate) fn set_folder_drag_card_target(&mut self, folder_id: Option<FolderId>) {
        let Some(drag) = &mut self.library.folder_drag else {
            return;
        };
        let target = folder_id.filter(|target| {
            folder_can_move_into(&self.library.library_folders, &drag.folder_id, target)
        });
        drag.set_drop_target(target, Instant::now(), true);
    }

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

    pub(crate) fn active_folder_drop_target(&self) -> Option<&FolderId> {
        active_folder_drop_target(
            self.library.library_drag.as_ref(),
            self.library.folder_drag.as_ref(),
        )
    }

    pub(crate) fn parent_directory_drop_box_visible(&self) -> bool {
        self.library.active_tag_filter.is_none()
            && self.library.selected_folder.is_some()
            && (self.library.library_drag.is_some() || self.library.folder_drag.is_some())
    }

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

    pub(crate) fn parent_directory_folder_id(&self) -> Option<FolderId> {
        let selected_folder = self.library.selected_folder.as_ref()?;
        self.library
            .library_folders
            .iter()
            .find(|folder| &folder.id == selected_folder)
            .and_then(|folder| folder.parent_id.clone())
    }

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

    pub(crate) fn library_folder_card_target_at_cursor(&self, cursor: Point) -> Option<FolderId> {
        let child_folders = self.child_folders();
        let dragged_folder_sentinel = FolderId::new("__pdf_folio_db_drag__");
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

    pub(crate) fn collapse_drag_expanded_folders(&mut self, folders: HashSet<FolderId>) {
        for folder_id in folders {
            self.library
                .collapsed_library_tree_folders
                .insert(folder_id);
        }
    }

    pub(crate) fn folder_has_children(&self, folder_id: &FolderId) -> bool {
        self.library
            .library_folders
            .iter()
            .any(|folder| folder.parent_id.as_ref() == Some(folder_id))
    }

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
            let Some(entry_index) = entries.iter().position(|entry| entry.id == drag.entry_id)
            else {
                return None;
            };
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

    pub(crate) fn max_library_scroll_offset(&self) -> f32 {
        let content_height = self
            .library_content_height_for_len(self.visible_library_entries().len())
            + folder_cards_section_height(self, self.child_folders().len());
        (content_height - self.library.library_viewport_height.max(1.0)).max(0.0)
    }

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
