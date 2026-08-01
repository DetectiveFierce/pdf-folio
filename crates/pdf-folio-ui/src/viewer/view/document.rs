//! Document canvas region composition inside the viewer.
//!
//! Stacks the raster page canvas, text-selection overlay, and the
//! document-anchored annotation layer over a bidirectional scrollable.
//! Scroll events emit [`Message::ViewportChanged`] so navigation and tile
//! prefetch stay in sync with iced's scrollable. Viewport chrome (find bar,
//! floating TOC toggle, compose/empty annotation hints) is pinned outside
//! the scrollable.
//!
//! Related: [`crate::components::viewer::canvas`] for draw/hit-test,
//! [`crate::components::viewer::annotations`] for anchored cards,
//! [`super::root`] for placement in the full viewer layout.

use crate::components::viewer::annotations::{
    view_annotations_content_layer, view_annotations_viewport_chrome,
};
use crate::components::viewer::canvas::{ViewerCanvas, ViewerSelectionOverlay};
use crate::components::viewer::find_bar::viewer_find_anchor;
use crate::components::viewer::toolbar::viewer_floating_sidebar_toggle;
use crate::*;
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{canvas, stack, Space};

/// Builds the scrollable document canvas stack (pages, selection, overlays).
///
/// Layers raster canvas + selection + anchored annotation cards inside a
/// bidirectional scrollable that reports `ViewportChanged`, then pins find,
/// floating TOC toggle, and compose/empty hints on the viewport.
///
/// Overlay slots (annotations layer, floating TOC, find, chrome) always occupy
/// a fixed stack position so opening find / toggling comments / menus does not
/// remount the scrollable and jump scroll to the top.
pub(crate) fn view_viewer_document(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let content_size = app.viewer_content_size(app.viewer.viewer_viewport_width);
    let viewer = canvas(ViewerCanvas { app })
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height));
    let selection_overlay = canvas(ViewerSelectionOverlay { app })
        .width(Length::Fixed(content_size.width))
        .height(Length::Fixed(content_size.height));

    // Always three content layers so show/hide annotations does not remount.
    let annotations_layer: Element<'_, Message> = if app.annotation_layer_active() {
        view_annotations_content_layer(app, tokens, content_size)
    } else {
        Space::new()
            .width(content_size.width)
            .height(content_size.height)
            .into()
    };

    let viewer_content = stack![viewer, selection_overlay, annotations_layer]
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

    // Fixed viewport-chrome slots (scrollable first so it keeps widget identity).
    let floating_toc: Element<'_, Message> = if !app.viewer.toc_open {
        pin(viewer_floating_sidebar_toggle(tokens))
            .x(Spacing::SM)
            .y(Spacing::SM)
            .into()
    } else {
        Space::new().width(0).height(0).into()
    };

    let find_chrome: Element<'_, Message> = if app.viewer.viewer_find.open {
        let find_width = app
            .layout()
            .viewer_find_bar_width
            .min((app.viewer.viewer_viewport_width - Spacing::MD * 2.0).max(280.0));
        viewer_find_anchor(app, tokens, find_width)
    } else {
        Space::new().width(0).height(0).into()
    };

    // Compose/empty hints still show when comments are hidden so users can
    // create notes; anchored cards are gated above via `annotations_visible`.
    let annotations_chrome: Element<'_, Message> =
        if let Some(chrome) = view_annotations_viewport_chrome(app, tokens) {
            chrome
        } else {
            Space::new().width(0).height(0).into()
        };

    stack![viewer_scroll, floating_toc, find_chrome, annotations_chrome]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
