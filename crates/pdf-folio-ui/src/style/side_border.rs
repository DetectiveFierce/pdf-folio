//! Per-side border drawing for styles that exceed iced's uniform border model.

use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::Operation;
use iced::advanced::{layout, mouse, overlay, renderer, Clipboard, Layout, Shell, Widget};
use iced::{Background, Color, Element, Event, Length, Rectangle, Size, Vector};

use super::tokens::{BorderSide, VisualBorder};

/// Wraps an element with a custom per-side border layer.
pub fn side_border<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    border: VisualBorder,
) -> Element<'a, Message> {
    Element::new(SideBorder {
        content: content.into(),
        border,
    })
}

struct SideBorder<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    border: VisualBorder,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for SideBorder<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> tree::Tag {
        struct Tag;
        tree::Tag::of::<Tag>()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );

        if layout.bounds().intersection(viewport).is_some() {
            draw_side_border(renderer, layout.bounds(), self.border);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

fn draw_side_border<Renderer>(renderer: &mut Renderer, bounds: Rectangle, border: VisualBorder)
where
    Renderer: iced::advanced::Renderer,
{
    for (side, bounds) in side_border_rects(bounds, border) {
        draw_side(renderer, side, bounds);
    }
}

fn side_border_rects(bounds: Rectangle, border: VisualBorder) -> [(BorderSide, Rectangle); 4] {
    let top_width = border.top.width.unwrap_or(0.0).max(0.0);
    let right_width = border.right.width.unwrap_or(0.0).max(0.0);
    let bottom_width = border.bottom.width.unwrap_or(0.0).max(0.0);
    let left_width = border.left.width.unwrap_or(0.0).max(0.0);

    [
        (
            border.top,
            Rectangle {
                height: top_width,
                ..bounds
            },
        ),
        (
            border.right,
            Rectangle {
                x: bounds.x + bounds.width - right_width,
                width: right_width,
                ..bounds
            },
        ),
        (
            border.bottom,
            Rectangle {
                y: bounds.y + bounds.height - bottom_width,
                height: bottom_width,
                ..bounds
            },
        ),
        (
            border.left,
            Rectangle {
                width: left_width,
                ..bounds
            },
        ),
    ]
}

fn draw_side<Renderer>(renderer: &mut Renderer, side: BorderSide, bounds: Rectangle)
where
    Renderer: iced::advanced::Renderer,
{
    let width = side.width.unwrap_or(0.0);
    let color = side.color.unwrap_or(Color::TRANSPARENT);
    if width <= 0.0 || color.a <= 0.0 {
        return;
    }

    renderer.fill_quad(
        renderer::Quad {
            bounds,
            border: iced::Border::default(),
            shadow: iced::Shadow::default(),
            snap: true,
        },
        Background::Color(color),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_border_rects_use_exact_side_widths() {
        let bounds = Rectangle {
            x: 10.0,
            y: 20.0,
            width: 200.0,
            height: 40.0,
        };
        let border = VisualBorder {
            top: BorderSide::new(2.0, Color::BLACK),
            right: BorderSide::new(4.0, Color::BLACK),
            bottom: BorderSide::new(6.0, Color::BLACK),
            left: BorderSide::new(12.0, Color::BLACK),
        };

        let [top, right, bottom, left] = side_border_rects(bounds, border);

        assert_eq!(top.1.height, 2.0);
        assert_eq!(right.1.width, 4.0);
        assert_eq!(right.1.x, 206.0);
        assert_eq!(bottom.1.height, 6.0);
        assert_eq!(bottom.1.y, 54.0);
        assert_eq!(left.1.width, 12.0);
        assert_eq!(left.1.x, 10.0);
    }

    #[test]
    fn side_border_rects_clamp_negative_widths_to_zero() {
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 24.0,
        };
        let border = VisualBorder {
            left: BorderSide::new(-4.0, Color::BLACK),
            ..VisualBorder::EMPTY
        };

        let [_, _, _, left] = side_border_rects(bounds, border);

        assert_eq!(left.1.width, 0.0);
    }
}
