use super::*;

impl PDFolioApp {
    pub(super) fn page_top(&self, target_page: u16) -> f32 {
        self.viewer_page_rect_for_page(target_page)
            .map_or(Spacing::PAGE_GUTTER, |rect| rect.y)
    }

    pub(super) fn jump_to_page(&mut self, page: u16) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };

        let page = page.min(doc.page_count().saturating_sub(1));
        if let Some(rect) = self.viewer_page_rect_for_page(page) {
            self.viewer.last_scroll_offset = self.viewer.scroll_offset;
            if matches!(self.viewer.viewer_scroll_mode, ViewerScrollMode::Horizontal) {
                self.viewer.horizontal_offset = rect.x;
                self.viewer.scroll_offset = 0.0;
            } else {
                self.viewer.scroll_offset = rect.y;
                if matches!(self.viewer.viewer_scroll_mode, ViewerScrollMode::Wrapped) {
                    self.viewer.horizontal_offset = 0.0;
                }
            }
        }
        self.clamp_scroll_offset();
        self.clamp_horizontal_offset();
        self.viewer.jump_dialog_open = false;
        self.viewer.page_input_editing = false;
        self.viewer.jump_input.clear();
        self.request_visible_pages()
    }

    pub(super) fn scroll_to_page_rect(&mut self, page: u16, x_fraction: f32, y_fraction: f32) {
        let Some(rect) = self.viewer_page_rect_for_page(page) else {
            return;
        };
        let target_x = rect.x + rect.width * x_fraction - self.viewer.viewer_viewport_width * 0.25;
        let target_y =
            rect.y + rect.height * y_fraction - self.viewer.viewer_viewport_height * 0.25;

        if matches!(self.viewer.viewer_scroll_mode, ViewerScrollMode::Horizontal) {
            self.viewer.horizontal_offset = target_x.max(0.0);
            self.viewer.scroll_offset = 0.0;
        } else {
            self.viewer.scroll_offset = target_y.max(0.0);
            if matches!(self.viewer.viewer_scroll_mode, ViewerScrollMode::Wrapped) {
                self.viewer.horizontal_offset = 0.0;
            }
        }
    }

    pub(super) fn max_horizontal_offset(&self) -> f32 {
        (self.content_width() - self.viewer.viewer_viewport_width.max(1.0)).max(0.0)
    }

    pub(super) fn max_scroll_offset(&self) -> f32 {
        (self.content_height() - self.viewer.viewer_viewport_height.max(1.0)).max(0.0)
    }

    pub(super) fn clamp_horizontal_offset(&mut self) {
        self.viewer.horizontal_offset = self
            .viewer
            .horizontal_offset
            .clamp(0.0, self.max_horizontal_offset());
    }

    pub(super) fn clamp_scroll_offset(&mut self) {
        self.viewer.scroll_offset = self
            .viewer
            .scroll_offset
            .clamp(0.0, self.max_scroll_offset());
    }

    pub(super) fn scroll_by(&mut self, delta: f32) -> Task<Message> {
        self.viewer.last_scroll_offset = self.viewer.scroll_offset;
        self.viewer.scroll_offset =
            (self.viewer.scroll_offset + delta).clamp(0.0, self.max_scroll_offset());
        self.request_visible_pages()
    }

    pub(super) fn scroll_page_mode_by(&mut self, direction: i16) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let current = i32::from(self.current_page());
        let page_count = i32::from(doc.page_count());
        let next = (current + i32::from(direction)).clamp(0, page_count.saturating_sub(1));
        self.jump_to_page(next as u16)
    }

    pub(super) fn pan_horizontally_by(&mut self, delta: f32) {
        self.viewer.horizontal_offset =
            (self.viewer.horizontal_offset + delta).clamp(0.0, self.max_horizontal_offset());
    }

    pub(super) fn set_viewer_scroll_mode(&mut self, mode: ViewerScrollMode) -> Task<Message> {
        if self.viewer.viewer_scroll_mode == mode {
            return Task::none();
        }
        let current_page = self.current_page();
        self.viewer.viewer_scroll_mode = mode;
        self.viewer.horizontal_offset = 0.0;
        self.viewer.scroll_offset = 0.0;
        let zoom_task = self.apply_active_dimension_zoom();
        let page_task = self.jump_to_page(current_page);
        Task::batch([zoom_task, page_task])
    }

    pub(super) fn set_viewer_spread_mode(&mut self, mode: ViewerSpreadMode) -> Task<Message> {
        if self.viewer.viewer_spread_mode == mode {
            return Task::none();
        }
        let current_page = self.current_page();
        self.viewer.viewer_spread_mode = mode;
        self.viewer.horizontal_offset = 0.0;
        self.viewer.scroll_offset = 0.0;
        let zoom_task = self.apply_active_dimension_zoom();
        let page_task = self.jump_to_page(current_page);
        Task::batch([zoom_task, page_task])
    }

    pub(super) fn zoom_to_width(
        &mut self,
        width: u16,
        cursor: Option<Point>,
        render_policy: ZoomRenderPolicy,
    ) -> Task<Message> {
        let previous_width = self.viewer.zoom_width;
        let new_width = width.clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH);

        if new_width == previous_width {
            return Task::none();
        }

        if matches!(render_policy, ZoomRenderPolicy::Debounced) {
            let preview_width_px = self.render_width_px();
            self.viewer
                .zoom_preview_width_px
                .get_or_insert(preview_width_px);
        } else {
            self.viewer.zoom_preview_width_px = None;
        }

        let anchor = cursor.map(|cursor| {
            let ratio = f32::from(new_width) / f32::from(previous_width);
            let old_x = self.viewer.horizontal_offset + cursor.x;
            let old_y = self.viewer.scroll_offset + cursor.y;
            ((old_x * ratio) - cursor.x, (old_y * ratio) - cursor.y)
        });

        self.viewer.zoom_width = new_width;
        if !self.viewer.zoom_editing {
            self.viewer.zoom_input = zoom_percent_label(new_width);
        }
        self.viewer.zoom_menu_open = false;
        self.viewer.zoom_generation = self.viewer.zoom_generation.wrapping_add(1);
        let generation = self.viewer.zoom_generation;

        if let Some((x, y)) = anchor {
            self.viewer.horizontal_offset = x.clamp(0.0, self.max_horizontal_offset());
            self.viewer.scroll_offset = y.clamp(0.0, self.max_scroll_offset());
        }

        self.clamp_horizontal_offset();

        match render_policy {
            ZoomRenderPolicy::Immediate => self.request_visible_pages(),
            ZoomRenderPolicy::Debounced => schedule_zoom_render(generation),
        }
    }

    pub(super) fn rendered_page_for_draw(&self, key: TileKey) -> Option<&RenderedPageView> {
        selected_render_key(
            self.viewer.rendered_pages.keys(),
            key,
            self.viewer.zoom_preview_width_px,
            true,
        )
        .and_then(|key| self.viewer.rendered_pages.get(&key))
    }

    pub(super) fn fallback_rendered_page_for_draw(
        &self,
        key: TileKey,
    ) -> Option<&RenderedPageView> {
        selected_render_key(
            self.viewer.rendered_pages.keys(),
            key,
            self.viewer.zoom_preview_width_px,
            false,
        )
        .and_then(|key| self.viewer.rendered_pages.get(&key))
    }

    pub(super) fn page_fade_progress(&self, key: TileKey) -> Option<f32> {
        let started = self.viewer.page_fade_started.get(&key)?;
        let elapsed = Instant::now().saturating_duration_since(*started);
        Some((elapsed.as_secs_f32() / (VIEWER_PAGE_FADE_MS as f32 / 1000.0)).clamp(0.0, 1.0))
    }

    pub(super) fn all_visible_pages_rendered_at_current_zoom(&self) -> bool {
        self.visible_page_range().all(|page| {
            self.viewer.rendered_pages.contains_key(&TileKey {
                page,
                width_px: self.render_width_px(),
            })
        })
    }

    pub(super) fn title(&self) -> String {
        if self.mode == AppMode::Library {
            return String::from("PDF-Folio");
        }

        self.viewer
            .doc
            .as_ref()
            .and_then(|doc| doc.path().file_name())
            .and_then(|name| name.to_str())
            .map(|name| format!("{name} - PDF-Folio"))
            .unwrap_or_else(|| String::from("PDF-Folio"))
    }
}
