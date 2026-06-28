//! Application messages exchanged between UI views and update logic.

use std::path::PathBuf;
use std::sync::Arc;

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
    PageRendered { key: TileKey, data: Vec<u8> },
    /// A thumbnail render finished.
    ThumbnailReady { page: u16, data: Vec<u8> },
    /// Scroll offset changed.
    ScrollChanged(f32),
    /// Increase zoom.
    ZoomIn,
    /// Decrease zoom.
    ZoomOut,
    /// Set rendered page width in pixels.
    ZoomSet(u16),
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
    /// Settings changed.
    SettingsChanged(Settings),
}
