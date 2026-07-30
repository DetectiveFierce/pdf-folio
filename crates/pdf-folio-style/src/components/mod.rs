//! Styled widget constructors for common UI pieces.

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
