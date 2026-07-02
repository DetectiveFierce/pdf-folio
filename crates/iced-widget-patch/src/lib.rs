//! Minimal `iced_widget` patch layer for pdf-folio.
//!
//! Everything is re-exported from upstream, except `scrollable`, where the
//! sidebar needs a left scrollbar without reversed vertical scroll behavior.

pub use iced_widget_upstream::*;

pub mod scrollable;

#[doc(no_inline)]
pub use scrollable::Scrollable;

/// Creates a new [`Scrollable`] with the provided content.
pub fn scrollable<'a, Message, Theme, Renderer>(
    content: impl Into<core::Element<'a, Message, Theme, Renderer>>,
) -> Scrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog + 'a,
    Renderer: core::text::Renderer,
{
    Scrollable::new(content)
}
