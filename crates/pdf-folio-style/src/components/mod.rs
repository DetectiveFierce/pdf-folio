//! Styled widget constructors for common UI chrome.
//!
//! Component helpers close over [`ThemeTokens`](crate::ThemeTokens) and a
//! [`Class`](crate::Class) so call sites stay free of raw color/padding math.
//! They accept labels, content elements, and message callbacks only — never
//! database handles or app state.
//!
//! | Submodule | Examples |
//! | --- | --- |
//! | [`core`] | `toolbar_button`, `tag_pill`, `search_input`, `error_banner` |
//! | [`library`] | `library_card`, `library_row`, selection checkboxes |
//! | [`viewer`] | `toc_entry`, annotation toolbar/popover surfaces |
//!
//! Prefer these builders when the same chrome appears in multiple views; reach
//! for class stylesheets directly when composing one-off layouts.

pub mod core;
pub mod library;
pub mod viewer;

pub use core::{
    align_content_x, align_content_y, aligned_text, empty_state, error_banner, icon_button,
    progress_bar, search_input, search_input_with_class, section_heading, sidebar_button, tag_pill,
    toolbar_button, weighted_text,
};
pub use library::{
    library_card, library_row, master_checkbox, selection_checkbox, MasterCheckboxState,
};
pub use viewer::{annotation_popover, annotation_toolbar, toc_entry};
