//! Application messages exchanged between UI views and update logic.

use std::path::PathBuf;
use std::sync::Arc;

use iced::keyboard;
use iced::Point;
use pdf_folio_core::{Annotation, AnnotationId, PdfDoc, TileKey};
use pdf_folio_library::{EntryId, ImportSummary, LibraryEntry, LibraryWatchEvent};

use crate::Settings;

/// Messages handled by the PDF-Folio application update loop.
#[derive(Debug, Clone)]
pub enum Message {
    /// Open the native file picker.
    OpenFileDialog,
    /// The native file picker was dismissed without choosing a file.
    FileDialogCanceled,
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
    ThumbnailReady {
        entry_id: EntryId,
        data: Vec<u8>,
        width: u16,
        height: u16,
    },
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
    /// Expand or collapse a table-of-contents node.
    ToggleOutlineNode(Vec<usize>),
    /// Open the jump-to-page overlay.
    OpenJumpDialog,
    /// Close the active overlay or panel.
    CloseOverlay,
    /// The jump-to-page input changed.
    JumpInputChanged(String),
    /// Submit the jump-to-page overlay.
    SubmitJump,
    /// Toggle the table-of-contents panel.
    ToggleTocPanel,
    /// Toggle the sidebar.
    ToggleSidebar,
    /// Toggle the placeholder view mode control.
    ToggleViewMode,
    /// Add an annotation.
    AnnotationAdded(Annotation),
    /// Delete an annotation.
    AnnotationDeleted(AnnotationId),
    /// Export annotations into a PDF.
    ExportAnnotations,
    /// Library entries loaded.
    LibraryLoaded(Vec<LibraryEntry>),
    /// Reload library entries from storage.
    LibraryRefresh,
    /// A library operation failed.
    LibraryError(String),
    /// Open the native folder picker for bulk import.
    ImportFolderDialog,
    /// The native folder picker selected an import directory.
    ImportFolderSelected(PathBuf),
    /// Bulk import finished.
    ImportFinished(ImportSummary),
    /// Open a library entry in the viewer.
    OpenLibraryEntry(EntryId),
    /// A library entry was clicked.
    LibraryEntryClicked(EntryId),
    /// A library entry document was opened successfully.
    LibraryDocumentOpened { entry_id: EntryId, doc: Arc<PdfDoc> },
    /// Return from the viewer to the library.
    BackToLibrary,
    /// Search query changed.
    SearchQueryChanged(String),
    /// Search debounce elapsed for a query.
    SearchDebounced(String),
    /// Search results loaded.
    SearchResults {
        entries: Vec<LibraryEntry>,
        hit_pages: std::collections::HashMap<EntryId, u16>,
    },
    /// Library scroll viewport changed.
    LibraryScrolled { offset_y: f32, viewport_height: f32 },
    /// A filesystem watcher event arrived.
    LibraryWatchEvent(LibraryWatchEvent),
    /// Tag filter changed.
    TagFilterChanged(Option<String>),
    /// Start inline tag entry for an item.
    StartTagEntry(EntryId),
    /// Inline tag text changed.
    TagInputChanged(String),
    /// Submit the active inline tag.
    SubmitTag,
    /// A library entry was tagged.
    EntryTagged { id: EntryId, tag: String },
    /// A library entry tag was removed.
    EntryUntagged { id: EntryId, tag: String },
    /// A library entry was deleted.
    EntryDeleted(EntryId),
    /// Reading progress changed.
    ProgressUpdated { entry_id: EntryId, page: u16 },
    /// Reading progress was saved.
    ProgressSaved,
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
    /// Toggle dark/light theme.
    ToggleTheme,
    /// Scroll down by one viewport.
    PageDown,
    /// Scroll up by one viewport.
    PageUp,
    /// Scroll by a small number of logical pixels.
    FineScroll(i16),
    /// Pan horizontally by a small number of logical pixels.
    HorizontalPan(i16),
    /// Open the jump-to-page overlay.
    Jump,
    /// Close overlays or panels.
    Escape,
}
