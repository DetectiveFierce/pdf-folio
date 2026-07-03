//! Minimal `iced_widget` patch layer for pdf-folio.
//!
//! Everything is re-exported from upstream, except `scrollable`, where the
//! sidebar needs a left scrollbar without reversed vertical scroll behavior.
//!
//! This crate exists so the workspace can apply a targeted local override to
//! a single widget while keeping the rest of the iced widget tree identical
//! to the pinned upstream revision.
//!
//! The workspace `[patch.crates-io]` table points `iced_widget` at this crate,
//! so any `iced::widget::*` usage transparently resolves to the patched
//! implementation. Only [`scrollable()`] is replaced; all other widgets pass
//! through unchanged via `pub use iced_widget_upstream::*`.

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
