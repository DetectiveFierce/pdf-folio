//! Viewer canvas rendering and wheel interaction.

use crate::*;
use iced::widget::canvas;
use iced::{mouse, Point, Rectangle, Renderer, Size, Theme};
use pdf_folio_core::TileKey;

#[derive(Debug)]
pub(crate) struct ViewerCanvas<'a> {
    pub(crate) app: &'a PDFolioApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZoomRenderPolicy {
    Immediate,
    Debounced,
}

impl canvas::Program<Message> for ViewerCanvas<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
            return None;
        };

        let (delta_x, delta_y) = scroll_delta_pixels(*delta, self.app.layout().line_scroll_pixels);

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
        let tokens = self.app.theme.tokens(&self.app.style_book);
        let viewer_style = viewer_primitives(tokens);
        frame.fill(&background, viewer_style.canvas);

        let Some(doc) = &self.app.doc else {
            return vec![frame.into_geometry()];
        };

        let page_width = f32::from(self.app.zoom_width);
        let x = ((bounds.width - page_width) / 2.0).max(Spacing::PAGE_GUTTER)
            - self.app.horizontal_offset;
        let mut y = Spacing::PAGE_GUTTER - self.app.scroll_offset;

        for page in 0..doc.page_count() {
            let height = self.app.page_height(page);
            let key = TileKey {
                page,
                width_px: self.app.render_width_px(),
            };
            let rect = Rectangle::new(Point::new(x, y), Size::new(page_width, height));

            if let Some(rendered) = self.app.rendered_page_for_draw(key) {
                frame.draw_image(rect, canvas::Image::new(rendered.handle.clone()).snap(true));
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

            y += height + Spacing::PAGE_GAP;
        }

        vec![frame.into_geometry()]
    }
}
pub(crate) fn scroll_delta_pixels(
    delta: mouse::ScrollDelta,
    line_scroll_pixels: f32,
) -> (f32, f32) {
    match delta {
        mouse::ScrollDelta::Lines { x, y } => (x * line_scroll_pixels, y * line_scroll_pixels),
        mouse::ScrollDelta::Pixels { x, y } => (x, y),
    }
}
