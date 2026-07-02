//! Viewer state helper types.

use iced::widget::image;
use pdf_folio_core::RenderedPage;

/// A rendered page prepared for display by iced.
#[derive(Debug, Clone)]
pub struct RenderedPageView {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

impl From<RenderedPage> for RenderedPageView {
    fn from(page: RenderedPage) -> Self {
        Self {
            width: page.width,
            height: page.height,
            handle: image::Handle::from_rgba(
                u32::from(page.width),
                u32::from(page.height),
                page.rgba,
            ),
        }
    }
}

/// A concrete character position in the viewer text layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewerTextAnchor {
    /// Zero-based page index.
    pub page: u16,
    /// Zero-based character index inside the page text layer.
    pub char_index: usize,
}

impl ViewerTextAnchor {
    /// Creates a new character anchor.
    pub fn new(page: u16, char_index: usize) -> Self {
        Self { page, char_index }
    }
}

/// Per-character text selection state for the raster viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerTextSelection {
    /// Character where the drag selection started.
    pub anchor: ViewerTextAnchor,
    /// Character currently under the selection drag.
    pub focus: ViewerTextAnchor,
    /// Whether the pointer is still dragging the selection.
    pub dragging: bool,
}

impl ViewerTextSelection {
    /// Starts a new selection anchored to one character.
    pub fn new(anchor: ViewerTextAnchor) -> Self {
        Self {
            anchor,
            focus: anchor,
            dragging: true,
        }
    }

    /// Returns the ordered selection endpoints.
    pub fn ordered(self) -> (ViewerTextAnchor, ViewerTextAnchor) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    /// Returns whether a page is inside the selection.
    pub fn contains_page(self, page: u16) -> bool {
        let (start, end) = self.ordered();
        (start.page..=end.page).contains(&page)
    }

    /// Returns the selected character range for a single page.
    pub fn char_range_for_page(
        self,
        page: u16,
        page_char_count: usize,
    ) -> Option<std::ops::RangeInclusive<usize>> {
        if page_char_count == 0 || !self.contains_page(page) {
            return None;
        }

        let (start, end) = self.ordered();
        let last = page_char_count - 1;
        let start_index = if page == start.page {
            start.char_index.min(last)
        } else {
            0
        };
        let end_index = if page == end.page {
            end.char_index.min(last)
        } else {
            last
        };

        (start_index <= end_index).then_some(start_index..=end_index)
    }
}
