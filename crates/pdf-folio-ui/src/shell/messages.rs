//! Application messages exchanged between UI views and update logic.
//!
//! [`Message`] is the single event vocabulary for the entire iced app. Views
//! and subscriptions emit variants; domain reducers pattern-match clusters of
//! them. Prefer extending an existing cluster over inventing a parallel
//! channel when adding features.
//!
//! # Message ownership
//!
//! 1. [`crate::library::update`] claims library selection, drag, folders,
//!    tags, bulk ops, import/export, Raindrop, search, and registry switcher
//!    flows that touch library data.
//! 2. [`crate::viewer::update`] claims zoom, scroll, page jump, outline,
//!    find-in-document, text selection, and page render results.
//! 3. [`super::update`] handles the remainder: startup probes, sync auth and
//!    auto-sync, file dialogs, chrome (context menu / command palette /
//!    confirmations), theme/style reload, shortcuts fan-out, and settings.
//!
//! Related types in this module (`ContextMenuTarget`, `ConfirmationAction`,
//! `Shortcut`, sidebar tabs) are small enums embedded in message payloads or
//! chrome state—not alternate event buses.
//!
//! # `Message` clusters (for navigation)
//!
//! | Cluster | Representative variants |
//! | --- | --- |
//! | Startup / lifecycle | `StartupResponsivenessProbe`, `StartupBackgroundReady`, `SessionSaved` |
//! | Sync auth & CRDT | `SyncSignIn*`, `AutoSync*`, `RemoteSyncAvailable`, `LibraryRegistry*` |
//! | Multi-library | `OpenLibrarySwitcher`, `SelectLibrary`, `CreateLibrary`, `DeleteLibrary` |
//! | File dialogs | `OpenFileDialog`, `FileSelected`, `Import*Dialog`, `ExportDestinationSelected` |
//! | Document open/render | `DocumentOpened`, `PageRendered`, `LibraryDocumentOpened`, `DocumentError` |
//! | Viewer navigation | `Viewport*`, `Jump*`, `PreviousPage` / `NextPage` |
//! | Viewer zoom | `ZoomIn` / `ZoomOut` / `ZoomSet`, `ZoomPresetSelected`, `ZoomRenderSettled` |
//! | Viewer text / find | `ViewerText*`, `OpenViewerFind`, `ViewerFind*`, `CopyViewerTextSelection` |
//! | Outline / sidebar | `ToggleOutlineNode`, `ToggleSidebar`, `ViewerSidebarTabSelected` |
//! | Library data load | `LibraryLoaded`, `LibraryFoldersLoaded`, `LibraryRefresh`, `LibraryError` |
//! | Library selection | `LibraryEntryClicked`, `EntryCheckboxToggled`, `SelectAll*`, clipboard |
//! | Library drag | `BeginLibraryEntryDrag`, `FolderDrag*`, `End*Drag`, `ManualEntryOrderSaved` |
//! | Search & filters | `SearchQueryChanged`, `TagFilterChanged`, `ReadingFilterChanged`, … |
//! | Folders & tags | `CreateFolder`, `Rename*`, `DeleteTag`, `BulkAddTag`, inspector tags |
//! | Bulk / destructive | `Bulk*`, `RequestConfirmation`, `ConfirmPendingAction` |
//! | Export | `OpenExportDialog`, `StartExport`, `ExportFinished` |
//! | Chrome | `ContextMenu*`, `CommandPalette*`, `CursorMoved` |
//! | Appearance | `ThemeToggled`, `ReloadStyles`, `StylesReloaded` |
//! | Input | `ShortcutPressed`, `ModifiersChanged`, `SettingsChanged` |

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use iced::keyboard;
use iced::Point;
use pdf_folio_cloud::raindrop::{
    RaindropImportDestination, RaindropImportPreview, RaindropImportProgress, RaindropImportSummary,
};
use pdf_folio_core::{
    EntryId, Folder, FolderId, ImportSummary, LibraryEntry, LibrarySortMode, LibraryWatchEvent,
};
use pdf_folio_core::{PageTextLayer, PdfDoc, TileKey};

use crate::library::state::{LibraryMetadataDensity, LibraryReadingFilter};
use crate::library::thumbnails::ThumbnailSize;
use crate::shell::commands::CommandId;
use crate::style::StyleBook;
use crate::viewer::rendering::ZoomPreset;
use crate::viewer::state::{ViewerScrollMode, ViewerSpreadMode};
use crate::{
    ExportConflictBehavior, ExportFilenameTemplate, ExportMode, ExportSource, LibraryExportSummary,
    Settings,
};

/// Right-click surfaces that can show contextual actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextMenuTarget {
    /// A library PDF entry.
    LibraryEntry(EntryId),
    /// A library folder, or the library root when `None`.
    Folder(Option<FolderId>),
    /// A library sidebar tag.
    Tag(String),
    /// Empty/library background space.
    LibraryBackground,
    /// The open document viewer canvas.
    ViewerCanvas,
}

/// Actions launched from right-click contextual menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuAction {
    /// Open the clicked library entry in the viewer.
    Open,
    /// Replace the multi-selection with only the clicked entry.
    SelectOnly,
    /// Add the clicked entry to the existing multi-selection.
    AddToSelection,
    /// Clear the current multi-selection.
    ClearSelection,
    /// Start tagging the clicked entry or selection.
    AddTag,
    /// Open the move-to-folder picker for the selection/entry.
    MoveTo,
    /// Open the export dialog for the selection/entry.
    Export,
    /// Reveal the PDF source file in the platform file manager.
    RevealInFileManager,
    /// Open the directory that contains the PDF source file.
    OpenContainingFolder,
    /// Pick a replacement path for a missing PDF source file.
    RelinkMissingFile,
    /// Persist details-panel metadata overrides for the entry.
    SaveDetails,
    /// Clear details overrides and restore extracted metadata.
    ResetDetails,
    /// Re-extract metadata from the source PDF file(s).
    RefreshMetadata,
    /// Clear display metadata overrides for the selection.
    ResetMetadata,
    /// Rebuild cover thumbnails for the selection.
    RebuildThumbnails,
    /// Rebuild the full-text search index for the selection.
    Reindex,
    /// Move the selection/entry to the trash can.
    DeleteFromLibrary,
    /// Select / scope the browser to the clicked folder.
    SelectFolder,
    /// Open the create-folder dialog under the current scope.
    NewFolder,
    /// Start renaming the clicked folder.
    RenameFolder,
    /// Start renaming the clicked tag.
    RenameTag,
    /// Delete the clicked tag from every PDF that uses it.
    DeleteTag,
    /// Open the move picker to re-parent the clicked folder.
    MoveFolderTo,
    /// Move the clicked folder to the library root.
    MoveFolderToRoot,
    /// Promote the clicked folder one level toward the root.
    MoveFolderUp,
    /// Reorder the clicked folder earlier among its siblings.
    MoveFolderEarlier,
    /// Reorder the clicked folder later among its siblings.
    MoveFolderLater,
    /// Move the clicked folder to the trash can.
    DeleteFolder,
    /// Open the native folder picker for bulk import.
    ImportFolder,
    /// Reload library entries and folders from storage.
    RefreshLibrary,
    /// Toggle between grid and list library layout.
    ToggleLayout,
    /// Sort the library by manual order.
    SortManual,
    /// Sort the library by title A→Z.
    SortTitleAsc,
    /// Copy the current PDF text selection to the OS clipboard.
    CopyViewerSelection,
    /// Open the find-in-document bar.
    FindInDocument,
    /// Open the jump-to-page overlay.
    JumpToPage,
    /// Increase viewer zoom.
    ZoomIn,
    /// Decrease viewer zoom.
    ZoomOut,
    /// Reset viewer zoom to the default width.
    ResetZoom,
    /// Show or hide the viewer outline / TOC sidebar.
    ToggleToc,
    /// Leave the viewer and return to the library.
    BackToLibrary,
}

/// Confirmation-only actions that overwrite, trash, or delete user-visible library data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationAction {
    /// Clear display metadata overrides for the selected PDFs.
    BulkResetDisplayMetadata,
    /// Move the selected PDFs to the Trash Can.
    BulkDeleteFromLibrary,
    /// Permanently delete selected PDFs from the trash.
    PermanentlyDeleteFromTrash,
    /// Permanently delete one selected folder subtree from the trash.
    PermanentlyDeleteFolderFromTrash(FolderId),
    /// Clear display metadata overrides for one PDF in the details panel.
    ResetDetailsMetadata(EntryId),
    /// Delete one folder without deleting PDFs on disk.
    DeleteFolder(FolderId),
    /// Remove a tag from every PDF that uses it.
    DeleteTag(String),
    /// Delete a whole discrete library database.
    DeleteLibrary(String),
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
    /// Tab title shown on the left library sidebar switcher.
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Tags => "Tags",
        }
    }
}

/// Navigation tabs inside the open-PDF viewer sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerSidebarTab {
    /// PDF outline/table of contents.
    Contents,
    /// Page thumbnail navigation.
    Thumbnails,
}

impl ViewerSidebarTab {
    /// Tab title shown on the open-PDF viewer sidebar switcher.
    pub fn label(self) -> &'static str {
        match self {
            Self::Contents => "Contents",
            Self::Thumbnails => "Thumbnails",
        }
    }
}

/// Messages handled by the PDF-Folio application update loop.
///
/// Every user action, async task completion, and subscription tick becomes a
/// `Message`. See the module-level cluster table for where to add variants
/// and which reducer typically handles them.
#[derive(Debug, Clone)]
pub enum Message {
    /// Startup responsiveness probe fired after launch.
    StartupResponsivenessProbe {
        /// When the whole app launch began.
        launch_started_at: Instant,
        /// When the probe task began waiting.
        probe_started_at: Instant,
        /// When the probe message was emitted to the update loop.
        emitted_at: Instant,
    },
    /// Startup background subscriptions may begin.
    StartupBackgroundReady,
    /// Open the native file picker.
    OpenFileDialog,
    /// Start Google sync sign-in.
    SyncSignInRequested,
    /// Google sync sign-in finished.
    SyncSignInFinished(Result<pdf_folio_cloud::sync::Session, String>),
    /// Periodic automatic sync timer fired.
    AutoSyncTick(Instant),
    /// Live remote watcher detected new CRDT operations.
    RemoteSyncAvailable {
        /// Library whose remote log advanced.
        library_id: String,
        /// Time the watcher noticed the remote change.
        noticed_at: Instant,
        /// Latest remote operation sequence observed.
        remote_sequence: i64,
    },
    /// Live remote watcher detected registry CRDT operations.
    LibraryRegistryRemoteAvailable {
        /// Time the watcher noticed the registry change.
        noticed_at: Instant,
        /// Latest remote registry operation sequence observed.
        remote_sequence: i64,
    },
    /// One automatic CRDT sync pass completed.
    AutoSyncFinished {
        /// Library that completed this sync pass.
        library_id: String,
        /// Sync result for the library.
        result: Result<pdf_folio_cloud::sync::SyncRunReport, String>,
    },
    /// App-level library registry sync completed.
    LibraryRegistrySyncFinished {
        /// Whether this registry sync should catch up every known library afterward.
        sync_all_after: bool,
        /// Updated registry after merging remote library profiles.
        result: Result<
            (
                crate::library::registry::LibraryRegistryRuntime,
                Vec<String>,
            ),
            String,
        >,
    },
    /// A library switcher preview was refreshed.
    LibraryPreviewRefreshed {
        /// Library whose preview was loaded.
        library_id: String,
        /// New preview payload.
        preview: crate::library::registry::LibraryPreview,
    },
    /// The native file picker was dismissed without choosing a file.
    FileDialogCanceled,
    /// A file was selected.
    FileSelected(PathBuf),
    /// A document was opened successfully.
    DocumentOpened { path: PathBuf, doc: Arc<PdfDoc> },
    /// A document operation failed.
    DocumentError(String),
    /// Dismiss the current document error banner.
    DismissDocumentError,
    /// A page render finished.
    PageRendered {
        key: TileKey,
        data: Vec<u8>,
        width: u16,
        height: u16,
        generation: Option<u64>,
    },
    /// A thumbnail render finished.
    ThumbnailReady {
        entry_id: EntryId,
        size: ThumbnailSize,
        data: Vec<u8>,
        width: u16,
        height: u16,
    },
    /// Scroll offset and viewport size changed.
    ViewportChanged {
        horizontal_offset: f32,
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
    /// Open the zoom percentage as an editable text input.
    StartZoomInputEdit,
    /// The typed zoom percentage changed.
    ZoomInputChanged(String),
    /// Submit the typed zoom percentage.
    SubmitZoomInput,
    /// Open or close the zoom preset menu.
    ToggleZoomMenu,
    /// Close the zoom preset menu.
    CloseZoomMenu,
    /// Apply a named zoom preset.
    ZoomPresetSelected(ZoomPreset),
    /// Set viewer scrolling behavior.
    ViewerScrollModeSelected(ViewerScrollMode),
    /// Set viewer spread behavior.
    ViewerSpreadModeSelected(ViewerSpreadMode),
    /// A wheel zoom gesture has been idle long enough to render the final zoom level.
    ZoomRenderSettled(u64),
    /// Jump to a zero-based page.
    JumpToPage(u16),
    /// Jump to the previous page.
    PreviousPage,
    /// Jump to the next page.
    NextPage,
    /// Expand or collapse a table-of-contents node.
    ToggleOutlineNode(Vec<usize>),
    /// Open the jump-to-page overlay.
    OpenJumpDialog,
    /// A page text layer was extracted.
    ViewerTextLayerLoaded {
        page: u16,
        layer: Arc<PageTextLayer>,
    },
    /// A page text-layer extraction failed.
    ViewerTextLayerError { page: u16, error: String },
    /// Start selecting PDF text at the character under the cursor.
    ViewerTextSelectionStarted { page: u16, char_index: usize },
    /// Extend PDF text selection to the character under the cursor.
    ViewerTextSelectionChanged { page: u16, char_index: usize },
    /// Finish the active PDF text selection drag.
    ViewerTextSelectionEnded,
    /// A click landed in the viewer canvas without starting a text selection.
    ViewerCanvasClicked,
    /// Clear the current PDF text selection.
    ClearViewerTextSelection,
    /// Copy the currently selected PDF text.
    CopyViewerTextSelection,
    /// Close the active overlay or panel.
    CloseOverlay,
    /// Last known app-window cursor position changed.
    CursorMoved(Point),
    /// Open a contextual right-click menu.
    ContextMenuOpened(ContextMenuTarget),
    /// Open a contextual right-click menu at a known window position.
    ContextMenuOpenedAt {
        target: ContextMenuTarget,
        position: Point,
    },
    /// Close the contextual right-click menu.
    ContextMenuClosed,
    /// Open the Library command palette.
    OpenCommandPalette,
    /// Close the Library command palette.
    CloseCommandPalette,
    /// Update the command-palette query.
    CommandPaletteQueryChanged(String),
    /// Move the highlighted command by a signed delta.
    CommandPaletteMoveSelection(i32),
    /// Run the highlighted command.
    CommandPaletteRunSelected,
    /// Run a command from the shared command registry.
    CommandPaletteRun(CommandId),
    /// A contextual menu action was chosen.
    ContextMenuActionSelected(ContextMenuAction),
    /// Show the viewer find-in-text bar.
    OpenViewerFind,
    /// Hide the viewer find-in-text bar.
    CloseViewerFind,
    /// Viewer find-in-text query changed.
    ViewerFindQueryChanged(String),
    /// Select the previous viewer find match.
    ViewerFindPrevious,
    /// Select the next viewer find match.
    ViewerFindNext,
    /// Toggle highlighting all viewer find matches.
    ViewerFindHighlightAllToggled(bool),
    /// Toggle case-sensitive viewer find matching.
    ViewerFindMatchCaseToggled(bool),
    /// Toggle diacritic-sensitive viewer find matching.
    ViewerFindMatchDiacriticsToggled(bool),
    /// The jump-to-page input changed.
    JumpInputChanged(String),
    /// Edit the toolbar page number directly.
    StartPageInputEdit,
    /// Submit the jump-to-page overlay.
    SubmitJump,
    /// Toggle the viewer sidebar (TOC / thumbnails rail).
    ToggleSidebar,
    /// Switch the active open-PDF viewer sidebar tab.
    ViewerSidebarTabSelected(ViewerSidebarTab),
    /// Toggle the placeholder view mode control.
    ToggleViewMode,
    /// Change the library sort mode.
    LibrarySortChanged(LibrarySortMode),
    /// Change the masonry grid card scale.
    LibraryGridZoomChanged(f32),
    /// Change the amount of metadata shown in library cards and rows.
    LibraryMetadataDensityChanged(LibraryMetadataDensity),
    /// Library view preferences were persisted.
    LibraryPreferencesSaved,
    /// Last app session was persisted.
    SessionSaved,
    /// Library entries loaded.
    LibraryLoaded {
        entries: Vec<LibraryEntry>,
        trash_entries: Vec<LibraryEntry>,
    },
    /// Open the library switcher screen.
    OpenLibrarySwitcher,
    /// Return from the library switcher to the active library.
    CloseLibrarySwitcher,
    /// Switch to a different discrete library.
    SelectLibrary(String),
    /// Toggle an existing library card overflow menu.
    ToggleLibraryCardMenu(String),
    /// Close the open library card overflow menu.
    CloseLibraryCardMenu,
    /// Open the create-library modal.
    OpenCreateLibraryDialog,
    /// Open the rename-library modal.
    OpenRenameLibraryDialog(String),
    /// Dismiss the active create/rename library modal.
    CancelLibraryNameDialog,
    /// Confirm the active create/rename library modal.
    ConfirmLibraryNameDialog,
    /// New-library name input changed.
    NewLibraryNameChanged(String),
    /// Create a new discrete library.
    CreateLibrary,
    /// A library registry mutation finished.
    LibraryRegistryUpdated(crate::library::registry::LibraryRegistryRuntime),
    /// Existing-library rename input changed.
    LibraryRenameInputChanged { library_id: String, value: String },
    /// Rename an existing discrete library.
    RenameLibrary(String),
    /// Request confirmation before deleting a discrete library.
    RequestDeleteLibrary(String),
    /// Delete a discrete library after confirmation.
    DeleteLibrary(String),
    /// Library folders loaded.
    LibraryFoldersLoaded(Vec<Folder>),
    /// Trashed library folders loaded.
    LibraryTrashFoldersLoaded(Vec<Folder>),
    /// Reload library entries from storage.
    LibraryRefresh,
    /// A library operation failed.
    LibraryError(String),
    /// Dismiss the current library error banner.
    DismissLibraryError,
    /// A library operation completed with a user-facing status.
    LibraryStatus(String),
    /// Open the unified import chooser.
    OpenImportMenu,
    /// Close the unified import chooser.
    CloseImportMenu,
    /// Open the native folder picker for bulk import.
    ImportFolderDialog,
    /// The native folder picker selected an import directory.
    ImportFolderSelected(PathBuf),
    /// Open the native file picker for single-PDF import.
    ImportPdfDialog,
    /// The native file picker selected a PDF to import.
    ImportPdfSelected(PathBuf),
    /// Bulk import finished.
    ImportFinished(ImportSummary),
    /// Close the import review sheet.
    CloseImportReview,
    /// Select all PDFs from the latest import review.
    SelectImportReviewEntries,
    /// Start importing PDFs from Raindrop.io.
    ImportRaindrop,
    /// Raindrop.io remote PDF preview loaded.
    RaindropImportPreviewLoaded(RaindropImportPreview),
    /// Remote Raindrop PDF thumbnail images loaded.
    RaindropPdfThumbnailsLoaded(Vec<(i64, Vec<u8>)>),
    /// Toggle one remote Raindrop PDF in the import picker.
    RaindropPdfToggled(i64, bool),
    /// Select all remote Raindrop PDFs in the import picker.
    SelectAllRaindropPdfs,
    /// Clear all remote Raindrop PDFs in the import picker.
    ClearAllRaindropPdfs,
    /// Change the selected Raindrop import destination.
    RaindropDestinationChanged(RaindropImportDestination),
    /// Toggle whether Raindrop folder structure is preserved during import.
    RaindropPreserveFolderStructureToggled(bool),
    /// Toggle the Raindrop import root folder selector.
    ToggleRaindropImportLocationMenu,
    /// Select the local folder treated as the import root.
    RaindropImportRootChanged(Option<FolderId>),
    /// Expand or collapse a folder branch in the Raindrop import location selector.
    ToggleRaindropImportLocationFolder(FolderId),
    /// Start creating a new import root folder.
    StartNewRaindropImportFolder,
    /// Update the new import root folder name.
    RaindropImportNewFolderNameChanged(String),
    /// Import selected PDFs from the Raindrop picker.
    ImportSelectedRaindropPdfs,
    /// Raindrop import progress changed.
    RaindropImportProgressUpdated(RaindropImportProgress),
    /// Raindrop import created a local folder.
    RaindropImportCreatedFolder(FolderId),
    /// Cancel the active Raindrop import and roll back imported files.
    CancelRaindropImport,
    /// Active Raindrop import rollback finished.
    RaindropImportRollbackFinished { removed: usize, errors: Vec<String> },
    /// Pending Raindrop rollback recovery check completed at startup.
    PendingRaindropRollbackChecked(Option<String>),
    /// Pending Raindrop rollback recovery finished.
    PendingRaindropRollbackFinished { removed: usize, errors: Vec<String> },
    /// Open Raindrop.io integrations settings in the browser.
    OpenRaindropIntegrations,
    /// Copy the Raindrop OAuth callback URL to the clipboard.
    CopyRaindropCallbackUrl,
    /// Raindrop OAuth client id input changed.
    RaindropClientIdChanged(String),
    /// Raindrop OAuth client secret input changed.
    RaindropClientSecretChanged(String),
    /// Start browser OAuth sign-in and import from Raindrop.io.
    SubmitRaindropSignIn,
    /// Raindrop.io import finished.
    RaindropImportFinished(RaindropImportSummary),
    /// Background author attribution finished.
    AuthorAttributionFinished,
    /// Open a library entry in the viewer.
    OpenLibraryEntry(EntryId),
    /// A library entry was clicked.
    LibraryEntryClicked(EntryId),
    /// A library folder was clicked.
    FolderClicked(Option<FolderId>),
    /// A library folder tree row was clicked.
    FolderTreeClicked(Option<FolderId>),
    /// Open a library folder from the sidebar file tree.
    FolderTreeFolderOpened(Option<FolderId>),
    /// Open the virtual trash can scope.
    OpenTrashCan,
    /// A library entry selection checkbox was toggled.
    EntryCheckboxToggled(EntryId),
    /// The master visible-entry selection checkbox was clicked.
    MasterCheckboxClicked,
    /// A library entry hover target changed.
    LibraryEntryHoverChanged(EntryId, bool),
    /// Animation frame for active UI tweens.
    AnimationFrame(Instant),
    /// Clear the current library PDF selection.
    ClearLibrarySelection,
    /// Clear the current library sidebar details and return to navigation.
    ClearLibrarySidebarDetails,
    /// Select all currently visible library PDFs.
    SelectAllVisibleLibraryEntries,
    /// Cut selected library PDFs or selected folder into the internal library clipboard.
    CutLibrarySelection,
    /// Copy selected library PDFs or selected folder into the internal library clipboard.
    CopyLibrarySelection,
    /// Paste the internal library clipboard into the active folder.
    PasteLibraryClipboard,
    /// A paste operation finished and can be pushed onto undo history.
    LibraryClipboardPasteFinished {
        action: crate::LibraryHistoryAction,
        clipboard: crate::LibraryClipboard,
        updated: usize,
        errors: Vec<String>,
    },
    /// A reversible library history operation finished.
    LibraryHistoryActionFinished {
        action: crate::LibraryHistoryAction,
        label: String,
        updated: usize,
        errors: Vec<String>,
    },
    /// Restore the previous library organization history snapshot.
    UndoLibraryAction,
    /// Restore the next library organization history snapshot.
    RedoLibraryAction,
    /// An undo or redo snapshot restore finished.
    LibraryHistoryRestoreFinished { target_index: usize, status: String },
    /// Begin dragging a library entry for manual reordering.
    BeginLibraryEntryDrag(EntryId),
    /// Cursor moved while dragging a library entry.
    LibraryEntryDragMoved(Point),
    /// Begin dragging a folder for nesting.
    BeginFolderDrag(FolderId),
    /// Begin dragging a folder from the sidebar file tree.
    BeginFolderTreeDrag(FolderId),
    /// Cursor moved while dragging a folder.
    FolderDragMoved(Point),
    /// A folder drop target changed while dragging PDFs.
    FolderDropTargetChanged(Option<FolderId>),
    /// The parent-directory drop target changed while dragging PDFs or folders.
    ParentDirectoryDropTargetChanged(bool),
    /// Auto-scroll timer tick while dragging a library entry.
    LibraryAutoScrollTick(Instant),
    /// Finish the active library entry drag.
    EndLibraryEntryDrag,
    /// Finish the active folder drag.
    EndFolderDrag,
    /// Manual entry ordering was persisted.
    ManualEntryOrderSaved,
    /// A library entry document was opened successfully.
    LibraryDocumentOpened { entry_id: EntryId, doc: Arc<PdfDoc> },
    /// Return from the viewer to the library.
    BackToLibrary,
    /// Return from the library to the already-open viewer document.
    BackToViewer,
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
    /// Toggle the library tag sidebar.
    ToggleLibrarySidebar,
    /// Begin resizing the library tag sidebar.
    BeginTagSidebarResize,
    /// Resize the library tag sidebar to a new logical width.
    TagSidebarResizeDragged(f32),
    /// Finish resizing the library tag sidebar.
    EndTagSidebarResize,
    /// Toggle the right-side Library inspector.
    ToggleLibraryInspector,
    /// Begin resizing the right-side Library inspector.
    BeginLibraryInspectorResize,
    /// Resize the right-side Library inspector from the current cursor x position.
    LibraryInspectorResizeDragged(f32),
    /// Finish resizing the right-side Library inspector.
    EndLibraryInspectorResize,
    /// Switch the active library sidebar navigation tab.
    LibrarySidebarTabChanged(LibrarySidebarTab),
    /// Expand or collapse the library root node in the sidebar file tree.
    ToggleLibraryTreeRoot,
    /// Expand or collapse the library tags section.
    ToggleLibraryTags,
    /// Expand or collapse one folder node in the sidebar file tree.
    ToggleLibraryTreeFolder(FolderId),
    /// A filesystem watcher event arrived.
    LibraryWatchEvent(LibraryWatchEvent),
    /// A filesystem watcher event was applied to the local library.
    LibraryWatchEventApplied(Result<(), String>),
    /// Tag filter changed.
    TagFilterChanged(Option<String>),
    /// A tag row in the sidebar was clicked.
    TagTreeClicked(String),
    /// A tag pill on a library card or row was clicked.
    TagPillClicked(String),
    /// Restore the library view shown before the last tag pill click.
    RestoreLibraryViewBeforeTag,
    /// Reading-progress filter changed.
    ReadingFilterChanged(Option<LibraryReadingFilter>),
    /// Recently-opened filter changed.
    RecentlyOpenedFilterChanged(bool),
    /// Missing-files filter changed.
    MissingFilterChanged(bool),
    /// Selected library folder changed.
    FolderSelected(Option<FolderId>),
    /// Clear active library search, tag, folder, reading, and missing filters.
    ClearLibraryFilters,
    /// Inline new folder name changed.
    NewFolderNameChanged(String),
    /// Open the new-folder dialog.
    OpenCreateFolderDialog,
    /// Create a folder in the selected folder.
    CreateFolder,
    /// A folder was created.
    FolderCreated {
        folder_id: FolderId,
        action: crate::LibraryHistoryAction,
    },
    /// Selected-folder rename input changed.
    FolderRenameInputChanged(String),
    /// Rename the selected folder.
    RenameSelectedFolder,
    /// Move the selected folder to the library root.
    MoveSelectedFolderToRoot,
    /// Move the selected folder up to its grandparent.
    MoveSelectedFolderUp,
    /// Move the selected folder earlier among its siblings.
    MoveSelectedFolderEarlier,
    /// Move the selected folder later among its siblings.
    MoveSelectedFolderLater,
    /// Open the folder picker for moving selected PDFs.
    OpenMoveSelectionDialog,
    /// Open the folder picker for moving the selected folder.
    OpenMoveSelectedFolderDialog,
    /// The move picker selected a destination folder, or the library root.
    MovePickerDestinationSelected(Option<FolderId>),
    /// Expand or collapse one folder branch in the move picker.
    ToggleMovePickerFolder(FolderId),
    /// Move the pending library content to the selected picker destination.
    ConfirmMovePicker,
    /// Dismiss the library move picker.
    CancelMovePicker,
    /// Request confirmation before deleting the selected folder.
    RequestDeleteSelectedFolder,
    /// Delete the selected folder after confirmation.
    DeleteFolder(FolderId),
    /// Folder metadata changed and the library should refresh.
    FolderUpdated,
    /// Start inline tag entry for an item.
    StartTagEntry(EntryId),
    /// Inline tag text changed.
    TagInputChanged(String),
    /// Submit the active inline tag.
    SubmitTag,
    /// Start renaming one sidebar tag.
    StartTagRename(String),
    /// Inline sidebar tag rename text changed.
    TagRenameInputChanged(String),
    /// Submit the active sidebar tag rename.
    SubmitTagRename,
    /// Cancel the active sidebar tag rename.
    CancelTagRename,
    /// Delete one tag from all PDFs.
    DeleteTag(String),
    /// A library entry was tagged.
    EntryTagged { id: EntryId, tag: String },
    /// A library entry tag was removed.
    EntryUntagged { id: EntryId, tag: String },
    /// A library entry was deleted.
    EntryDeleted(EntryId),
    /// Bulk tag text changed.
    BulkTagInputChanged(String),
    /// Inspector tag input changed.
    InspectorTagInputChanged(String),
    /// Apply an existing suggested tag from the inspector.
    InspectorApplyTag(String),
    /// Add the inspector tag input to selected PDFs.
    InspectorAddTag,
    /// Remove a tag from one entry through the inspector.
    InspectorRemoveTag { entry_id: EntryId, tag: String },
    /// Remove a tag from every selected PDF through the inspector.
    InspectorRemoveTagFromSelection(String),
    /// Open the tag manager modal.
    OpenTagManager,
    /// Close the tag manager modal.
    CloseTagManager,
    /// Tag manager filter changed.
    TagManagerFilterChanged(String),
    /// Tag manager merge destination changed.
    TagManagerMergeDestinationChanged(String),
    /// Merge one tag into another existing/new tag.
    MergeTag { source: String, destination: String },
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
    /// Move selected PDFs to the Trash Can.
    BulkDeleteFromLibrary,
    /// Restore selected PDFs from the trash.
    RestoreSelectedFromTrash,
    /// Permanently delete selected PDFs from the trash.
    PermanentlyDeleteSelectedFromTrash,
    /// Permanently delete the selected folder subtree from the trash.
    PermanentlyDeleteSelectedFolderFromTrash(FolderId),
    /// A trashed folder subtree was permanently deleted.
    TrashFolderPermanentlyDeleted { updated: usize, errors: Vec<String> },
    /// Request confirmation before a destructive or overwriting library action.
    RequestConfirmation(ConfirmationAction),
    /// Run the currently pending destructive or overwriting library action.
    ConfirmPendingAction,
    /// Dismiss the active confirmation dialog.
    CancelConfirmation,
    /// Toggle the session-only folder delete warning suppression checkbox.
    FolderDeleteWarningSuppressionToggled(bool),
    /// Details-panel title override changed.
    DetailsTitleChanged(String),
    /// Details-panel author override changed.
    DetailsAuthorChanged(String),
    /// Persist details-panel metadata overrides.
    SaveDetailsMetadata,
    /// Reset one details-panel entry to extracted PDF metadata.
    ResetDetailsMetadata(EntryId),
    /// Reveal one PDF in the platform file manager where supported.
    RevealEntryInFileManager(EntryId),
    /// Open the containing folder for one PDF.
    OpenEntryContainingFolder(EntryId),
    /// Copy one PDF source file path.
    CopyEntryFilePath(EntryId),
    /// Pick a replacement source file for a missing PDF.
    RelinkMissingEntry(EntryId),
    /// A replacement source file was chosen for a missing PDF.
    RelinkFileSelected { entry_id: EntryId, path: PathBuf },
    /// Relinking a missing PDF finished.
    RelinkFinished { entry_id: EntryId, path: PathBuf },
    /// Metadata edit finished.
    MetadataEditFinished {
        entry_id: EntryId,
        action: crate::LibraryHistoryAction,
        label: String,
        errors: Vec<String>,
    },
    /// A bulk operation finished.
    BulkOperationFinished {
        label: String,
        updated: usize,
        errors: Vec<String>,
    },
    /// Open the library export dialog.
    OpenExportDialog(ExportSource),
    /// Close the library export dialog or completion sheet.
    CloseExportDialog,
    /// Native folder picker chose an export destination.
    ExportDestinationSelected(PathBuf),
    /// Open the native folder picker for an export destination.
    ChooseExportDestination,
    /// Export mode changed.
    ExportModeChanged(ExportMode),
    /// Export filename template changed.
    ExportFilenameTemplateChanged(ExportFilenameTemplate),
    /// Export metadata CSV option changed.
    ExportMetadataCsvToggled(bool),
    /// Export metadata JSON option changed.
    ExportMetadataJsonToggled(bool),
    /// Export tags option changed.
    ExportTagsToggled(bool),
    /// Export reading progress option changed.
    ExportReadingProgressToggled(bool),
    /// Export conflict behavior changed.
    ExportConflictBehaviorChanged(ExportConflictBehavior),
    /// Start the configured export.
    StartExport,
    /// Export finished.
    ExportFinished(Result<LibraryExportSummary, String>),
    /// Reveal the last export destination in the platform file manager.
    RevealExportedFolder,
    /// Copy the last export destination path.
    CopyExportPath,
    /// A drag-to-folder assignment finished.
    FolderAssignmentFinished {
        folder_id: Option<FolderId>,
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
}

/// Keyboard shortcuts recognized by the shell shortcut mapper.
///
/// Produced by [`super::shortcuts::keyboard_event_message`] and applied by
/// [`super::shortcuts::handle_shortcut`]. Some variants are mode-sensitive
/// (library selection vs viewer zoom/scroll).
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
    /// Focus the library search field.
    FocusSearch,
    /// Focus the selected PDF title or selected folder name for rename.
    RenameSelected,
    /// Move selected library entries to the Trash Can.
    DeleteSelected,
    /// Cut selected library entries or folders.
    Cut,
    /// Open the jump-to-page overlay.
    Jump,
    /// Copy selected text.
    Copy,
    /// Paste selected library entries or folders.
    Paste,
    /// Undo the latest library organization edit.
    Undo,
    /// Redo the latest undone library organization edit.
    Redo,
    /// Open the Library command palette.
    OpenCommandPalette,
    /// Toggle the left-side Library sidebar.
    ToggleLibrarySidebar,
    /// Toggle the right-side Library inspector.
    ToggleLibraryInspector,
    /// Close overlays or panels.
    Escape,
}
