//! Page navigation, scroll clamping, and zoom helpers for the open document.
//!
//! Implements `PDFolioApp` methods used by the viewer updater, shortcuts, and
//! canvas interaction: jump-to-page, continuous and page-mode scrolling,
//! horizontal pan, scroll/spread mode changes, and cursor-anchored zoom.
//!
//! Tasks returned typically batch `request_visible_pages` (tile fan-out) with
//! `scroll_viewer_to_offsets_task` (iced scrollable sync). Debounced zoom uses
//! [`super::tasks::schedule_zoom_render`] so wheel gestures do not thrash the
//! renderer.
//!
//! Related: [`super::layout`] for geometry, [`super::rendering`] for zoom
//! presets and [`ZoomRenderPolicy`], [`super::update`] for message wiring.

use crate::viewer::layout::selected_render_key;
use crate::*;

impl PDFolioApp {
    /// Y origin of `target_page` in document content coordinates.
    pub(crate) fn page_top(&self, target_page: u16) -> f32 {
        self.viewer_page_rect_for_page(target_page)
            .map_or(Spacing::PAGE_GUTTER, |rect| rect.y)
    }

    /// Jumps to a zero-based page, updating scroll offsets for the active mode.
    ///
    /// Closes the jump dialog, clamps the page index, requests visible tiles,
    /// and scrolls the viewer scrollable to the new offsets.
    pub(crate) fn jump_to_page(&mut self, page: u16) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };

        let page = page.min(doc.page_count().saturating_sub(1));
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            self.viewer.last_scroll_offset = self.viewer.scroll_offset;
            self.viewer.page_scroll_page = page;
            self.viewer.scroll_offset = 0.0;
            self.viewer.horizontal_offset = 0.0;
        } else if let Some(rect) = self.viewer_page_rect_for_page(page) {
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
        Task::batch([
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Scrolls so a fractional point inside `page` is near the viewport center.
    ///
    /// Used by find-in-document and similar “reveal this location” flows.
    pub(crate) fn scroll_to_page_rect(&mut self, page: u16, x_fraction: f32, y_fraction: f32) {
        self.scroll_to_page_rect_with_bias(page, x_fraction, y_fraction, 0.25, 0.25);
    }

    /// Scrolls so a fractional point inside `page` is at the viewport center
    /// (mockup annotation reveal: `scrollIntoView({ block: 'center' })`).
    pub(crate) fn scroll_to_page_rect_centered(
        &mut self,
        page: u16,
        x_fraction: f32,
        y_fraction: f32,
    ) {
        self.scroll_to_page_rect_with_bias(page, x_fraction, y_fraction, 0.5, 0.5);
    }

    fn scroll_to_page_rect_with_bias(
        &mut self,
        page: u16,
        x_fraction: f32,
        y_fraction: f32,
        x_bias: f32,
        y_bias: f32,
    ) {
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            self.viewer.page_scroll_page = page;
        }

        let Some(rect) = self.viewer_page_rect_for_page(page) else {
            return;
        };
        let target_x =
            rect.x + rect.width * x_fraction - self.viewer.viewer_viewport_width * x_bias;
        let target_y =
            rect.y + rect.height * y_fraction - self.viewer.viewer_viewport_height * y_bias;

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

    /// Maximum legal horizontal scroll offset for the current content size.
    pub(crate) fn max_horizontal_offset(&self) -> f32 {
        (self.content_width() - self.viewer.viewer_viewport_width.max(1.0)).max(0.0)
    }

    /// Maximum legal vertical scroll offset for the current content size.
    pub(crate) fn max_scroll_offset(&self) -> f32 {
        (self.content_height() - self.viewer.viewer_viewport_height.max(1.0)).max(0.0)
    }

    /// iced task that sets the viewer scrollable absolute offset from runtime state.
    pub(crate) fn scroll_viewer_to_offsets_task(&self) -> Task<Message> {
        operation::scroll_to(
            Id::new(VIEWER_SCROLLABLE_ID),
            operation::AbsoluteOffset {
                x: Some(self.viewer.horizontal_offset.max(0.0)),
                y: Some(self.viewer.scroll_offset.max(0.0)),
            },
        )
    }

    /// Clamps `horizontal_offset` into `[0, max_horizontal_offset()]`.
    pub(crate) fn clamp_horizontal_offset(&mut self) {
        self.viewer.horizontal_offset = self
            .viewer
            .horizontal_offset
            .clamp(0.0, self.max_horizontal_offset());
    }

    /// Clamps `scroll_offset` into `[0, max_scroll_offset()]`.
    pub(crate) fn clamp_scroll_offset(&mut self) {
        self.viewer.scroll_offset = self
            .viewer
            .scroll_offset
            .clamp(0.0, self.max_scroll_offset());
    }

    /// Nudges vertical scroll by `delta` pixels and refreshes visible pages.
    pub(crate) fn scroll_by(&mut self, delta: f32) -> Task<Message> {
        self.viewer.last_scroll_offset = self.viewer.scroll_offset;
        self.viewer.scroll_offset =
            (self.viewer.scroll_offset + delta).clamp(0.0, self.max_scroll_offset());
        Task::batch([
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Advances or rewinds one page in page-scroll mode (`direction` ±1).
    pub(crate) fn scroll_page_mode_by(&mut self, direction: i16) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let current = i32::from(self.current_page());
        let page_count = i32::from(doc.page_count());
        let next = (current + i32::from(direction)).clamp(0, page_count.saturating_sub(1));
        self.viewer.last_scroll_offset = self.viewer.scroll_offset;
        self.viewer.page_scroll_page = next as u16;
        self.viewer.scroll_offset = 0.0;
        self.viewer.horizontal_offset = 0.0;
        Task::batch([
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Nudges horizontal scroll by `delta` pixels and syncs the iced scrollable.
    pub(crate) fn pan_horizontally_by(&mut self, delta: f32) -> Task<Message> {
        self.viewer.horizontal_offset =
            (self.viewer.horizontal_offset + delta).clamp(0.0, self.max_horizontal_offset());
        Task::batch([
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Switches scroll mode, re-applying dimension zoom and jumping to the current page.
    pub(crate) fn set_viewer_scroll_mode(&mut self, mode: ViewerScrollMode) -> Task<Message> {
        if self.viewer.viewer_scroll_mode == mode {
            return Task::none();
        }
        let current_page = self.current_page();
        self.viewer.viewer_scroll_mode = mode;
        if mode == ViewerScrollMode::Page {
            self.viewer.page_scroll_page = current_page;
            self.viewer.page_mode_wheel_accum = 0.0;
            self.viewer.page_mode_wheel_last_event_at = None;
            self.viewer.page_mode_wheel_gesture_consumed = false;
        }
        self.viewer.horizontal_offset = 0.0;
        self.viewer.scroll_offset = 0.0;
        let zoom_task = self.apply_active_dimension_zoom();
        let page_task = self.jump_to_page(current_page);
        Task::batch([zoom_task, page_task])
    }

    /// Switches spread pairing, re-applying dimension zoom and jumping to the current page.
    pub(crate) fn set_viewer_spread_mode(&mut self, mode: ViewerSpreadMode) -> Task<Message> {
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

    /// Sets zoom page width, optionally anchoring the document under `cursor`.
    ///
    /// `Immediate` requests tiles now; `Debounced` schedules
    /// [`Message::ZoomRenderSettled`] after a short idle so wheel zoom stays smooth.
    pub(crate) fn zoom_to_width(
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
        self.viewer.visibility_menu_open = false;
        self.viewer.zoom_generation = self.viewer.zoom_generation.wrapping_add(1);
        let generation = self.viewer.zoom_generation;

        if let Some((x, y)) = anchor {
            self.viewer.horizontal_offset = x.clamp(0.0, self.max_horizontal_offset());
            self.viewer.scroll_offset = y.clamp(0.0, self.max_scroll_offset());
        }

        self.clamp_horizontal_offset();

        match render_policy {
            ZoomRenderPolicy::Immediate => Task::batch([
                self.request_visible_pages(),
                self.scroll_viewer_to_offsets_task(),
            ]),
            ZoomRenderPolicy::Debounced => Task::batch([
                schedule_zoom_render(generation),
                self.scroll_viewer_to_offsets_task(),
            ]),
        }
    }

    /// Best tile for drawing `key`, including exact matches and zoom previews.
    pub(crate) fn rendered_page_for_draw(&self, key: TileKey) -> Option<&RenderedPageView> {
        selected_render_key(
            self.viewer.rendered_pages.keys(),
            key,
            self.viewer.zoom_preview_width_px,
            true,
        )
        .and_then(|key| self.viewer.rendered_pages.get(&key))
    }

    /// Closest non-exact tile for `key` used while the preferred width is pending.
    pub(crate) fn fallback_rendered_page_for_draw(
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

    /// 0.0–1.0 fade-in progress for a newly arrived tile, if still animating.
    pub(crate) fn page_fade_progress(&self, key: TileKey) -> Option<f32> {
        let started = self.viewer.page_fade_started.get(&key)?;
        let elapsed = Instant::now().saturating_duration_since(*started);
        Some(
            (elapsed.as_secs_f32() / (self.layout().viewer_page_fade_ms as f32 / 1000.0))
                .clamp(0.0, 1.0),
        )
    }

    /// Whether every visible page already has a tile at the current render width.
    pub(crate) fn all_visible_pages_rendered_at_current_zoom(&self) -> bool {
        self.visible_page_range().all(|page| {
            self.viewer.rendered_pages.contains_key(&TileKey {
                page,
                width_px: self.render_width_px(),
            })
        })
    }

    /// Window title: resolved document title in viewer mode, otherwise `"PDF-Folio"`.
    ///
    /// Uses the same background-loaded [`ViewerRuntime::document_title`] as the
    /// toolbar (library entry / PDF metadata), not the raw filesystem name, so
    /// session-restored viewers do not keep a content-hash filename in the
    /// window chrome.
    pub(crate) fn title(&self) -> String {
        if matches!(self.mode, AppMode::Library | AppMode::LibrarySwitcher) {
            return String::from("PDF-Folio");
        }

        self.viewer
            .document_title
            .as_deref()
            .filter(|title| !title.is_empty())
            .map(|title| format!("{title} - PDF-Folio"))
            .unwrap_or_else(|| String::from("PDF-Folio"))
    }
}
