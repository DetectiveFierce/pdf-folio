//! Styled widget constructors for common UI chrome.
//!
//! Component helpers close over [`ThemeTokens`](crate::ThemeTokens) and a
//! [`Class`](crate::Class) so call sites stay free of raw color/padding math.
//! They accept labels, content elements, and message callbacks only — never
//! database handles or app state.
//!
//! | Submodule | Examples |
//! | --- | --- |
//! | [`core`] | `toolbar_button`, `tag_pill`, `search_input_with_class` |
//! | [`library`] | selection checkboxes and master checkbox |
//! | [`viewer`] | `toc_entry` |
//!
//! Prefer these builders when the same chrome appears in multiple views; reach
//! for class stylesheets directly when composing one-off layouts.

/// Shared chrome builders (toolbar buttons, tags, empty states, …).
pub mod core;
/// Library selection checkboxes (entry + master).
pub mod library;
/// Viewer-specific builders (TOC entry).
pub mod viewer;

pub use core::{
    aligned_text, empty_state, icon_button, progress_bar, search_input_with_class, section_heading,
    tag_pill, toolbar_button, weighted_text,
};
pub use library::{master_checkbox, selection_checkbox, MasterCheckboxState};
pub use viewer::toc_entry;
