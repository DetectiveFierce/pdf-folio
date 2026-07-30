//! # Library layout metrics and visible entry projection
//!
//! Zoom limits, column caps, and the primary **`visible_library_entries`**
//! pipeline: apply tag/folder/reading/missing/recent filters and manual
//! sort within the selected folder.
//!
//! ## Ownership
//!
//! Domain geometry bound to `PDFolioApp` layout tokens and library filter
//! state. Filter predicates themselves are pure helpers from
//! `components::library::filters`.
//!
//! Consumers: view composition, drag hit-testing, thumbnail windowing, and
//! selection pruning after filter changes.

use crate::*;

fn entry_folder_manual_order(entry: &LibraryEntry, folder_id: &FolderId) -> i64 {
    entry
        .folder_orders
        .iter()
        .find(|membership| &membership.folder_id == folder_id)
        .map(|membership| membership.manual_order)
        .unwrap_or(entry.manual_order)
}

impl PDFolioApp {
    /// Minimum grid zoom factor from layout metrics.
    pub(crate) fn library_grid_zoom_min(&self) -> f32 {
        self.layout()
            .metric("LibraryInteraction", "grid_zoom_min", 0.25)
    }

    /// Maximum grid zoom factor from layout metrics.
    pub(crate) fn library_grid_zoom_limit(&self) -> f32 {
        self.layout()
            .metric("LibraryInteraction", "grid_zoom_max", 12.0)
    }

    /// Discrete zoom step for wheel/button adjustments.
    pub(crate) fn library_grid_zoom_step(&self) -> f32 {
        self.layout()
            .metric("LibraryInteraction", "grid_zoom_step", 0.05)
    }

    /// Hard cap on masonry columns at minimum zoom.
    pub(crate) fn library_grid_dense_column_cap(&self) -> usize {
        self.layout()
            .count("LibraryInteraction", "grid_zoom_dense_column_cap", 28)
    }

    /// Extra vertical space reserved for card hover elevation.
    pub(crate) fn library_card_hover_lift(&self) -> f32 {
        self.layout()
            .metric("LibraryInteraction", "card_hover_lift", 2.0)
    }

    /// Extra vertical space reserved for list-row hover elevation.
    pub(crate) fn library_row_hover_lift(&self) -> f32 {
        self.layout()
            .metric("LibraryInteraction", "row_hover_lift", 1.0)
    }

    /// Entries shown in the main pane after search, tag, folder, and smart filters.
    pub(crate) fn visible_library_entries(&self) -> Vec<LibraryEntry> {
        let source = self
            .library
            .search_results
            .as_ref()
            .unwrap_or_else(|| self.active_library_entries());
        let filter_by_selected_folder = self.library.active_tag_filter.is_none();
        let mut entries = source
            .iter()
            .filter(|entry| {
                self.library
                    .active_tag_filter
                    .as_ref()
                    .is_none_or(|tag| entry.tags.iter().any(|entry_tag| entry_tag == tag))
            })
            .filter(|entry| {
                !filter_by_selected_folder
                    || entry_visible_in_folder_scope(entry, self.library.selected_folder.as_ref())
            })
            .filter(|entry| {
                self.library
                    .active_reading_filter
                    .is_none_or(|filter| library_entry_reading_state(entry) == filter)
            })
            .filter(|entry| {
                !self.library.active_recently_opened_filter || entry.opened_at.is_some()
            })
            .filter(|entry| !self.library.missing_filter_active || entry.missing)
            .cloned()
            .collect::<Vec<_>>();

        if self.library.active_recently_opened_filter {
            entries.sort_by(|left, right| {
                right
                    .opened_at
                    .cmp(&left.opened_at)
                    .then_with(|| left.manual_order.cmp(&right.manual_order))
            });
        } else if self.library.library_sort_mode == LibrarySortMode::Manual {
            if let Some(folder_id) = self.library.selected_folder.as_ref() {
                entries.sort_by(|left, right| {
                    entry_folder_manual_order(left, folder_id)
                        .cmp(&entry_folder_manual_order(right, folder_id))
                        .then_with(|| left.manual_order.cmp(&right.manual_order))
                });
            }
        }

        entries
    }

    /// Live or trash entry list depending on `trash_view_active`.
    pub(crate) fn active_library_entries(&self) -> &Vec<LibraryEntry> {
        if self.library.trash_view_active {
            &self.library.library_trash_entries
        } else {
            &self.library.library_entries
        }
    }

    /// Current grid zoom factor clamped to layout min/max.
    pub(crate) fn library_grid_zoom(&self) -> f32 {
        self.library
            .library_grid_zoom
            .clamp(self.library_grid_zoom_min(), self.library_grid_zoom_max())
    }

    /// Alias for the configured maximum grid zoom.
    pub(crate) fn library_grid_zoom_max(&self) -> f32 {
        let width = self.library_available_grid_width();
        (width / self.layout().library_grid_card_width)
            .max(1.0)
            .clamp(1.0, self.library_grid_zoom_limit())
    }

    /// Width available for masonry columns after sidebars and padding.
    pub(crate) fn library_available_grid_width(&self) -> f32 {
        let sidebar_width = if self.library.library_tag_sidebar_open {
            self.library.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        let inspector_width = if self.library.library_inspector_open {
            self.library.library_inspector_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        let estimated_width =
            (self.viewer.viewport_width - sidebar_width - inspector_width - Spacing::LG * 2.0)
                .max(1.0);
        let viewport_width = self.library.library_viewport_width.min(estimated_width);
        (viewport_width - self.layout().library_scrollbar_gutter).max(1.0)
    }

    /// Recompute content viewport width after sidebar/inspector open or resize.
    pub(crate) fn recalculate_library_viewport_width(&mut self) {
        let sidebar_width = if self.library.library_tag_sidebar_open {
            self.library.library_tag_sidebar_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        let inspector_width = if self.library.library_inspector_open {
            self.library.library_inspector_width + self.layout().sidebar_resize_handle_width
        } else {
            0.0
        };
        self.library.library_viewport_width =
            (self.viewer.viewport_width - sidebar_width - inspector_width - Spacing::LG * 2.0)
                .max(1.0);
    }

    /// Adjust zoom so the grid keeps approximately `columns` columns after a chrome change.
    pub(crate) fn fit_library_grid_zoom_to_columns(&mut self, columns: usize) {
        if self.library.compact_view_mode || columns == 0 {
            return;
        }
        let columns = columns.min(self.library_grid_dense_column_cap());
        let available_width = self.library_available_grid_width().max(1.0);
        let total_gap = columns.saturating_sub(1) as f32 * self.layout().library_masonry_gap;
        let card_width = ((available_width - total_gap) / columns as f32).max(1.0);
        self.library.library_grid_zoom = (card_width / self.layout().library_grid_card_width)
            .clamp(self.library_grid_zoom_min(), self.library_grid_zoom_max());
    }

    /// Horizontal/vertical gap between masonry cards.
    pub(crate) fn library_grid_column_gap(&self) -> f32 {
        self.layout().library_masonry_gap
    }

    /// Ideal card width implied by current zoom before column packing.
    pub(crate) fn library_grid_target_card_width(&self) -> f32 {
        self.layout().library_grid_card_width * self.library_grid_zoom()
    }

    /// Actual card width after dividing available width by column count.
    pub(crate) fn library_grid_card_width(&self) -> f32 {
        if self.library.compact_view_mode {
            return self.library_grid_target_card_width();
        }

        let columns = self.library_entries_per_row().max(1);
        let total_gap = columns.saturating_sub(1) as f32 * self.library_grid_column_gap();
        ((self.library_available_grid_width() - total_gap) / columns as f32).max(1.0)
    }

    /// Fixed chrome height under the thumbnail for title/meta at the current density.
    pub(crate) fn library_card_info_height(&self) -> f32 {
        (self.layout().library_card_info_height * self.library_grid_zoom()).clamp(88.0, 176.0)
    }

    /// Cap on thumbnail media height inside a card.
    pub(crate) fn library_card_media_max_height(&self) -> f32 {
        self.layout().library_card_media_max_height * self.library_grid_zoom()
    }

    /// Text measure width for card titles.
    pub(crate) fn library_card_title_width(&self) -> f32 {
        self.layout().library_card_title_width * self.library_grid_zoom()
    }

    /// Font scale factor for card text at the current zoom.
    pub(crate) fn library_card_text_scale(&self) -> f32 {
        self.library_grid_zoom().clamp(0.55, 1.35)
    }

    /// Resolved card body font size in points.
    pub(crate) fn library_card_font_size(&self, base_size: u32) -> u32 {
        ((base_size as f32) * self.library_card_text_scale())
            .round()
            .clamp(8.0, 28.0) as u32
    }

    /// Inner padding for library cards.
    pub(crate) fn library_card_padding(&self) -> f32 {
        (Spacing::LG * self.library_card_text_scale()).clamp(4.0, 24.0)
    }

    /// Vertical spacing between card text rows.
    pub(crate) fn library_card_spacing(&self) -> f32 {
        (Spacing::SM * self.library_card_text_scale()).clamp(2.0, Spacing::SM)
    }

    /// Resolved title font size for cards.
    pub(crate) fn library_card_title_font_size(&self) -> u32 {
        self.library_card_font_size(16)
    }

    /// Choose Small/Default/Large cover tier for the current zoom.
    pub(crate) fn thumbnail_size_for_grid_zoom(&self) -> ThumbnailSize {
        let width = self.library_grid_card_width();
        if width <= 140.0 {
            ThumbnailSize::Small
        } else if width >= 340.0 {
            ThumbnailSize::Large
        } else {
            ThumbnailSize::Default
        }
    }

    /// Best available cached thumbnail for an entry at the preferred size (with fallbacks).
    pub(crate) fn thumbnail_for_entry(
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

    /// Human-readable zoom percentage for toolbar controls.
    pub(crate) fn library_grid_zoom_label(&self) -> String {
        format!("{:.0}%", self.library_grid_zoom() * 100.0)
    }
}
