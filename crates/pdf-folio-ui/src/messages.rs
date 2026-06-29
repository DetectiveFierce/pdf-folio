//! Application messages exchanged between UI views and update logic.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use iced::keyboard;
use iced::Point;
use pdf_folio_core::{Annotation, AnnotationId, PdfDoc, TileKey};
use pdf_folio_library::{
    EntryId, Folder, FolderId, ImportSummary, LibraryEntry, LibrarySortMode, LibraryWatchEvent,
};

use crate::app::ThumbnailSize;
use crate::style::StyleBook;
use crate::Settings;

/// Top-level application menu groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenu {
    /// File and import commands.
    File,
    /// Selection and metadata editing commands.
    Edit,
    /// Layout, theme, navigation, and zoom commands.
    View,
    /// Open-PDF reading commands.
    Document,
    /// Library organization and maintenance commands.
    Library,
    /// Long-running library maintenance commands.
    Tools,
    /// Product help and status commands.
    Help,
}

impl AppMenu {
    /// User-facing menu title.
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Document => "Document",
            Self::Library => "Library",
            Self::Tools => "Tools",
            Self::Help => "Help",
        }
    }
}

/// Concrete actions launched from the application menu bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMenuAction {
    /// Open a PDF from disk.
    OpenFile,
    /// Import PDFs from a folder.
    ImportFolder,
    /// Return from the viewer to the library.
    BackToLibrary,
    /// Reload the library from storage.
    RefreshLibrary,
    /// Select all visible PDFs.
    SelectAllVisible,
    /// Clear selected PDFs.
    ClearSelection,
    /// Save the current single-PDF metadata edit.
    SaveDetails,
    /// Reset the current single-PDF metadata edit.
    ResetDetails,
    /// Add the typed bulk tag.
    AddTag,
    /// Remove the typed bulk tag.
    RemoveTag,
    /// Add selection to the active folder.
    AddToFolder,
    /// Remove selection from the active folder.
    RemoveFromFolder,
    /// Delete selected PDFs from library metadata.
    DeleteFromLibrary,
    /// Toggle grid/list library layout.
    ToggleLayout,
    /// Toggle the light/dark theme.
    ToggleTheme,
    /// Reload KDL style files.
    ReloadStyles,
    /// Toggle the viewer table-of-contents panel.
    ToggleToc,
    /// Open the jump-to-page dialog.
    JumpToPage,
    /// Increase viewer zoom.
    ZoomIn,
    /// Decrease viewer zoom.
    ZoomOut,
    /// Reset viewer zoom.
    ResetZoom,
    /// Change the library sort mode.
    SortLibrary(LibrarySortMode),
    /// Create a folder under the active folder.
    CreateFolder,
    /// Clear display metadata for selected PDFs.
    ResetMetadata,
    /// Apply title sort cleanup to selected PDFs.
    SortTitles,
    /// Refresh selected PDF metadata.
    RefreshMetadata,
    /// Rebuild selected PDF thumbnails.
    RebuildThumbnails,
    /// Reindex selected PDFs for full-text search.
    Reindex,
}

/// Contextual menus shown inside the selected-PDF menu strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMenu {
    /// Single-selection metadata actions.
    More,
    /// Bulk tag actions.
    Tags,
    /// Bulk folder membership actions.
    Folders,
    /// Bulk metadata actions.
    Metadata,
    /// Bulk maintenance actions.
    Maintenance,
}

/// Confirmation-only actions that overwrite or delete user-visible library data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationAction {
    /// Clear display metadata overrides for the selected PDFs.
    BulkResetDisplayMetadata,
    /// Delete the selected PDFs from library metadata.
    BulkDeleteFromLibrary,
    /// Clear display metadata overrides for one PDF in the details panel.
    ResetDetailsMetadata(EntryId),
}

/// Top selection-toolbar actions chosen from compact dropdown menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionToolbarAction {
    /// Add the typed tag to selected PDFs.
    AddTag,
    /// Remove the typed tag from selected PDFs.
    RemoveTag,
    /// Add selected PDFs to the active folder.
    AddToFolder,
    /// Remove selected PDFs from the active folder.
    RemoveFromFolder,
    /// Save the single selected PDF metadata edits.
    SaveDetails,
    /// Reset the single selected PDF metadata edits.
    ResetDetails,
    /// Recompute title sort keys.
    SortTitles,
    /// Refresh extracted PDF metadata.
    RefreshMetadata,
    /// Clear selected PDF display metadata overrides.
    ResetMetadata,
    /// Rebuild cover thumbnails.
    RebuildThumbnails,
    /// Reindex full text.
    Reindex,
    /// Delete selected PDFs from library metadata.
    DeleteMetadata,
}

impl SelectionToolbarAction {
    /// User-facing menu label.
    pub fn label(self) -> &'static str {
        match self {
            Self::AddTag => "Add tag",
            Self::RemoveTag => "Remove tag",
            Self::AddToFolder => "Add to folder",
            Self::RemoveFromFolder => "Remove from folder",
            Self::SaveDetails => "Save details",
            Self::ResetDetails => "Reset details",
            Self::SortTitles => "Sort titles",
            Self::RefreshMetadata => "Refresh metadata",
            Self::ResetMetadata => "Reset metadata",
            Self::RebuildThumbnails => "Rebuild thumbnails",
            Self::Reindex => "Reindex",
            Self::DeleteMetadata => "Delete metadata",
        }
    }
}

impl std::fmt::Display for SelectionToolbarAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Main navigation tabs inside the library sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySidebarTab {
    /// Folder hierarchy and all-library navigation.
    Files,
    /// Tag filtering navigation.
    Tags,
}

impl LibrarySidebarTab {
    /// User-facing tab label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Tags => "Tags",
        }
    }
}

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
        size: ThumbnailSize,
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
    /// The application window size changed.
    WindowResized { width: f32, height: f32 },
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
    /// Change the library sort mode.
    LibrarySortChanged(LibrarySortMode),
    /// Change the masonry grid card scale.
    LibraryGridZoomChanged(f32),
    /// Library view preferences were persisted.
    LibraryPreferencesSaved,
    /// Add an annotation.
    AnnotationAdded(Annotation),
    /// Delete an annotation.
    AnnotationDeleted(AnnotationId),
    /// Export annotations into a PDF.
    ExportAnnotations,
    /// Library entries loaded.
    LibraryLoaded(Vec<LibraryEntry>),
    /// Library folders loaded.
    LibraryFoldersLoaded(Vec<Folder>),
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
    /// Background author attribution finished.
    AuthorAttributionFinished,
    /// Open a library entry in the viewer.
    OpenLibraryEntry(EntryId),
    /// A library entry was clicked.
    LibraryEntryClicked(EntryId),
    /// A library entry hover target changed.
    LibraryEntryHoverChanged(EntryId, bool),
    /// Animation frame for active UI tweens.
    AnimationFrame(Instant),
    /// Clear the current library PDF selection.
    ClearLibrarySelection,
    /// Select all currently visible library PDFs.
    SelectAllVisibleLibraryEntries,
    /// Begin dragging a library entry for manual reordering.
    BeginLibraryEntryDrag(EntryId),
    /// Cursor moved while dragging a library entry.
    LibraryEntryDragMoved(Point),
    /// Auto-scroll timer tick while dragging a library entry.
    LibraryAutoScrollTick(Instant),
    /// Finish the active library entry drag.
    EndLibraryEntryDrag,
    /// Manual entry ordering was persisted.
    ManualEntryOrderSaved,
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
    LibraryScrolled {
        offset_y: f32,
        viewport_x: f32,
        viewport_y: f32,
        viewport_width: f32,
        viewport_height: f32,
    },
    /// Collapse the library tag sidebar.
    CollapseLibrarySidebar,
    /// Expand the library tag sidebar.
    ExpandLibrarySidebar,
    /// Begin resizing the library tag sidebar.
    BeginTagSidebarResize,
    /// Resize the library tag sidebar to a new logical width.
    TagSidebarResizeDragged(f32),
    /// Finish resizing the library tag sidebar.
    EndTagSidebarResize,
    /// Switch the active library sidebar navigation tab.
    LibrarySidebarTabChanged(LibrarySidebarTab),
    /// Expand or collapse the library root node in the sidebar file tree.
    ToggleLibraryTreeRoot,
    /// Expand or collapse one folder node in the sidebar file tree.
    ToggleLibraryTreeFolder(FolderId),
    /// A filesystem watcher event arrived.
    LibraryWatchEvent(LibraryWatchEvent),
    /// Tag filter changed.
    TagFilterChanged(Option<String>),
    /// Selected library folder changed.
    FolderSelected(Option<FolderId>),
    /// Inline new folder name changed.
    NewFolderNameChanged(String),
    /// Open the new-folder dialog.
    OpenCreateFolderDialog,
    /// Create a folder in the selected folder.
    CreateFolder,
    /// A folder was created.
    FolderCreated(FolderId),
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
    /// Bulk tag text changed.
    BulkTagInputChanged(String),
    /// Add the bulk tag to all selected PDFs.
    BulkAddTag,
    /// Remove the bulk tag from all selected PDFs.
    BulkRemoveTag,
    /// Add selected PDFs to the current folder.
    BulkAddToCurrentFolder,
    /// Remove selected PDFs from the current folder.
    BulkRemoveFromCurrentFolder,
    /// Clear display metadata overrides for selected PDFs.
    BulkResetDisplayMetadata,
    /// Recompute title sort keys for selected PDFs.
    BulkApplyTitleSortCleanup,
    /// Refresh extracted metadata for selected PDFs from the source files.
    BulkRefreshPdfMetadata,
    /// Rebuild thumbnails for selected PDFs.
    BulkRebuildThumbnails,
    /// Reindex full text for selected PDFs.
    BulkReindex,
    /// Delete selected PDFs from library metadata only.
    BulkDeleteFromLibrary,
    /// A compact selection-toolbar menu action was chosen.
    SelectionToolbarActionSelected(SelectionToolbarAction),
    /// Request confirmation before a destructive or overwriting library action.
    RequestConfirmation(ConfirmationAction),
    /// Run the currently pending destructive or overwriting library action.
    ConfirmPendingAction,
    /// Dismiss the active confirmation dialog.
    CancelConfirmation,
    /// Details-panel title override changed.
    DetailsTitleChanged(String),
    /// Details-panel author override changed.
    DetailsAuthorChanged(String),
    /// Persist details-panel metadata overrides.
    SaveDetailsMetadata,
    /// Reset one details-panel entry to extracted PDF metadata.
    ResetDetailsMetadata(EntryId),
    /// Metadata edit finished.
    MetadataEditFinished {
        entry_id: EntryId,
        label: String,
        errors: Vec<String>,
    },
    /// A bulk operation finished.
    BulkOperationFinished {
        label: String,
        updated: usize,
        errors: Vec<String>,
    },
    /// Reading progress changed.
    ProgressUpdated { entry_id: EntryId, page: u16 },
    /// Reading progress was saved.
    ProgressSaved,
    /// Toggle app theme.
    ThemeToggled,
    /// Reload KDL style files.
    ReloadStyles,
    /// KDL style reload finished.
    StylesReloaded(Result<Arc<StyleBook>, String>),
    /// A keyboard shortcut was pressed.
    ShortcutPressed(Shortcut),
    /// Settings changed.
    SettingsChanged(Settings),
    /// Open or switch the active top-level menu.
    AppMenuOpened(AppMenu),
    /// Close the active top-level menu.
    AppMenuClosed,
    /// Run an action selected from the top-level menu.
    AppMenuActionSelected(AppMenuAction),
    /// Open or switch the active selected-PDF contextual menu.
    SelectionMenuOpened(SelectionMenu),
    /// Close the active selected-PDF contextual menu.
    SelectionMenuClosed,
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
    /// Reload KDL styles.
    ReloadStyles,
    /// Scroll down by one viewport.
    PageDown,
    /// Scroll up by one viewport.
    PageUp,
    /// Scroll by a small number of logical pixels.
    FineScroll(i16),
    /// Pan horizontally by a small number of logical pixels.
    HorizontalPan(i16),
    /// Select all visible library entries.
    SelectAll,
    /// Open the selected library entry.
    OpenSelected,
    /// Delete selected library entries from metadata.
    DeleteSelected,
    /// Open the jump-to-page overlay.
    Jump,
    /// Close overlays or panels.
    Escape,
}
