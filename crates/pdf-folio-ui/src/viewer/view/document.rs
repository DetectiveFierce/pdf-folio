use crate::components::viewer::canvas::{ViewerCanvas, ViewerSelectionOverlay};
use crate::components::viewer::find_bar::viewer_find_anchor;
use crate::components::viewer::toolbar::viewer_floating_sidebar_toggle;
use crate::*;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{canvas, stack};

pub(crate) fn view_viewer_document(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let content_size = app.viewer_content_size(app.viewer.viewer_viewport_width);
    let viewer = canvas(ViewerCanvas { app })
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height));
    let selection_overlay = canvas(ViewerSelectionOverlay { app })
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height));
    let viewer_content = stack![viewer, selection_overlay]
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height));
    let viewer_scroll = scrollable(viewer_content)
        .id(Id::new(VIEWER_SCROLLABLE_ID))
        .direction(Direction::Both {
            vertical: Scrollbar::default(),
            horizontal: Scrollbar::default(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::ViewerCanvas, status))
        .on_scroll(|viewport| {
            let offset = viewport.absolute_offset();
            let bounds = viewport.bounds();
            Message::ViewportChanged {
                horizontal_offset: offset.x,
                scroll_offset: offset.y,
                width: bounds.width,
                height: bounds.height,
            }
        });
    let mut viewer_stack = stack![viewer_scroll]
        .width(Length::Fill)
        .height(Length::Fill);
    if !app.viewer.toc_open {
        viewer_stack = viewer_stack.push(
            pin(viewer_floating_sidebar_toggle(tokens))
                .x(Spacing::SM)
                .y(Spacing::SM),
        );
    }
    if app.viewer.viewer_find.open {
        let find_width = app
            .layout()
            .viewer_find_bar_width
            .min((app.viewer.viewer_viewport_width - Spacing::MD * 2.0).max(320.0));
        viewer_stack = viewer_stack.push(viewer_find_anchor(app, tokens, find_width));
    }

    viewer_stack.into()
}
