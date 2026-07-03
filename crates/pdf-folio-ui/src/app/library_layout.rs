use super::*;

impl PDFolioApp {
    pub(super) fn visible_library_entries(&self) -> Vec<LibraryEntry> {
        let source = self
            .library
            .search_results
            .as_ref()
            .unwrap_or(&self.library.library_entries);
        source
            .iter()
            .filter(|entry| {
                self.library
                    .active_tag_filter
                    .as_ref()
                    .is_none_or(|tag| entry.tags.iter().any(|entry_tag| entry_tag == tag))
            })
            .filter(|entry| {
                entry_visible_in_folder_scope(entry, self.library.selected_folder.as_ref())
            })
            .filter(|entry| {
                self.library
                    .active_reading_filter
                    .is_none_or(|filter| library_entry_reading_state(entry) == filter)
            })
            .filter(|entry| !self.library.missing_filter_active || entry.missing)
            .cloned()
            .collect()
    }

    pub(super) fn library_grid_zoom(&self) -> f32 {
        self.library
            .library_grid_zoom
            .clamp(LIBRARY_GRID_ZOOM_MIN, self.library_grid_zoom_max())
    }

    pub(super) fn library_grid_zoom_max(&self) -> f32 {
        let width = self.library_available_grid_width();
        (width / self.layout().library_grid_card_width)
            .max(1.0)
            .clamp(1.0, LIBRARY_GRID_ZOOM_MAX)
    }

    pub(super) fn library_available_grid_width(&self) -> f32 {
        let sidebar_width = if self.library.library_tag_sidebar_open {
            self.library.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        let window_main_width = (self.viewer.viewport_width - sidebar_width).max(1.0);
        self.library
            .library_viewport_width
            .max(window_main_width)
            .max(self.layout().window_size()[0] - sidebar_width)
            - Spacing::LG * 2.0
            - self.layout().library_scrollbar_gutter
    }

    pub(super) fn recalculate_library_viewport_width(&mut self) {
        let sidebar_width = if self.library.library_tag_sidebar_open {
            self.library.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        self.library.library_viewport_width =
            (self.viewer.viewport_width - sidebar_width - Spacing::LG * 2.0).max(1.0);
    }

    pub(super) fn fit_library_grid_zoom_to_columns(&mut self, columns: usize) {
        if self.library.compact_view_mode || columns == 0 {
            return;
        }
        let columns = columns.min(LIBRARY_GRID_ZOOM_DENSE_COLUMN_CAP);
        let available_width = self.library_available_grid_width().max(1.0);
        let total_gap = columns.saturating_sub(1) as f32 * self.layout().library_masonry_gap;
        let card_width = ((available_width - total_gap) / columns as f32).max(1.0);
        self.library.library_grid_zoom = (card_width / self.layout().library_grid_card_width)
            .clamp(LIBRARY_GRID_ZOOM_MIN, self.library_grid_zoom_max());
    }

    pub(super) fn library_grid_card_width(&self) -> f32 {
        self.layout().library_grid_card_width * self.library_grid_zoom()
    }

    pub(super) fn library_card_info_height(&self) -> f32 {
        (self.layout().library_card_info_height * self.library_grid_zoom()).clamp(88.0, 176.0)
    }

    pub(super) fn library_card_media_max_height(&self) -> f32 {
        self.layout().library_card_media_max_height * self.library_grid_zoom()
    }

    pub(super) fn library_card_title_width(&self) -> f32 {
        self.layout().library_card_title_width * self.library_grid_zoom()
    }

    pub(super) fn library_card_text_scale(&self) -> f32 {
        self.library_grid_zoom().clamp(0.55, 1.35)
    }

    pub(super) fn library_card_font_size(&self, base_size: u32) -> u32 {
        ((base_size as f32) * self.library_card_text_scale())
            .round()
            .clamp(8.0, 28.0) as u32
    }

    pub(super) fn library_card_padding(&self) -> f32 {
        (Spacing::LG * self.library_card_text_scale()).clamp(4.0, 24.0)
    }

    pub(super) fn library_card_spacing(&self) -> f32 {
        (Spacing::SM * self.library_card_text_scale()).clamp(2.0, Spacing::SM)
    }

    pub(super) fn library_card_title_font_size(&self) -> u32 {
        self.library_card_font_size(16)
    }

    pub(super) fn thumbnail_size_for_grid_zoom(&self) -> ThumbnailSize {
        let width = self.library_grid_card_width();
        if width <= 140.0 {
            ThumbnailSize::Small
        } else if width >= 340.0 {
            ThumbnailSize::Large
        } else {
            ThumbnailSize::Default
        }
    }

    pub(super) fn thumbnail_for_entry(
        &self,
        entry_id: &EntryId,
        preferred_size: ThumbnailSize,
    ) -> Option<&ThumbnailView> {
        [
            preferred_size,
            ThumbnailSize::Default,
            ThumbnailSize::Large,
            ThumbnailSize::Small,
        ]
        .into_iter()
        .find_map(|size| {
            self.library.thumbnails.get(&ThumbnailCacheKey {
                entry_id: entry_id.clone(),
                size,
            })
        })
    }

    pub(super) fn library_grid_zoom_label(&self) -> String {
        format!("{:.0}%", self.library_grid_zoom() * 100.0)
    }
}
