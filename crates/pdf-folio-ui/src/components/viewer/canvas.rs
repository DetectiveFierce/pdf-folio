//! # Viewer canvas
//!
//! Custom iced `canvas::Program` widgets under `components::viewer::canvas`
//! that paint continuous PDF pages, text-selection highlights, and animated
//! spinners. Handles wheel zoom/scroll, text selection drag, empty-click
//! clear, and right-click context menu open on the page surface.
//!
//! ## Ownership
//!
//! Presentation and hit-testing only: reads page layout, rendered tiles, and
//! selection state from `app.viewer`, emits viewer `Message`s. Page raster
//! cache and document loading live in the viewer domain/shell. Spinner is
//! reused by `components::shared::{loading, sync_status}`.
//!
//! Related: [`super::toolbar`] / [`super::find_bar`] for chrome around the
//! canvas; `crate::viewer::view` hosts the continuous layout.

use crate::*;
use iced::widget::canvas;
use iced::{mouse, Color, Point, Radians, Rectangle, Renderer, Size, Theme};
use pdf_folio_core::{PageTextChar, PageTextLayer, TileKey};
use std::time::Instant;

/// Pointer movement (logical px) after an empty press before the click is
/// treated as a drag rather than a selection-clear click.
const EMPTY_CANVAS_CLICK_DRAG_THRESHOLD: f32 = 4.0;

/// Continuous PDF page surface painted via iced canvas.
///
/// Implements pointer wheel (Ctrl-zoom / horizontal scroll modes), text
/// selection start/update, and context-menu open. Draw path composites
/// rendered page tiles from the viewer cache.
#[derive(Debug)]
pub(crate) struct ViewerCanvas<'a> {
    /// Shared app state providing document layout, tiles, and modifiers.
    pub(crate) app: &'a PDFolioApp,
}

/// Per-widget interaction state for [`ViewerCanvas`].
///
/// Tracks a provisional empty-canvas press so a short click can clear text
/// selection while a drag past the threshold is ignored as a clear.
#[derive(Debug, Default)]
pub(crate) struct ViewerCanvasState {
    /// Cursor position of a left press that did not hit a character; cleared
    /// once movement exceeds [`EMPTY_CANVAS_CLICK_DRAG_THRESHOLD`] or on release.
    pending_empty_click: Option<Point>,
}

/// Transparent overlay that draws active text-selection highlights over pages.
///
/// Layered above [`ViewerCanvas`] so selection geometry stays independent of
/// tile redraws.
#[derive(Debug)]
pub(crate) struct ViewerSelectionOverlay<'a> {
    /// App state with current `viewer_text_selection` and page text layers.
    pub(crate) app: &'a PDFolioApp,
}

/// Animated circular spinner used for history restore, document open, and sync.
///
/// `started_at` / `now` drive rotation; `color` is typically theme primary or
/// a muted accent depending on the host surface.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HistoryRestoreSpinner {
    /// Instant when the long-running operation began (defines rotation phase).
    pub(crate) started_at: Instant,
    /// Current animation clock, usually `app.library.animation_now`.
    pub(crate) now: Instant,
    /// Stroke color for the spinner arc.
    pub(crate) color: Color,
}

impl canvas::Program<Message> for ViewerCanvas<'_> {
    type State = ViewerCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !self.app.viewer.modifiers.control()
                    && self.app.viewer.viewer_scroll_mode != ViewerScrollMode::Horizontal
                {
                    return None;
                }

                let (delta_x, delta_y) =
                    scroll_delta_pixels(*delta, self.app.layout().line_scroll_pixels);

                let cursor = cursor
                    .position_in(bounds)
                    .unwrap_or_else(|| Point::new(bounds.width / 2.0, bounds.height / 2.0));

                Some(
                    canvas::Action::publish(Message::ViewportWheelScrolled {
                        delta_x,
                        delta_y,
                        cursor,
                        viewport_width: bounds.width,
                        viewport_height: bounds.height,
                    })
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                state.pending_empty_click = None;
                if let Some(anchor) = char_at_position(self.app, bounds, position) {
                    Some(
                        canvas::Action::publish(Message::ViewerTextSelectionStarted {
                            page: anchor.page,
                            char_index: anchor.char_index,
                        })
                        .and_capture(),
                    )
                } else if self.app.viewer.viewer_text_selection.is_some() {
                    state.pending_empty_click = Some(position);
                    Some(canvas::Action::capture())
                } else {
                    None
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let position = cursor.position_over(bounds)?;
                Some(
                    canvas::Action::publish(Message::ContextMenuOpenedAt {
                        target: ContextMenuTarget::ViewerCanvas,
                        position,
                    })
                    .and_capture(),
                )
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(start), Some(position)) =
                    (state.pending_empty_click, cursor.position_in(bounds))
                {
                    if point_distance(start, position) > EMPTY_CANVAS_CLICK_DRAG_THRESHOLD {
                        state.pending_empty_click = None;
                    }
                }

                if !self
                    .app
                    .viewer
                    .viewer_text_selection
                    .is_some_and(|selection| selection.dragging)
                {
                    return None;
                }

                cursor
                    .position_in(bounds)
                    .and_then(|position| char_at_position(self.app, bounds, position))
                    .map(|anchor| {
                        canvas::Action::publish(Message::ViewerTextSelectionChanged {
                            page: anchor.page,
                            char_index: anchor.char_index,
                        })
                        .and_capture()
                    })
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if self
                    .app
                    .viewer
                    .viewer_text_selection
                    .is_some_and(|selection| selection.dragging)
                {
                    state.pending_empty_click = None;
                    Some(canvas::Action::publish(Message::ViewerTextSelectionEnded).and_capture())
                } else if state.pending_empty_click.take().is_some() {
                    Some(canvas::Action::publish(Message::ViewerCanvasClicked).and_capture())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = canvas::Path::rectangle(Point::ORIGIN, bounds.size());
        let tokens = self
            .app
            .appearance
            .theme
            .tokens(&self.app.appearance.style_book);
        let viewer_style = viewer_primitives(tokens);
        frame.fill(&background, viewer_style.canvas);

        if self.app.viewer.doc.is_none() {
            return vec![frame.into_geometry()];
        };
        for (page, rect) in self.app.viewer_page_rects_visible_content() {
            let key = TileKey {
                page,
                width_px: self.app.render_width_px(),
            };

            if let Some(rendered) = self.app.rendered_page_for_draw(key) {
                if let Some(progress) = self.app.page_fade_progress(key) {
                    if progress < 1.0 {
                        if let Some(fallback) = self.app.fallback_rendered_page_for_draw(key) {
                            frame.draw_image(
                                rect,
                                canvas::Image::new(fallback.handle.clone()).snap(true),
                            );
                        }
                        frame.draw_image(
                            rect,
                            canvas::Image::new(rendered.handle.clone())
                                .opacity(progress)
                                .snap(true),
                        );
                    } else {
                        frame.draw_image(
                            rect,
                            canvas::Image::new(rendered.handle.clone()).snap(true),
                        );
                    }
                } else {
                    frame.draw_image(rect, canvas::Image::new(rendered.handle.clone()).snap(true));
                }
            } else {
                let shadow = canvas::Path::rectangle(
                    Point::new(
                        rect.x + viewer_style.page_shadow.offset_x,
                        rect.y + viewer_style.page_shadow.offset_y,
                    ),
                    Size::new(rect.width, rect.height),
                );
                frame.fill(&shadow, viewer_style.page_shadow.color);
                let placeholder = canvas::Path::rectangle(rect.position(), rect.size());
                frame.fill(&placeholder, viewer_style.placeholder);
            }
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor
            .position_in(bounds)
            .and_then(|position| char_at_position(self.app, bounds, position))
            .is_some()
        {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl canvas::Program<Message> for ViewerSelectionOverlay<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        _event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        if self.app.viewer.doc.is_none() {
            return vec![frame.into_geometry()];
        };

        for (page, rect) in self.app.viewer_page_rects_visible_content() {
            draw_find_highlights(self.app, &mut frame, page, rect);
            draw_text_selection(self.app, &mut frame, page, rect);
        }

        vec![frame.into_geometry()]
    }
}

/// Hit-test: map a canvas-local point to a text character anchor on a visible page.
fn char_at_position(
    app: &PDFolioApp,
    _bounds: Rectangle,
    position: Point,
) -> Option<crate::viewer::state::ViewerTextAnchor> {
    app.viewer.doc.as_ref()?;
    for (page, rect) in app.viewer_page_rects_visible_content() {
        if position.x >= rect.x
            && position.x <= rect.x + rect.width
            && position.y >= rect.y
            && position.y <= rect.y + rect.height
        {
            return app
                .viewer
                .viewer_text_layers
                .get(&page)
                .and_then(|layer| char_in_page_at_position(layer, rect, position))
                .map(|char_index| crate::viewer::state::ViewerTextAnchor::new(page, char_index));
        }
    }

    None
}

/// Hit-test within one page’s text layer: exact glyph hit, else nearest on the same line band.
fn char_in_page_at_position(
    layer: &PageTextLayer,
    page_rect: Rectangle,
    position: Point,
) -> Option<usize> {
    let mut nearest = None;
    let mut nearest_distance = f32::MAX;

    for (char_index, character) in layer.chars.iter().enumerate() {
        let rect = character_screen_rect(character, page_rect);
        if rect.width <= 0.0 || rect.height <= 0.0 {
            continue;
        }

        if point_in_rect(position, rect) {
            return Some(char_index);
        }

        let center_y = rect.y + rect.height / 2.0;
        let vertical_slop = (rect.height * 0.85).max(5.0);
        if (position.y - center_y).abs() <= vertical_slop {
            let center_x = rect.x + rect.width / 2.0;
            let distance = (position.x - center_x).abs();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = Some(char_index);
            }
        }
    }

    if nearest_distance <= 28.0 {
        nearest
    } else {
        None
    }
}

impl canvas::Program<Message> for HistoryRestoreSpinner {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let elapsed = self
            .now
            .saturating_duration_since(self.started_at)
            .as_secs_f32();
        let rotation = elapsed * std::f32::consts::TAU * 0.85;
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = 18.0;
        let arc = canvas::Path::new(|path| {
            path.arc(canvas::path::Arc {
                center,
                radius,
                start_angle: Radians(rotation),
                end_angle: Radians(rotation + std::f32::consts::TAU * 0.76),
            });
        });
        frame.stroke(
            &arc,
            canvas::Stroke::default()
                .with_width(4.0)
                .with_color(self.color)
                .with_line_cap(canvas::LineCap::Round),
        );
        vec![frame.into_geometry()]
    }
}

/// Paint find-match rectangles on `page` (selected match uses accent fill; others optional).
fn draw_find_highlights(
    app: &PDFolioApp,
    frame: &mut canvas::Frame,
    page: u16,
    page_rect: Rectangle,
) {
    if app.viewer.viewer_find.query.is_empty() {
        return;
    }

    let Some(layer) = app.viewer.viewer_text_layers.get(&page) else {
        return;
    };

    let selected = app.viewer.viewer_find.selected;
    for (index, matched) in app.viewer.viewer_find.matches.iter().enumerate() {
        if matched.page != page {
            continue;
        }

        let is_selected = Some(index) == selected;
        if !app.viewer.viewer_find.highlight_all && !is_selected {
            continue;
        }

        let Some(range) = matched.char_range() else {
            continue;
        };
        let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
        let viewer_style = viewer_primitives(tokens);
        let color = if is_selected {
            viewer_style.find_selected_fill
        } else {
            viewer_style.find_fill
        };
        for rect in selected_line_highlights(layer, page_rect, range) {
            let path = canvas::Path::rectangle(rect.position(), rect.size());
            frame.fill(&path, color);
        }
    }
}

/// Paint the active text-selection range as filled line rectangles on `page`.
fn draw_text_selection(
    app: &PDFolioApp,
    frame: &mut canvas::Frame,
    page: u16,
    page_rect: Rectangle,
) {
    let (Some(selection), Some(layer)) = (
        app.viewer.viewer_text_selection,
        app.viewer.viewer_text_layers.get(&page),
    ) else {
        return;
    };
    let Some(range) = selection.char_range_for_page(page, layer.chars.len()) else {
        return;
    };

    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let color = viewer_primitives(tokens).text_selection_fill;
    for rect in selected_line_highlights(layer, page_rect, range) {
        let path = canvas::Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, color);
    }
}

/// Theme fill color used for text selection (test helper).
#[cfg(test)]
fn viewer_selection_fill(tokens: ThemeTokens) -> iced::Color {
    viewer_primitives(tokens).text_selection_fill
}

/// Merge selected character screen rects into per-line highlight rectangles.
fn selected_line_highlights(
    layer: &PageTextLayer,
    page_rect: Rectangle,
    range: std::ops::RangeInclusive<usize>,
) -> Vec<Rectangle> {
    let mut rects: Vec<Rectangle> = range
        .filter_map(|index| layer.chars.get(index))
        .map(|character| character_screen_rect(character, page_rect))
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .collect();

    rects.sort_by(|a, b| {
        let line_order = rect_center_y(*a).total_cmp(&rect_center_y(*b));
        if line_order == std::cmp::Ordering::Equal {
            a.x.total_cmp(&b.x)
        } else {
            line_order
        }
    });

    let mut lines: Vec<Rectangle> = Vec::new();
    for rect in rects {
        let padded = pad_rect(rect, 1.5, 1.0);
        if let Some(line) = lines.last_mut() {
            let line_center = rect_center_y(*line);
            let rect_center = rect_center_y(padded);
            let same_line_threshold = (line.height.max(padded.height) * 0.62).max(3.0);
            if (line_center - rect_center).abs() <= same_line_threshold {
                *line = union_rect(*line, padded);
                continue;
            }
        }
        lines.push(padded);
    }

    lines
        .into_iter()
        .map(|rect| clamp_rect_to_page(rect, page_rect))
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .collect()
}

/// Map a normalized character bounds rect into screen coordinates within `page_rect`.
fn character_screen_rect(character: &PageTextChar, page_rect: Rectangle) -> Rectangle {
    Rectangle::new(
        Point::new(
            page_rect.x + character.bounds.x * page_rect.width,
            page_rect.y + character.bounds.y * page_rect.height,
        ),
        Size::new(
            character.bounds.width * page_rect.width,
            character.bounds.height * page_rect.height,
        ),
    )
}

/// Vertical center of a rectangle (line-grouping key for selection highlights).
fn rect_center_y(rect: Rectangle) -> f32 {
    rect.y + rect.height / 2.0
}

/// Euclidean distance between two points (empty-click drag threshold checks).
fn point_distance(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// Expand a rect by horizontal/vertical padding (used when building line highlights).
fn pad_rect(rect: Rectangle, horizontal: f32, vertical: f32) -> Rectangle {
    Rectangle::new(
        Point::new(rect.x - horizontal, rect.y - vertical),
        Size::new(rect.width + horizontal * 2.0, rect.height + vertical * 2.0),
    )
}

/// Axis-aligned bounding box of two rectangles (merge adjacent glyph highlights).
fn union_rect(a: Rectangle, b: Rectangle) -> Rectangle {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

/// Clip a highlight rect to the page bounds so paint does not spill past the page.
fn clamp_rect_to_page(rect: Rectangle, page_rect: Rectangle) -> Rectangle {
    let left = rect.x.max(page_rect.x);
    let top = rect.y.max(page_rect.y);
    let right = (rect.x + rect.width).min(page_rect.x + page_rect.width);
    let bottom = (rect.y + rect.height).min(page_rect.y + page_rect.height);
    Rectangle::new(
        Point::new(left, top),
        Size::new((right - left).max(0.0), (bottom - top).max(0.0)),
    )
}

/// Inclusive point-in-rectangle test for character hit-testing.
fn point_in_rect(point: Point, rect: Rectangle) -> bool {
    point.x >= rect.x
        && point.x <= rect.x + rect.width
        && point.y >= rect.y
        && point.y <= rect.y + rect.height
}

/// Convert a wheel event into a pixel scroll delta for the viewer.
pub(crate) fn scroll_delta_pixels(
    delta: mouse::ScrollDelta,
    line_scroll_pixels: f32,
) -> (f32, f32) {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => (x * line_scroll_pixels, y * line_scroll_pixels),
        mouse::ScrollDelta::Pixels { x, y } => (x, y),
    }
}

/// Unit tests for selection highlight geometry helpers.
#[cfg(test)]
mod tests {
    use super::*;
    use pdf_folio_core::{PageTextChar, TextRect};

    #[test]
    fn selected_line_highlights_merge_adjacent_characters() {
        let layer = PageTextLayer {
            page: 0,
            width_points: 100.0,
            height_points: 100.0,
            chars: vec![
                text_char(0, "H", 0.10, 0.10),
                text_char(1, "i", 0.16, 0.10),
                text_char(2, "T", 0.10, 0.24),
            ],
        };
        let page_rect = Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0));

        let highlights = selected_line_highlights(&layer, page_rect, 0..=2);

        assert_eq!(highlights.len(), 2);
        assert!(highlights[0].width > 20.0);
        assert!(highlights[0].height < 35.0);
        assert!(highlights[1].y > highlights[0].y);
    }

    #[test]
    fn viewer_selection_fill_is_visible_on_light_pages() {
        let fill = viewer_selection_fill(crate::style::fallback_light_tokens());

        assert!(fill.r > fill.b);
        assert!(fill.g > fill.b);
        assert!(fill.a >= 0.4);
    }

    /// Build a fixed-size test glyph at normalized page coordinates `(x, y)`.
    fn text_char(index: usize, text: &str, x: f32, y: f32) -> PageTextChar {
        PageTextChar {
            index,
            text: text.to_owned(),
            bounds: TextRect {
                x,
                y,
                width: 0.05,
                height: 0.08,
            },
        }
    }
}
