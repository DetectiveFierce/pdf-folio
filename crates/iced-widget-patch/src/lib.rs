//! Minimal `iced_widget` patch layer for PDF-Folio.
//!
//! # Why this crate exists
//!
//! PDF-Folio sidebars need a **left-edge vertical scrollbar** without reversing
//! vertical scroll semantics (wheel up still moves content up). Upstream
//! `iced_widget::scrollable` ties vertical `Anchor::End` to both scroller
//! placement *and* inverted scroll direction, which is wrong for a left rail.
//!
//! Rather than maintain a full iced fork, this package is named `iced_widget`
//! and re-exports upstream wholesale, replacing only the scrollable surface.
//!
//! # What differs from upstream scrollable
//!
//! The local [`scrollable`] module is based on iced's scrollable at the pinned
//! git revision, with layout changes so that:
//!
//! - Vertical `Scrollbar` alignment `Anchor::End` places the rail on the
//!   **left** of the content.
//! - Vertical scroll deltas are **not** inverted when that alignment is used
//!   (`Direction::align` only mirrors the horizontal axis).
//!
//! Horizontal anchoring behavior matches upstream. All other widgets are
//! identical to `iced_widget_upstream`.
//!
//! # How workspace `[patch]` wires it
//!
//! Root `Cargo.toml`:
//!
//! ```toml
//! [patch.crates-io]
//! iced_widget = { path = "crates/iced-widget-patch" }
//! # iced_core, iced_runtime, … pin the same git rev as iced_widget_upstream
//! ```
//!
//! This crate depends on git `iced_widget` as package name
//! `iced_widget_upstream`, then:
//!
//! 1. `pub use iced_widget_upstream::*;`
//! 2. Shadow `scrollable` with the local module and factory [`scrollable()`].
//!
//! Any `iced::widget::scrollable` / `use iced::widget::*` in the app therefore
//! resolves to the patched type. Supporting iced crates (`iced_core`,
//! `iced_renderer`, …) must stay on the **same git revision** as
//! `iced_widget_upstream` so there is a single type universe.
//!
//! # Maintainer rules
//!
//! - Keep the override to scrollable only; do not fork additional widgets.
//! - When upgrading iced, bump every `rev = "…"` together, rebase
//!   `scrollable.rs`, and smoke-test library + viewer sidebars.

pub use iced_widget_upstream::*;

pub mod scrollable;

#[doc(no_inline)]
pub use scrollable::Scrollable;

/// Creates a new patched [`Scrollable`] with the provided content.
///
/// Same constructor shape as upstream; the returned type is this crate's
/// scrollable, which supports left vertical rails without inverted scrolling.
pub fn scrollable<'a, Message, Theme, Renderer>(
    content: impl Into<core::Element<'a, Message, Theme, Renderer>>,
) -> Scrollable<'a, Message, Theme, Renderer>
where
    Theme: scrollable::Catalog + 'a,
    Renderer: core::text::Renderer,
{
    Scrollable::new(content)
}
