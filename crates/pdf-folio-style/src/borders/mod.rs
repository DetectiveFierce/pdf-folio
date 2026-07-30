//! Per-side border drawing for chrome that exceeds iced's uniform border model.
//!
//! iced borders are a single width/color/radius for the whole widget. PDF-Folio
//! styles often need asymmetric edges (for example a stronger left accent on a
//! selected sidebar row). KDL can set per-side borders on a class state; the
//! class stylesheet then returns a [`crate::tokens::VisualBorder`], and
//! [`side_border`] wraps content so those sides paint correctly.
//!
//! Typical flow:
//!
//! 1. KDL component state declares nested `border { left width=2 color=$accent }`.
//! 2. [`crate::classes::side_border_for_class`] extracts the [`VisualBorder`](crate::tokens::VisualBorder).
//! 3. Component helpers call [`side_border`] around the iced container/button.

/// Widget implementation of [`side_border`].
pub mod side;

pub use side::side_border;
