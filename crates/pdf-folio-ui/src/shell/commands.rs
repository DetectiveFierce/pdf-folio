//! Shared command registry for command palette, menus, and context actions.
//!
//! Commands are a higher-level intent layer on top of [`super::messages::Message`].
//! A [`CommandId`] is a stable identifier used by the palette and menus;
//! resolution helpers decide whether a command is visible/enabled in the
//! current app state and which message to emit when it runs.
//!
//! # Key types
//!
//! - [`CommandId`] — stable id for palette / menu entries.
//! - [`CommandSpec`] — static label, category, surface, and danger metadata.
//! - [`ResolvedCommand`] — runtime-resolved enablement and target message.
//! - [`CommandSurface`] — which UI surface exposes the command.
//!
//! # Related modules
//!
//! - [`super::messages`] — messages produced when a command runs.
//! - [`crate::components::shared::command_palette`] — palette UI.
//! - [`super::shortcuts`] — some shortcuts map to the same intents.

use crate::*;

/// Stable identifier for a command-palette / menu command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    /// Open a PDF via the native file picker (viewer / global).
    OpenFile,
    /// Import one or more PDFs into the active library.
    ImportPdf,
    /// Import every PDF under a chosen folder into the library.
    ImportFolder,
    /// Start the Raindrop.io import flow.
    ImportRaindrop,
    /// Leave the viewer and return to the library surface.
    BackToLibrary,
    /// Reload library entries and folders from the database.
    RefreshLibrary,
    /// Undo the latest library organization edit.
    UndoLibraryAction,
    /// Redo the latest undone library organization edit.
    RedoLibraryAction,
    /// Cut selected PDFs or the focused folder into the in-app clipboard.
    CutLibrarySelection,
    /// Copy selected PDFs or the focused folder into the in-app clipboard.
    CopyLibrarySelection,
    /// Paste the in-app clipboard into the active folder scope.
    PasteLibraryClipboard,
    /// Restore filters/scroll captured before a tag-pill drill-in.
    RestoreTagPillView,
    /// Clear folder scope and show all PDFs in the library root.
    GoToLibraryRoot,
    /// Open the trash can browser scope.
    GoToTrash,
    /// Clear search, tag, folder, reading, and missing filters.
    ClearFilters,
    /// Select every PDF currently visible in the browser.
    SelectAllVisible,
    /// Clear the current multi-selection.
    ClearSelection,
    /// Open the single selected PDF in the viewer.
    OpenSelected,
    /// Open the move-to-folder picker for the selection.
    MoveSelectionToFolder,
    /// Move the selection to the trash can (with confirmation).
    MoveSelectionToTrash,
    /// Add the bulk-tag input string to every selected PDF.
    AddTypedTag,
    /// Remove the bulk-tag input string from every selected PDF.
    RemoveTypedTag,
    /// Add selected PDFs to the currently scoped folder.
    AddSelectionToCurrentFolder,
    /// Remove selected PDFs from the currently scoped folder.
    RemoveSelectionFromCurrentFolder,
    /// Open the create-folder dialog under the current scope.
    CreateFolder,
    /// Focus rename for the selected/details folder.
    RenameFolder,
    /// Open the move picker to re-parent the selected folder.
    MoveFolderTo,
    /// Re-parent the selected folder to the library root.
    MoveFolderToRoot,
    /// Promote the selected folder one level toward the root.
    MoveFolderUp,
    /// Reorder the selected folder earlier among its siblings.
    MoveFolderEarlier,
    /// Reorder the selected folder later among its siblings.
    MoveFolderLater,
    /// Move the selected folder to the trash can.
    MoveFolderToTrash,
    /// Start renaming the active tag filter.
    RenameTag,
    /// Delete the active tag from every PDF that uses it.
    DeleteTag,
    /// Open the tag manager merge flow for the active tag.
    MergeTag,
    /// Persist details-panel title/author overrides for the selected PDF.
    SaveDetails,
    /// Clear details-panel overrides and restore extracted PDF metadata.
    ResetDetails,
    /// Re-extract metadata from source files for the selection.
    RefreshMetadata,
    /// Clear display metadata overrides for the selection.
    ResetDisplayMetadata,
    /// Recompute title sort keys for the selection.
    ApplyTitleSortCleanup,
    /// Rebuild cover thumbnails for the selection.
    RebuildThumbnails,
    /// Rebuild the full-text search index for the selection.
    ReindexFullText,
    /// Toggle between masonry grid and compact list layout.
    ToggleLibraryLayout,
    /// Show or hide the left library sidebar.
    ToggleLibrarySidebar,
    /// Show or hide the right library inspector.
    ToggleLibraryInspector,
    /// Sort the library by manual order.
    SortManual,
    /// Sort the library by title A→Z.
    SortTitleAsc,
    /// Sort the library by title Z→A.
    SortTitleDesc,
    /// Sort the library by author A→Z.
    SortAuthorAsc,
    /// Sort the library by author Z→A.
    SortAuthorDesc,
    /// Sort the library by most recently added.
    SortRecentlyAdded,
    /// Sort the library by most recently opened.
    SortRecentlyOpened,
    /// Sort the library by reading progress.
    SortReadingProgress,
    /// Sort the library by page count.
    SortPageCount,
    /// Sort the library with missing files first / grouped.
    SortMissingFiles,
    /// Cycle or set the library metadata density on cards/rows.
    SetMetadataDensity,
    /// Toggle the missing-files-only filter.
    ToggleMissingFiles,
    /// Scope the browser to recently added PDFs.
    GoToRecentlyAdded,
    /// Scope the browser to recently opened PDFs.
    GoToRecentlyOpened,
    /// Scope the browser to PDFs not filed in any folder.
    GoToUnfiled,
    /// Jump navigation to a chosen folder (palette stub / future picker).
    GoToFolder,
    /// Jump navigation to a chosen tag (palette stub / future picker).
    GoToTag,
    /// Focus the bulk/inspector tag input for the selection.
    AddTagToSelection,
    /// Open the export dialog for the selected PDFs.
    ExportSelectedPdfs,
    /// Toggle light/dark theme.
    ToggleTheme,
    /// Reload KDL style files from disk.
    ReloadStyles,
    /// Show or hide the viewer outline / TOC sidebar.
    ToggleToc,
    /// Open the jump-to-page overlay in the viewer.
    JumpToPage,
    /// Open the find-in-document bar in the viewer.
    FindInDocument,
    /// Increase viewer zoom.
    ZoomIn,
    /// Decrease viewer zoom.
    ZoomOut,
    /// Reset viewer zoom to the default width.
    ResetZoom,
    /// Use page-at-a-time viewer scrolling.
    SetViewerScrollPage,
    /// Use continuous vertical viewer scrolling.
    SetViewerScrollVertical,
    /// Use continuous horizontal viewer scrolling.
    SetViewerScrollHorizontal,
    /// Use wrapped multi-column viewer scrolling.
    SetViewerScrollWrapped,
    /// Show single pages without two-page spreads.
    SetViewerSpreadNone,
    /// Pair pages with odd pages on the left.
    SetViewerSpreadOdd,
    /// Pair pages with even pages on the left (cover-style).
    SetViewerSpreadEven,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Grouping category for command palette sections.
pub enum CommandCategory {
    /// Library-wide maintenance and undo/redo.
    Library,
    /// Import PDFs from disk or Raindrop.
    Import,
    /// Multi-select clipboard and bulk selection actions.
    Selection,
    /// Folder create/rename/move/trash.
    Folder,
    /// Tag rename/delete/merge and bulk tagging.
    Tag,
    /// Display metadata and details-panel edits.
    Metadata,
    /// Export selected PDFs to disk.
    Export,
    /// Thumbnail rebuild and full-text reindex.
    Maintenance,
    /// Scope/filter navigation (root, trash, recent, …).
    Navigation,
    /// Layout, sort, density, and viewer view modes.
    View,
    /// In-document jump and find.
    Document,
    /// Theme and style reload.
    Appearance,
}

impl CommandCategory {
    /// Section heading shown when grouping commands in the palette UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Import => "Import",
            Self::Selection => "Selection",
            Self::Folder => "Folder",
            Self::Tag => "Tag",
            Self::Metadata => "Metadata",
            Self::Export => "Export",
            Self::Maintenance => "Maintenance",
            Self::Navigation => "Navigation",
            Self::View => "View",
            Self::Document => "Document",
            Self::Appearance => "Appearance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What kind of target a command operates on (entry, folder, …).
pub enum CommandTargetKind {
    /// No specific target; global or mode-level command.
    None,
    /// Requires library mode / library surface context.
    Library,
    /// Operates on the selected or details folder.
    Folder,
    /// Operates on the active tag filter.
    Tag,
    /// Requires exactly one selected PDF.
    SinglePdf,
    /// Requires one or more selected PDFs.
    MultiplePdfs,
    /// Operates on the current search/browse result set.
    SearchResult,
    /// Requires an open viewer with a document.
    Viewer,
    /// In-document command (find, jump) while a PDF is open.
    Document,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Danger level used to style destructive commands.
pub enum CommandDanger {
    /// Non-destructive; default palette styling.
    Safe,
    /// Deletes or trashes user data; styled as destructive.
    Destructive,
    /// Overwrites display or extracted metadata; warn-style affordance.
    OverwritesMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Which UI surface exposes a command (library, viewer, global).
pub enum CommandSurface {
    /// Overflow "more" menu in the library header.
    HeaderMore,
    /// Unified import chooser dropdown.
    ImportMenu,
    /// Floating toolbar shown when PDFs are multi-selected.
    SelectionToolbar,
    /// Full command palette overlay.
    CommandPalette,
}

#[derive(Debug, Clone, Copy)]
/// Static specification of a command (id, label, category, surface).
pub struct CommandSpec {
    /// Stable id used by palette, menus, and enablement helpers.
    pub id: CommandId,
    /// Primary user-facing label in palette and menu rows.
    pub label: &'static str,
    /// Optional embedded icon bytes for menu rows.
    pub icon: Option<&'static [u8]>,
    /// Optional shortcut hint string shown beside the label.
    pub shortcut: Option<&'static str>,
    /// Palette section grouping.
    pub category: CommandCategory,
    /// Kind of selection/context the command expects.
    pub target: CommandTargetKind,
    /// Visual danger styling for destructive or overwriting actions.
    pub danger: CommandDanger,
    /// Extra search terms matched by the palette filter.
    pub aliases: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
/// A command after context resolution (enabled/visible/message).
pub struct ResolvedCommand {
    /// Static metadata for this command.
    pub spec: CommandSpec,
    /// Whether the command can run in the current app state.
    pub enabled: bool,
    /// Whether the command should appear on the requesting surface.
    pub visible: bool,
}

/// Static table of every command palette / menu command specification.
const COMMAND_SPECS: &[CommandSpec] = &[
    spec(
        CommandId::OpenFile,
        "Open PDF",
        Some("Ctrl+O"),
        CommandCategory::Import,
        CommandTargetKind::None,
        CommandDanger::Safe,
        &["open file"],
    ),
    spec(
        CommandId::ImportPdf,
        "Import PDFs",
        None,
        CommandCategory::Import,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["upload pdf"],
    ),
    spec(
        CommandId::ImportFolder,
        "Import Folder",
        None,
        CommandCategory::Import,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["upload folder"],
    ),
    spec(
        CommandId::ImportRaindrop,
        "Import from Raindrop",
        None,
        CommandCategory::Import,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["raindrop"],
    ),
    spec(
        CommandId::BackToLibrary,
        "Back to Library",
        Some("Esc"),
        CommandCategory::Navigation,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["return"],
    ),
    spec(
        CommandId::RefreshLibrary,
        "Refresh Library",
        Some("F5"),
        CommandCategory::Library,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["reload"],
    ),
    spec(
        CommandId::UndoLibraryAction,
        "Undo",
        Some("Ctrl+Z"),
        CommandCategory::Library,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["undo library"],
    ),
    spec(
        CommandId::RedoLibraryAction,
        "Redo",
        Some("Ctrl+Y"),
        CommandCategory::Library,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["redo library"],
    ),
    spec(
        CommandId::CutLibrarySelection,
        "Cut",
        Some("Ctrl+X"),
        CommandCategory::Selection,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["clipboard"],
    ),
    spec(
        CommandId::CopyLibrarySelection,
        "Copy",
        Some("Ctrl+C"),
        CommandCategory::Selection,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["clipboard"],
    ),
    spec(
        CommandId::PasteLibraryClipboard,
        "Paste",
        Some("Ctrl+V"),
        CommandCategory::Selection,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["clipboard"],
    ),
    spec(
        CommandId::RestoreTagPillView,
        "Previous Library View",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["back"],
    ),
    spec(
        CommandId::GoToLibraryRoot,
        "Go to All PDFs",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["root", "library"],
    ),
    spec(
        CommandId::GoToTrash,
        "Go to Trash",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["trash can"],
    ),
    spec(
        CommandId::ClearFilters,
        "Clear Filters",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["reset filters"],
    ),
    spec(
        CommandId::SelectAllVisible,
        "Select All Visible PDFs",
        Some("Ctrl+A"),
        CommandCategory::Selection,
        CommandTargetKind::SearchResult,
        CommandDanger::Safe,
        &["select all"],
    ),
    spec(
        CommandId::ClearSelection,
        "Clear Selection",
        Some("Esc"),
        CommandCategory::Selection,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["deselect"],
    ),
    spec(
        CommandId::OpenSelected,
        "Open Selected PDF",
        Some("Enter"),
        CommandCategory::Selection,
        CommandTargetKind::SinglePdf,
        CommandDanger::Safe,
        &["open"],
    ),
    spec(
        CommandId::MoveSelectionToFolder,
        "Move Selection to Folder",
        None,
        CommandCategory::Selection,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["move"],
    ),
    spec(
        CommandId::MoveSelectionToTrash,
        "Move Selection to Trash",
        Some("Delete"),
        CommandCategory::Selection,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Destructive,
        &["delete"],
    ),
    spec(
        CommandId::AddTypedTag,
        "Add Typed Tag",
        None,
        CommandCategory::Tag,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["bulk tag"],
    ),
    spec(
        CommandId::RemoveTypedTag,
        "Remove Typed Tag",
        None,
        CommandCategory::Tag,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["bulk tag"],
    ),
    spec(
        CommandId::AddSelectionToCurrentFolder,
        "Add Selection to Current Folder",
        None,
        CommandCategory::Folder,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["folder"],
    ),
    spec(
        CommandId::RemoveSelectionFromCurrentFolder,
        "Remove Selection from Current Folder",
        None,
        CommandCategory::Folder,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["folder"],
    ),
    spec(
        CommandId::CreateFolder,
        "New Folder",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["create folder"],
    ),
    spec(
        CommandId::RenameFolder,
        "Rename Folder",
        Some("F2"),
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["folder name"],
    ),
    spec(
        CommandId::MoveFolderTo,
        "Move Folder",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["folder move"],
    ),
    spec(
        CommandId::MoveFolderToRoot,
        "Move Folder to Root",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["root folder"],
    ),
    spec(
        CommandId::MoveFolderUp,
        "Move Folder Up",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["promote folder"],
    ),
    spec(
        CommandId::MoveFolderEarlier,
        "Move Folder Earlier",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["reorder"],
    ),
    spec(
        CommandId::MoveFolderLater,
        "Move Folder Later",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["reorder"],
    ),
    spec(
        CommandId::MoveFolderToTrash,
        "Move Folder to Trash",
        None,
        CommandCategory::Folder,
        CommandTargetKind::Folder,
        CommandDanger::Destructive,
        &["delete folder"],
    ),
    spec(
        CommandId::RenameTag,
        "Rename Tag",
        None,
        CommandCategory::Tag,
        CommandTargetKind::Tag,
        CommandDanger::Safe,
        &["tag name"],
    ),
    spec(
        CommandId::DeleteTag,
        "Delete Tag",
        None,
        CommandCategory::Tag,
        CommandTargetKind::Tag,
        CommandDanger::Destructive,
        &["remove tag"],
    ),
    spec(
        CommandId::MergeTag,
        "Merge Tag",
        None,
        CommandCategory::Tag,
        CommandTargetKind::Tag,
        CommandDanger::Safe,
        &["combine tags"],
    ),
    spec(
        CommandId::SaveDetails,
        "Save Details",
        Some("Enter"),
        CommandCategory::Metadata,
        CommandTargetKind::SinglePdf,
        CommandDanger::Safe,
        &["save metadata"],
    ),
    spec(
        CommandId::ResetDetails,
        "Reset Details",
        None,
        CommandCategory::Metadata,
        CommandTargetKind::SinglePdf,
        CommandDanger::OverwritesMetadata,
        &["reset metadata"],
    ),
    spec(
        CommandId::RefreshMetadata,
        "Refresh Metadata",
        None,
        CommandCategory::Metadata,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::OverwritesMetadata,
        &["pdf metadata"],
    ),
    spec(
        CommandId::ResetDisplayMetadata,
        "Reset Display Metadata",
        None,
        CommandCategory::Metadata,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::OverwritesMetadata,
        &["clear metadata"],
    ),
    spec(
        CommandId::ApplyTitleSortCleanup,
        "Apply Title Sort Cleanup",
        None,
        CommandCategory::Metadata,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::OverwritesMetadata,
        &["sort titles"],
    ),
    spec(
        CommandId::RebuildThumbnails,
        "Rebuild Thumbnails",
        None,
        CommandCategory::Maintenance,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["covers"],
    ),
    spec(
        CommandId::ReindexFullText,
        "Reindex Full Text",
        None,
        CommandCategory::Maintenance,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["search index"],
    ),
    spec(
        CommandId::ToggleLibraryLayout,
        "Toggle Grid/List",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["view mode"],
    ),
    spec(
        CommandId::ToggleLibrarySidebar,
        "Toggle Library Sidebar",
        Some("B"),
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["left sidebar", "folders", "tags"],
    ),
    spec(
        CommandId::ToggleLibraryInspector,
        "Toggle Library Inspector",
        Some("I"),
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["right sidebar", "details"],
    ),
    spec(
        CommandId::SortManual,
        "Sort by Manual",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort"],
    ),
    spec(
        CommandId::SortTitleAsc,
        "Sort by Title A-Z",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort"],
    ),
    spec(
        CommandId::SortTitleDesc,
        "Sort by Title Z-A",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort"],
    ),
    spec(
        CommandId::SortAuthorAsc,
        "Sort by Author A-Z",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort"],
    ),
    spec(
        CommandId::SortAuthorDesc,
        "Sort by Author Z-A",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort"],
    ),
    spec(
        CommandId::SortRecentlyAdded,
        "Sort by Recently Added",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort recent"],
    ),
    spec(
        CommandId::SortRecentlyOpened,
        "Sort by Recently Opened",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort opened"],
    ),
    spec(
        CommandId::SortReadingProgress,
        "Sort by Reading Progress",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort progress"],
    ),
    spec(
        CommandId::SortPageCount,
        "Sort by Page Count",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort pages"],
    ),
    spec(
        CommandId::SortMissingFiles,
        "Sort by Missing Files",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["sort missing"],
    ),
    spec(
        CommandId::SetMetadataDensity,
        "Set Metadata Density",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["density"],
    ),
    spec(
        CommandId::ToggleMissingFiles,
        "Toggle Missing Files",
        None,
        CommandCategory::View,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["missing"],
    ),
    spec(
        CommandId::GoToRecentlyAdded,
        "Go to Recently Added",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["recent"],
    ),
    spec(
        CommandId::GoToRecentlyOpened,
        "Go to Recently Opened",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["opened"],
    ),
    spec(
        CommandId::GoToUnfiled,
        "Go to Unfiled",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Library,
        CommandDanger::Safe,
        &["no folder"],
    ),
    spec(
        CommandId::GoToFolder,
        "Go to Folder...",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Folder,
        CommandDanger::Safe,
        &["folder"],
    ),
    spec(
        CommandId::GoToTag,
        "Go to Tag...",
        None,
        CommandCategory::Navigation,
        CommandTargetKind::Tag,
        CommandDanger::Safe,
        &["tag"],
    ),
    spec(
        CommandId::AddTagToSelection,
        "Add Tag to Selection",
        None,
        CommandCategory::Tag,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["bulk tag"],
    ),
    spec(
        CommandId::ExportSelectedPdfs,
        "Export Selected PDFs",
        None,
        CommandCategory::Export,
        CommandTargetKind::MultiplePdfs,
        CommandDanger::Safe,
        &["save copies"],
    ),
    spec(
        CommandId::ToggleTheme,
        "Toggle Theme",
        Some("Ctrl+Shift+T"),
        CommandCategory::Appearance,
        CommandTargetKind::None,
        CommandDanger::Safe,
        &["dark", "light"],
    ),
    spec(
        CommandId::ReloadStyles,
        "Reload Styles",
        Some("Ctrl+Shift+R"),
        CommandCategory::Appearance,
        CommandTargetKind::None,
        CommandDanger::Safe,
        &["kdl", "theme"],
    ),
    spec(
        CommandId::ToggleToc,
        "Toggle Table of Contents",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["sidebar", "outline"],
    ),
    spec(
        CommandId::JumpToPage,
        "Jump to Page",
        Some("Ctrl+G"),
        CommandCategory::Document,
        CommandTargetKind::Document,
        CommandDanger::Safe,
        &["go to page"],
    ),
    spec(
        CommandId::FindInDocument,
        "Find in Document",
        Some("Ctrl+F"),
        CommandCategory::Document,
        CommandTargetKind::Document,
        CommandDanger::Safe,
        &["search pdf"],
    ),
    spec(
        CommandId::ZoomIn,
        "Zoom In",
        Some("Ctrl++"),
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["increase zoom"],
    ),
    spec(
        CommandId::ZoomOut,
        "Zoom Out",
        Some("Ctrl+-"),
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["decrease zoom"],
    ),
    spec(
        CommandId::ResetZoom,
        "Reset Zoom",
        Some("Ctrl+0"),
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["automatic zoom"],
    ),
    spec(
        CommandId::SetViewerScrollPage,
        "Page Scrolling",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["scroll"],
    ),
    spec(
        CommandId::SetViewerScrollVertical,
        "Vertical Scrolling",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["scroll continuous"],
    ),
    spec(
        CommandId::SetViewerScrollHorizontal,
        "Horizontal Scrolling",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["scroll"],
    ),
    spec(
        CommandId::SetViewerScrollWrapped,
        "Wrapped Scrolling",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["scroll wrap"],
    ),
    spec(
        CommandId::SetViewerSpreadNone,
        "No Spreads",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["single page"],
    ),
    spec(
        CommandId::SetViewerSpreadOdd,
        "Odd Spreads",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["two page"],
    ),
    spec(
        CommandId::SetViewerSpreadEven,
        "Even Spreads",
        None,
        CommandCategory::View,
        CommandTargetKind::Viewer,
        CommandDanger::Safe,
        &["cover"],
    ),
];

/// Builds a [`CommandSpec`] with no icon for the static [`COMMAND_SPECS`] table.
const fn spec(
    id: CommandId,
    label: &'static str,
    shortcut: Option<&'static str>,
    category: CommandCategory,
    target: CommandTargetKind,
    danger: CommandDanger,
    aliases: &'static [&'static str],
) -> CommandSpec {
    CommandSpec {
        id,
        label,
        icon: None,
        shortcut,
        category,
        target,
        danger,
        aliases,
    }
}

/// Commands available on the library surface.
pub fn library_commands(app: &PDFolioApp) -> Vec<ResolvedCommand> {
    COMMAND_SPECS
        .iter()
        .copied()
        .map(|spec| ResolvedCommand {
            spec,
            enabled: command_enabled(app, spec.id),
            visible: command_visible(app, spec.id, CommandSurface::CommandPalette),
        })
        .collect()
}

/// Whether a command is enabled in the current app state.
pub fn command_enabled(app: &PDFolioApp, id: CommandId) -> bool {
    if is_shared_command(id) {
        return true;
    }

    if is_viewer_command(id) {
        return app.mode == AppMode::Viewer && app.viewer.doc.is_some();
    }

    if app.mode != AppMode::Library {
        return false;
    }

    let has_selection = !app.library.selected_library_entries.is_empty();
    let single_selection = app.library.selected_library_entries.len() == 1;
    let has_folder =
        app.library.details_folder_id.is_some() || app.library.selected_folder.is_some();
    let has_active_folder = app.library.selected_folder.is_some();
    let has_tag = app.library.active_tag_filter.is_some();
    let has_bulk_tag = has_selection && !app.library.bulk_tag_input.trim().is_empty();
    match id {
        CommandId::ImportPdf
        | CommandId::ImportFolder
        | CommandId::ImportRaindrop
        | CommandId::RefreshLibrary
        | CommandId::GoToLibraryRoot
        | CommandId::GoToTrash
        | CommandId::SelectAllVisible
        | CommandId::ToggleLibraryLayout
        | CommandId::ToggleLibrarySidebar
        | CommandId::ToggleLibraryInspector
        | CommandId::SortManual
        | CommandId::SortTitleAsc
        | CommandId::SortTitleDesc
        | CommandId::SortAuthorAsc
        | CommandId::SortAuthorDesc
        | CommandId::SortRecentlyAdded
        | CommandId::SortRecentlyOpened
        | CommandId::SortReadingProgress
        | CommandId::SortPageCount
        | CommandId::SortMissingFiles
        | CommandId::SetMetadataDensity
        | CommandId::ToggleMissingFiles => true,
        CommandId::UndoLibraryAction => app.library.history.can_undo(),
        CommandId::RedoLibraryAction => app.library.history.can_redo(),
        CommandId::CutLibrarySelection | CommandId::CopyLibrarySelection => {
            app.can_cut_or_copy_library_selection()
        }
        CommandId::PasteLibraryClipboard => app.can_paste_library_clipboard(),
        CommandId::RestoreTagPillView => app.library.previous_tag_pill_view.is_some(),
        CommandId::ClearFilters => {
            app.library.trash_view_active
                || app.library.selected_folder.is_some()
                || app.library.active_tag_filter.is_some()
                || app.library.active_reading_filter.is_some()
                || app.library.active_recently_opened_filter
                || app.library.missing_filter_active
                || !app.library.search_query.trim().is_empty()
        }
        CommandId::ClearSelection
        | CommandId::MoveSelectionToFolder
        | CommandId::MoveSelectionToTrash
        | CommandId::RefreshMetadata
        | CommandId::ResetDisplayMetadata
        | CommandId::ApplyTitleSortCleanup
        | CommandId::RebuildThumbnails
        | CommandId::ReindexFullText
        | CommandId::AddTagToSelection => has_selection,
        CommandId::AddTypedTag | CommandId::RemoveTypedTag => has_bulk_tag,
        CommandId::AddSelectionToCurrentFolder | CommandId::RemoveSelectionFromCurrentFolder => {
            has_selection && has_active_folder
        }
        CommandId::OpenSelected | CommandId::SaveDetails | CommandId::ResetDetails => {
            single_selection
        }
        CommandId::CreateFolder => !app.library.trash_view_active,
        CommandId::RenameFolder
        | CommandId::MoveFolderTo
        | CommandId::MoveFolderToRoot
        | CommandId::MoveFolderUp
        | CommandId::MoveFolderEarlier
        | CommandId::MoveFolderLater
        | CommandId::MoveFolderToTrash => has_folder,
        CommandId::RenameTag | CommandId::DeleteTag => has_tag,
        CommandId::MergeTag
        | CommandId::GoToRecentlyAdded
        | CommandId::GoToRecentlyOpened
        | CommandId::GoToUnfiled
        | CommandId::GoToFolder
        | CommandId::GoToTag => false,
        CommandId::ExportSelectedPdfs => has_selection,
        CommandId::BackToLibrary
        | CommandId::OpenFile
        | CommandId::ToggleTheme
        | CommandId::ReloadStyles
        | CommandId::ToggleToc
        | CommandId::JumpToPage
        | CommandId::FindInDocument
        | CommandId::ZoomIn
        | CommandId::ZoomOut
        | CommandId::ResetZoom
        | CommandId::SetViewerScrollPage
        | CommandId::SetViewerScrollVertical
        | CommandId::SetViewerScrollHorizontal
        | CommandId::SetViewerScrollWrapped
        | CommandId::SetViewerSpreadNone
        | CommandId::SetViewerSpreadOdd
        | CommandId::SetViewerSpreadEven => false,
    }
}

/// Whether a command should appear in the current context.
pub fn command_visible(app: &PDFolioApp, id: CommandId, surface: CommandSurface) -> bool {
    match surface {
        CommandSurface::ImportMenu => {
            matches!(
                id,
                CommandId::ImportPdf | CommandId::ImportFolder | CommandId::ImportRaindrop
            ) && app.mode == AppMode::Library
        }
        CommandSurface::HeaderMore => {
            matches!(
                id,
                CommandId::RefreshLibrary
                    | CommandId::SelectAllVisible
                    | CommandId::ClearSelection
                    | CommandId::ClearFilters
                    | CommandId::RebuildThumbnails
                    | CommandId::ReindexFullText
                    | CommandId::ResetDisplayMetadata
                    | CommandId::ApplyTitleSortCleanup
                    | CommandId::ToggleMissingFiles
            ) && command_enabled(app, id)
        }
        CommandSurface::SelectionToolbar => {
            matches!(
                id,
                CommandId::MoveSelectionToFolder
                    | CommandId::RefreshMetadata
                    | CommandId::RebuildThumbnails
                    | CommandId::ReindexFullText
                    | CommandId::MoveSelectionToTrash
                    | CommandId::ClearSelection
            ) && command_enabled(app, id)
        }
        CommandSurface::CommandPalette => {
            !matches!(
                id,
                CommandId::MergeTag
                    | CommandId::SetMetadataDensity
                    | CommandId::GoToRecentlyAdded
                    | CommandId::GoToUnfiled
                    | CommandId::GoToFolder
                    | CommandId::GoToTag
            ) && command_enabled(app, id)
        }
    }
}

/// Commands that stay available regardless of library/viewer mode.
fn is_shared_command(id: CommandId) -> bool {
    matches!(
        id,
        CommandId::OpenFile | CommandId::ToggleTheme | CommandId::ReloadStyles
    )
}

/// Commands that require viewer mode with an open document to be enabled.
fn is_viewer_command(id: CommandId) -> bool {
    matches!(
        id,
        CommandId::BackToLibrary
            | CommandId::ToggleToc
            | CommandId::JumpToPage
            | CommandId::FindInDocument
            | CommandId::ZoomIn
            | CommandId::ZoomOut
            | CommandId::ResetZoom
            | CommandId::SetViewerScrollPage
            | CommandId::SetViewerScrollVertical
            | CommandId::SetViewerScrollHorizontal
            | CommandId::SetViewerScrollWrapped
            | CommandId::SetViewerSpreadNone
            | CommandId::SetViewerSpreadOdd
            | CommandId::SetViewerSpreadEven
    )
}

/// Message emitted when the command is invoked.
pub fn command_message(app: &PDFolioApp, id: CommandId) -> Option<Message> {
    Some(match id {
        CommandId::OpenFile => Message::OpenFileDialog,
        CommandId::ImportPdf => Message::ImportPdfDialog,
        CommandId::ImportFolder => Message::ImportFolderDialog,
        CommandId::ImportRaindrop => Message::ImportRaindrop,
        CommandId::BackToLibrary => Message::BackToLibrary,
        CommandId::RefreshLibrary => Message::LibraryRefresh,
        CommandId::UndoLibraryAction => Message::UndoLibraryAction,
        CommandId::RedoLibraryAction => Message::RedoLibraryAction,
        CommandId::CutLibrarySelection => Message::CutLibrarySelection,
        CommandId::CopyLibrarySelection => Message::CopyLibrarySelection,
        CommandId::PasteLibraryClipboard => Message::PasteLibraryClipboard,
        CommandId::RestoreTagPillView => Message::RestoreLibraryViewBeforeTag,
        CommandId::GoToLibraryRoot => Message::FolderSelected(None),
        CommandId::GoToTrash => Message::OpenTrashCan,
        CommandId::ClearFilters => Message::ClearLibraryFilters,
        CommandId::SelectAllVisible => Message::SelectAllVisibleLibraryEntries,
        CommandId::ClearSelection => Message::ClearLibrarySelection,
        CommandId::OpenSelected => {
            let entry_id = app.library.selected_library_entries.iter().next()?.clone();
            Message::OpenLibraryEntry(entry_id)
        }
        CommandId::MoveSelectionToFolder => Message::OpenMoveSelectionDialog,
        CommandId::MoveSelectionToTrash => {
            Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)
        }
        CommandId::AddTypedTag => Message::BulkAddTag,
        CommandId::RemoveTypedTag => Message::BulkRemoveTag,
        CommandId::AddSelectionToCurrentFolder => Message::BulkAddToCurrentFolder,
        CommandId::RemoveSelectionFromCurrentFolder => Message::BulkRemoveFromCurrentFolder,
        CommandId::CreateFolder => Message::OpenCreateFolderDialog,
        CommandId::RenameFolder => Message::RenameSelectedFolder,
        CommandId::MoveFolderTo => Message::OpenMoveSelectedFolderDialog,
        CommandId::MoveFolderToRoot => Message::MoveSelectedFolderToRoot,
        CommandId::MoveFolderUp => Message::MoveSelectedFolderUp,
        CommandId::MoveFolderEarlier => Message::MoveSelectedFolderEarlier,
        CommandId::MoveFolderLater => Message::MoveSelectedFolderLater,
        CommandId::MoveFolderToTrash => Message::RequestDeleteSelectedFolder,
        CommandId::RenameTag => {
            Message::StartTagRename(app.library.active_tag_filter.as_ref()?.clone())
        }
        CommandId::DeleteTag => Message::RequestConfirmation(ConfirmationAction::DeleteTag(
            app.library.active_tag_filter.as_ref()?.clone(),
        )),
        CommandId::SaveDetails => Message::SaveDetailsMetadata,
        CommandId::ResetDetails => {
            let entry_id = app.library.details_entry_id.clone()?;
            Message::RequestConfirmation(ConfirmationAction::ResetDetailsMetadata(entry_id))
        }
        CommandId::RefreshMetadata => Message::BulkRefreshPdfMetadata,
        CommandId::ResetDisplayMetadata => {
            Message::RequestConfirmation(ConfirmationAction::BulkResetDisplayMetadata)
        }
        CommandId::ApplyTitleSortCleanup => Message::BulkApplyTitleSortCleanup,
        CommandId::RebuildThumbnails => Message::BulkRebuildThumbnails,
        CommandId::ReindexFullText => Message::BulkReindex,
        CommandId::ToggleLibraryLayout => Message::ToggleViewMode,
        CommandId::ToggleLibrarySidebar => Message::ToggleLibrarySidebar,
        CommandId::ToggleLibraryInspector => Message::ToggleLibraryInspector,
        CommandId::SortManual => Message::LibrarySortChanged(LibrarySortMode::Manual),
        CommandId::SortTitleAsc => Message::LibrarySortChanged(LibrarySortMode::TitleAsc),
        CommandId::SortTitleDesc => Message::LibrarySortChanged(LibrarySortMode::TitleDesc),
        CommandId::SortAuthorAsc => Message::LibrarySortChanged(LibrarySortMode::AuthorAsc),
        CommandId::SortAuthorDesc => Message::LibrarySortChanged(LibrarySortMode::AuthorDesc),
        CommandId::SortRecentlyAdded => Message::LibrarySortChanged(LibrarySortMode::RecentlyAdded),
        CommandId::SortRecentlyOpened => {
            Message::LibrarySortChanged(LibrarySortMode::RecentlyOpened)
        }
        CommandId::SortReadingProgress => {
            Message::LibrarySortChanged(LibrarySortMode::ReadingProgress)
        }
        CommandId::SortPageCount => Message::LibrarySortChanged(LibrarySortMode::PageCount),
        CommandId::SortMissingFiles => Message::LibrarySortChanged(LibrarySortMode::MissingFiles),
        CommandId::ToggleMissingFiles => {
            Message::MissingFilterChanged(!app.library.missing_filter_active)
        }
        CommandId::GoToRecentlyOpened => Message::RecentlyOpenedFilterChanged(true),
        CommandId::AddTagToSelection => Message::BulkAddTag,
        CommandId::ExportSelectedPdfs => Message::OpenExportDialog(ExportSource::SelectedEntries),
        CommandId::ToggleTheme => Message::ThemeToggled,
        CommandId::ReloadStyles => Message::ReloadStyles,
        CommandId::ToggleToc => Message::ToggleSidebar,
        CommandId::JumpToPage => Message::OpenJumpDialog,
        CommandId::FindInDocument => Message::OpenViewerFind,
        CommandId::ZoomIn => Message::ZoomIn,
        CommandId::ZoomOut => Message::ZoomOut,
        CommandId::ResetZoom => Message::ZoomPresetSelected(ZoomPreset::Automatic),
        CommandId::SetViewerScrollPage => Message::ViewerScrollModeSelected(ViewerScrollMode::Page),
        CommandId::SetViewerScrollVertical => {
            Message::ViewerScrollModeSelected(ViewerScrollMode::Vertical)
        }
        CommandId::SetViewerScrollHorizontal => {
            Message::ViewerScrollModeSelected(ViewerScrollMode::Horizontal)
        }
        CommandId::SetViewerScrollWrapped => {
            Message::ViewerScrollModeSelected(ViewerScrollMode::Wrapped)
        }
        CommandId::SetViewerSpreadNone => Message::ViewerSpreadModeSelected(ViewerSpreadMode::None),
        CommandId::SetViewerSpreadOdd => Message::ViewerSpreadModeSelected(ViewerSpreadMode::Odd),
        CommandId::SetViewerSpreadEven => Message::ViewerSpreadModeSelected(ViewerSpreadMode::Even),
        CommandId::SetMetadataDensity
        | CommandId::MergeTag
        | CommandId::GoToRecentlyAdded
        | CommandId::GoToUnfiled
        | CommandId::GoToFolder
        | CommandId::GoToTag => return None,
    })
}

/// Whether a command matches the palette query string.
pub fn command_matches(spec: CommandSpec, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let haystacks = std::iter::once(spec.label)
        .chain(std::iter::once(spec.category.label()))
        .chain(spec.aliases.iter().copied());
    haystacks
        .map(str::to_lowercase)
        .any(|value| fuzzy_contains(&value, &query))
}

/// Substring match, or subsequence match so sparse palette queries still hit.
fn fuzzy_contains(value: &str, query: &str) -> bool {
    if value.contains(query) {
        return true;
    }
    let mut chars = value.chars();
    query
        .chars()
        .all(|needle| chars.any(|candidate| candidate == needle))
}
