use super::*;

impl PDFolioApp {
    pub(super) fn visible_library_entry_window_at(
        &self,
        entries_len: usize,
        scroll_offset: f32,
    ) -> std::ops::Range<usize> {
        if entries_len == 0 {
            return 0..0;
        }

        let per_row = self.library_entries_per_row();
        let row_height = self.library_row_height();
        let first_row = (scroll_offset / row_height).floor().max(0.0) as usize;
        let visible_rows = (self.library.library_viewport_height / row_height)
            .ceil()
            .max(1.0) as usize;
        let start_row = first_row.saturating_sub(self.layout().library_overscan_rows);
        let end_row = first_row
            .saturating_add(visible_rows)
            .saturating_add(self.layout().library_overscan_rows)
            .saturating_add(1);

        let start = (start_row * per_row).min(entries_len);
        let end = (end_row * per_row).min(entries_len);
        start..end
    }

    pub(super) fn visible_library_masonry_layout_items_at<'a>(
        &self,
        layout: &'a LibraryMasonryLayout,
        scroll_offset: f32,
    ) -> Vec<&'a LibraryMasonryItem> {
        let top = scroll_offset.max(0.0)
            - self.layout().library_overscan_rows as f32 * self.library_row_height();
        let bottom = scroll_offset.max(0.0)
            + self.library.library_viewport_height.max(1.0)
            + self.layout().library_overscan_rows as f32 * self.library_row_height();
        let mut items = layout
            .columns
            .iter()
            .flat_map(|column| column.iter())
            .filter(|item| item.top + item.height >= top && item.top <= bottom)
            .collect::<Vec<_>>();
        items.sort_by_key(|item| item.index);
        items
    }

    pub(super) fn library_entries_per_row(&self) -> usize {
        if self.library.compact_view_mode {
            1
        } else {
            let available_width = self.library_available_grid_width();
            let column_pitch = self.library_grid_card_width() + self.layout().library_masonry_gap;
            ((available_width + self.layout().library_masonry_gap) / column_pitch)
                .floor()
                .max(1.0)
                .min(self.library_grid_dense_column_cap() as f32) as usize
        }
    }

    pub(super) fn library_row_height(&self) -> f32 {
        if self.library.compact_view_mode {
            self.layout().library_list_row_height + self.library_row_hover_lift()
        } else {
            self.layout().library_grid_row_height * self.library_grid_zoom()
        }
    }

    pub(super) fn library_masonry_layout(&self, entries: &[LibraryEntry]) -> LibraryMasonryLayout {
        let column_count = self.library_entries_per_row().max(1);
        let mut columns = vec![Vec::new(); column_count];
        let mut column_heights = vec![0.0; column_count];

        for (index, entry) in entries.iter().enumerate() {
            let column = shortest_column_index(&column_heights);
            let top = column_heights[column];
            let height = self.library_card_estimated_height(&entry.id);
            columns[column].push(LibraryMasonryItem { index, top, height });
            column_heights[column] = top + height + self.layout().library_masonry_gap;
        }

        let content_height = column_heights
            .into_iter()
            .map(|height| (height - self.layout().library_masonry_gap).max(0.0))
            .fold(0.0, f32::max);

        LibraryMasonryLayout {
            columns,
            content_height,
        }
    }

    pub(super) fn library_render_item_masonry_layout(
        &self,
        items: &[LibraryRenderItem],
    ) -> LibraryMasonryLayout {
        let entries = items
            .iter()
            .map(LibraryRenderItem::entry)
            .cloned()
            .collect::<Vec<_>>();
        self.library_masonry_layout(&entries)
    }

    pub(super) fn library_card_estimated_height(&self, entry_id: &EntryId) -> f32 {
        let thumbnail_height = self
            .thumbnail_for_entry(entry_id, self.thumbnail_size_for_grid_zoom())
            .map(|thumbnail| {
                let height = self.library_grid_card_width() * f32::from(thumbnail.height)
                    / f32::from(thumbnail.width.max(1));
                height.min(self.library_card_media_max_height())
            })
            .unwrap_or(self.library_card_media_max_height());

        thumbnail_height + self.library_card_info_height() + self.library_card_hover_lift()
    }

    pub(super) fn library_card_hover_progress(&self, entry_id: &EntryId) -> f32 {
        self.library
            .library_card_hover_animations
            .get(entry_id)
            .map(|animation| animation.interpolate(0.0, 1.0, self.library.animation_now))
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }

    pub(super) fn set_library_card_hover(&mut self, entry_id: EntryId, hovered: bool) {
        self.library.animation_now = Instant::now();
        let animation = self
            .library
            .library_card_hover_animations
            .entry(entry_id)
            .or_insert_with(Self::library_card_hover_animation);
        animation.go_mut(hovered, self.library.animation_now);
    }

    pub(super) fn tick_animations(&mut self, now: Instant) {
        self.library.animation_now = now;
        let visible_entry_ids = self
            .visible_library_entries()
            .into_iter()
            .map(|entry| entry.id)
            .collect::<HashSet<_>>();
        self.library
            .library_card_hover_animations
            .retain(|entry_id, animation| {
                animation.is_animating(now) || visible_entry_ids.contains(entry_id)
            });
        self.expire_folder_drop_flash(now);
        self.expire_viewer_page_fades(now);
    }

    pub(super) fn start_bulk_operation_progress(&mut self, label: impl Into<String>, total: usize) {
        let label = label.into();
        self.library.library_status = Some(format!("{label} {total} PDFs..."));
        self.library.bulk_operation_progress = Some(BulkOperationProgress {
            label,
            total,
            started_at: Instant::now(),
        });
    }

    pub(super) fn start_folder_drop_flash(&mut self, folder_id: FolderId, now: Instant) {
        self.library.folder_drop_flash = Some((folder_id, now));
        self.library.animation_now = now;
    }

    pub(super) fn expire_folder_drop_flash(&mut self, now: Instant) {
        if self
            .library
            .folder_drop_flash
            .as_ref()
            .is_some_and(|(_, started_at)| {
                now.saturating_duration_since(*started_at)
                    >= Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
            })
        {
            self.library.folder_drop_flash = None;
        }
    }

    pub(super) fn folder_drop_flash_active(&self, folder_id: &FolderId) -> bool {
        folder_drop_flash_active_at(
            folder_id,
            self.library
                .folder_drop_flash
                .as_ref()
                .map(|(flashed_folder_id, started_at)| (flashed_folder_id, *started_at)),
            self.library.animation_now,
        )
    }

    pub(super) fn library_card_hover_animation() -> Animation<bool> {
        Animation::new(false)
            .duration(Duration::from_millis(LIBRARY_CARD_HOVER_DURATION_MS))
            .easing(animation::Easing::EaseOutCubic)
    }

    pub(super) fn library_card_hover_animation_active(&self) -> bool {
        self.library
            .library_card_hover_animations
            .values()
            .any(|animation| animation.is_animating(self.library.animation_now))
    }

    pub(super) fn expire_viewer_page_fades(&mut self, now: Instant) {
        self.viewer.page_fade_started.retain(|_, started_at| {
            now.saturating_duration_since(*started_at) < Duration::from_millis(VIEWER_PAGE_FADE_MS)
        });
    }

    pub(super) fn viewer_page_fade_active(&self) -> bool {
        !self.viewer.page_fade_started.is_empty()
    }

    pub(super) fn clear_library_transient_interactions(&mut self) {
        self.library.library_card_hover_animations.clear();
        self.library.folder_drop_flash = None;
        self.library.library_drag = None;
        self.library.folder_drag = None;
        self.library.resizing_library_tag_sidebar = false;
    }
}
