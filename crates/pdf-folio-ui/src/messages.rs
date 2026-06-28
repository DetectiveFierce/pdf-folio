//! Application messages exchanged between UI views and update logic.

use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard;
use iced::Point;
use pdf_folio_core::{Annotation, AnnotationId, PdfDoc, TileKey};
use pdf_folio_library::{EntryId, LibraryEntry};

use crate::Settings;

/// Messages handled by the PDF-Folio application update loop.
#[derive(Debug, Clone)]
pub enum Message {
    /// Open the native file picker.
    OpenFileDialog,
    /// A file was selected.
    FileSelected(PathBuf),
    /// A document was opened successfully.
    DocumentOpened(Arc<PdfDoc>),
    /// A document operation failed.
    DocumentError(String),
    /// A page render finished.
    PageRendered {
        key: TileKey,
        data: Vec<u8>,
        width: u16,
        height: u16,
    },
    /// A thumbnail render finished.
    ThumbnailReady { page: u16, data: Vec<u8> },
    /// Scroll offset changed.
    ScrollChanged(f32),
    /// Scroll offset and viewport size changed.
    ViewportChanged {
        scroll_offset: f32,
        width: f32,
        height: f32,
    },
    /// Wheel input over the document viewport.
    ViewportWheelScrolled {
        delta_x: f32,
        delta_y: f32,
        cursor: Point,
        viewport_width: f32,
        viewport_height: f32,
    },
    /// Keyboard modifiers changed.
    ModifiersChanged(keyboard::Modifiers),
    /// Increase zoom.
    ZoomIn,
    /// Decrease zoom.
    ZoomOut,
    /// Set rendered page width in pixels.
    ZoomSet(u16),
    /// A wheel zoom gesture has been idle long enough to render the final zoom level.
    ZoomRenderSettled(u64),
    /// Jump to a zero-based page.
    JumpToPage(u16),
    /// Toggle the table-of-contents panel.
    ToggleTocPanel,
    /// Toggle the sidebar.
    ToggleSidebar,
    /// Add an annotation.
    AnnotationAdded(Annotation),
    /// Delete an annotation.
    AnnotationDeleted(AnnotationId),
    /// Export annotations into a PDF.
    ExportAnnotations,
    /// Library entries loaded.
    LibraryLoaded(Vec<LibraryEntry>),
    /// Search query changed.
    SearchQueryChanged(String),
    /// Search results loaded.
    SearchResults(Vec<LibraryEntry>),
    /// A library entry was tagged.
    EntryTagged { id: EntryId, tag: String },
    /// A library entry was deleted.
    EntryDeleted(EntryId),
    /// Toggle app theme.
    ThemeToggled,
    /// A keyboard shortcut was pressed.
    ShortcutPressed(Shortcut),
    /// Settings changed.
    SettingsChanged(Settings),
}

/// Keyboard shortcuts handled by the Phase 1 viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Increase zoom.
    In,
    /// Decrease zoom.
    Out,
    /// Reset zoom to the configured default.
    Reset,
}
