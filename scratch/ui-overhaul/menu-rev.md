# PDF-Folio Narrowed UX Refinement Plan

Scope this pass to **Library mode only**. PDF-Folio has both Library and Viewer modes, but this refinement should only address the library-management side for now.

Use `ui overhaul/menus.md` as the inventory of the current runtime surfaces. The implementation should be grounded in the current Rust/Iced code, especially:

- `crates/pdf-folio-ui/src/app/menu.rs` - top app menu, app-menu action routing, and app-menu dropdown rendering.
- `crates/pdf-folio-ui/src/app/context_menu.rs` - right-click menu grouping and action routing.
- `crates/pdf-folio-ui/src/app/commands.rs` - central Library command registry.
- `crates/pdf-folio-ui/src/app/messages.rs` - `Message`, `AppMenuAction`, `ContextMenuAction`, `Shortcut`, and related enums.
- `crates/pdf-folio-ui/src/app/update.rs` and `crates/pdf-folio-ui/src/app/update/shortcuts.rs` - command execution and shortcut handling.
- `crates/pdf-folio-ui/src/app/shortcuts.rs` - keyboard-event mapping. `Ctrl+K` is currently unused and should become the command palette shortcut.
- `crates/pdf-folio-ui/src/library/view.rs` - current library toolbar, breadcrumb/filter row, selection status row, content layout, and sidebar/main-content composition.
- `crates/pdf-folio-ui/src/library/view/sidebar.rs` - current left sidebar, folder/tag navigation, selected PDF details, multi-selection details, and folder details.
- `crates/pdf-folio-ui/src/library/view/dialogs.rs` - create folder, move picker, confirmations, and Raindrop import dialogs.
- `crates/pdf-folio-ui/src/app.rs` - `PDFolioApp`, `LibraryRuntime`, and `ChromeRuntime` state fields.
- `crates/pdf-folio-ui/src/app/library_selection.rs` - selection helpers such as `primary_selected_entry`, `selected_entries`, `clear_library_selection`, `select_folder_for_details`, and metadata editor syncing.
- `crates/pdf-folio-ui/src/library/tasks.rs` - existing import, metadata, thumbnail, reindex, tag, and folder tasks.
- `crates/pdf-folio-style/src/*` and `crates/pdf-folio-style/styles/**/*.kdl` - classes, layout tokens, labels, and theme styling.

Do **not** work on:

- Missing-file recovery, except preserving existing `RelinkMissingEntry` behavior where it already exists.
- Table view.
- Viewer header/sidebar/viewer UX.
- Annotation/note systems.
- Cloud sync.
- Complex library health dashboards.
- Major visual theming overhaul.

The goal is to make the library UI feel modern, usable, and full-featured by fixing action placement, selection behavior, metadata editing, tags, import, and export.

---

# Core Rule

For every action, ask:

> What object is this action acting on?

Then place the action near that object.

Use this mapping:

| Target | Location |
| --- | --- |
| Whole library | Library header, Import button, command palette |
| Current folder | Folder header/inspector, folder context menu |
| Current tag | Tag sidebar row/menu, tag inspector, tag manager |
| One selected PDF | Selection toolbar, right inspector, PDF context menu |
| Multiple selected PDFs | Selection toolbar, bulk inspector |
| Rare command | Command palette |
| Dangerous action | Contextual menu/inspector, visually separated |

Keep viewer commands in the existing top menu and viewer surfaces. This plan only demotes the top menu as the primary Library-mode interaction surface.

---

# Original Code Shape To Preserve

At the start of this refinement, the app had:

- A permanent app menu bar with `File`, `Edit`, `View`, `Document`, `Library`, `Tools`, and `Help`.
- Library toolbar controls in `view_library`: search, undo/redo, sort picker, grid/list toggle, metadata density picker, viewer return, and new folder.
- A breadcrumb/filter row and a selection status row in `library/view.rs`.
- A selected-object toolbar under the app menu in `app/menu/selection.rs`; this has since been removed.
- A left sidebar that currently changes modes: navigation, selected PDF details, multi-selection details, or folder details.
- Context menus that duplicate command routing in `app/context_menu.rs`.
- Selection and metadata runtime fields on `LibraryRuntime`: `selected_library_entries`, `details_entry_id`, `details_title_input`, `details_author_input`, `bulk_tag_input`, `details_folder_id`, and `folder_details_sidebar_open`.
- Existing message routes for most non-export commands.

Refactor these pieces rather than creating a parallel UI system.

---

# Phase 1 - Create A Central Command Registry

## Goal

Stop duplicating command definitions and enabled logic across the app menu, selection toolbar, context menus, header controls, inspector, and future command palette.

## Current duplication

Command concepts are currently spread across:

- `AppMenuAction` plus `app_menu_action_message` in `app/menu.rs`.
- `ContextMenuAction` plus `context_menu_action_message` and `ContextMenuItemSpec` in `app/context_menu.rs`.
- Legacy app-menu-attached selection toolbar definitions previously lived in `app/menu/selection.rs`; this file has been removed.
- Direct `Message` values in `library/view.rs` toolbar controls.
- Direct shortcut-to-message logic in `app/update/shortcuts.rs`.

## Tasks

Create a new command module, preferably:

```text
crates/pdf-folio-ui/src/app/commands.rs
```

Wire it from `crates/pdf-folio-ui/src/app.rs` with `#[path = "app/commands.rs"] mod app_commands;`.

Define a stable command model:

```rust
pub enum CommandId { ... }
pub enum CommandCategory { Library, Import, Selection, Folder, Tag, Metadata, Export, Maintenance, Navigation, View }
pub enum CommandTargetKind { None, Library, Folder, Tag, SinglePdf, MultiplePdfs, SearchResult }
pub enum CommandDanger { Safe, Destructive, OverwritesMetadata }

pub struct CommandSpec {
    pub id: CommandId,
    pub label: &'static str,
    pub icon: Option<&'static [u8]>,
    pub shortcut: Option<&'static str>,
    pub category: CommandCategory,
    pub target: CommandTargetKind,
    pub danger: CommandDanger,
}
```

Add helpers that evaluate commands against `&PDFolioApp`:

```rust
pub fn library_commands(app: &PDFolioApp) -> Vec<ResolvedCommand>;
pub fn command_enabled(app: &PDFolioApp, id: CommandId) -> bool;
pub fn command_visible(app: &PDFolioApp, id: CommandId, surface: CommandSurface) -> bool;
pub fn command_message(app: &PDFolioApp, id: CommandId) -> Option<Message>;
```

Start by registering existing commands and mapping to existing messages instead of rewriting task execution. Initial `CommandId` values should cover all current library items:

- Import/Open: `OpenFile`, `ImportPdf`, `ImportFolder`, `ImportRaindrop`.
- Library navigation/state: `RefreshLibrary`, `RestoreTagPillView`, `GoToLibraryRoot`, `GoToTrash`, `ClearFilters`.
- Selection: `SelectAllVisible`, `ClearSelection`, `OpenSelected`, `MoveSelectionToFolder`, `MoveSelectionToTrash`.
- Folder: `CreateFolder`, `RenameFolder`, `MoveFolderTo`, `MoveFolderToRoot`, `MoveFolderUp`, `MoveFolderEarlier`, `MoveFolderLater`, `MoveFolderToTrash`.
- Tag: `RenameTag`, `DeleteTag`, later `MergeTag`.
- Metadata: `SaveDetails`, `ResetDetails`, `RefreshMetadata`, `ResetDisplayMetadata`, `ApplyTitleSortCleanup`.
- Maintenance: `RebuildThumbnails`, `ReindexFullText`.
- View: `ToggleLibraryLayout`, `SetLibrarySort`, `SetMetadataDensity`, `ToggleMissingFiles`.

Keep `AppMenuAction` and `ContextMenuAction` temporarily if that reduces churn, but route their messages through the command registry wherever possible.

## Acceptance criteria

- Existing actions still work.
- Enabled/disabled logic comes from `commands.rs` for newly touched Library-mode actions.
- App menu, context menu, selection toolbar, and future command palette can use the same `CommandId`.
- Viewer commands can remain in the old app-menu path for now.
- No major UX change is required yet.

---

# Phase 2 - Replace The Top Menu As The Primary Interaction Surface

## Goal

The old permanent menu bar should no longer be the main way to use Library mode. Keep its commands available temporarily, but move everyday Library-mode use into the library header, contextual menus, inspector, and command palette.

## Original implementation

The original library toolbar was built at the top of `view_library` in `crates/pdf-folio-ui/src/library/view.rs`. It included search, undo/redo, sort, grid/list, metadata density, viewer return, and new folder. It did **not** yet show a view title/breadcrumb as the primary header title, an Import button, a More menu, or a complete action grouping.

The old top app menu is rendered by `view_app_menu_bar` in `app/menu.rs`. The selection context toolbar used to be attached under that menu by `library_context_toolbar_visible` and `view_selection_context_row`; that legacy selection toolbar has since been removed.

## Tasks

Refactor the `view_library` header into explicit functions:

```rust
fn view_library_header(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message>
fn library_header_title(app: &PDFolioApp) -> String
fn view_library_import_menu(...)
fn view_library_more_menu(...)
```

The normal header should become:

```text
[Current view title / breadcrumb]      [Search] [Filter] [Sort] [View] [Density] [Import] [...]
```

Examples:

```text
All PDFs                                  Search library...   Sort: Recently Added   Grid   Import   ...
Statistics / Estimation                   Search folder...    Sort: Manual           Grid   Import   ...
Tag: admissibility                        Search tag...       Sort: Title A-Z        Grid   ...
Trash Can                                 Search trash...     Sort: Recently Added   List   ...
```

Use current state to derive the title:

- `app.library.trash_view_active` -> `Trash Can`.
- `app.library.active_tag_filter` -> `Tag: {tag}`.
- `app.folder_breadcrumbs()` or `app.library.selected_folder` -> folder breadcrumb/title.
- Default -> `All PDFs` or active library name if clearer.

Preserve existing controls by reusing:

- `library_search_input`.
- `component_library_sort_picker`.
- `component_library_layout_toggle_button`.
- `component_library_metadata_density_picker`.
- `library_history_icon_button`.
- `library_new_folder_button` until folder creation moves beside the folder section.

Add an Import button/menu that routes through existing messages:

- `Message::ImportPdfDialog`.
- `Message::ImportFolderDialog`.
- `Message::ImportRaindrop`.

Add a Library More menu backed by the command registry. Start with existing messages:

- `Refresh Library` -> `Message::LibraryRefresh`.
- `Select All Visible PDFs` -> `Message::SelectAllVisibleLibraryEntries`.
- `Clear Selection` -> `Message::ClearLibrarySelection`.
- `Rebuild Thumbnails` -> `Message::BulkRebuildThumbnails` when a selection exists.
- `Reindex Full Text` -> `Message::BulkReindex` when a selection exists.
- `Reset Display Metadata` -> `Message::RequestConfirmation(ConfirmationAction::BulkResetDisplayMetadata)` when a selection exists.
- `Apply Title Sort Cleanup` -> `Message::BulkApplyTitleSortCleanup` when a selection exists.
- `Reload Styles` can remain hidden behind the app menu unless this is treated as a development-only command.

Do not show irrelevant disabled commands in the header More menu. Use command `visible` logic to hide them.

## Acceptance criteria

- Common library actions are available without the old top menu.
- `view_library` has a clear header function rather than one large toolbar block.
- The old top menu can remain temporarily, but Library mode should be usable from the new header and contextual surfaces.
- Commands irrelevant to the current context are hidden from contextual menus.

---

# Phase 3 - Add A Command Palette

## Goal

Keep the app powerful without exposing every command in permanent UI.

## Shortcut

Use:

```text
Ctrl+K
Cmd+K where applicable
```

Implementation note: `app/shortcuts.rs` currently handles `Ctrl+F`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V`, `Ctrl+Z`, `Ctrl+Y`, `Ctrl+G`, `Ctrl+A`, Delete, Enter, F2, Escape, and viewer zoom/scroll keys. Add a new `Shortcut::OpenCommandPalette`, map `Ctrl+K` in `keyboard_event_message`, and handle it in `app/update/shortcuts.rs`.

## Tasks

Add palette state to `ChromeRuntime` or `LibraryRuntime` in `app.rs`, such as:

```rust
pub command_palette_open: bool,
pub command_palette_query: String,
pub command_palette_selected_index: usize,
```

Add messages in `messages.rs`:

```rust
OpenCommandPalette
CloseCommandPalette
CommandPaletteQueryChanged(String)
CommandPaletteMoveSelection(i32)
CommandPaletteRunSelected
CommandPaletteRun(CommandId)
```

If `CommandId` lives in `commands.rs`, make sure it is public and imported by `messages.rs`.

Render the palette in the same overlay stack that currently renders app menus, context menus, dialogs, and viewer overlays. The overlay composition is in `crates/pdf-folio-ui/src/app/view.rs`.

Minimum commands:

```text
Import PDFs
Import Folder
Import from Raindrop
New Folder
Go to All PDFs
Go to Recently Added
Go to Recently Opened
Go to Unfiled
Go to Trash
Go to Folder...
Go to Tag...
Add Tag to Selection
Move Selection to Folder
Export Selected PDFs
Refresh Metadata
Rebuild Thumbnails
Reindex Full Text
Toggle Grid/List
Clear Selection
```

Only include commands that can be represented by current state/messages during the first pass. For commands that require new views like Unfiled, Recently Opened, or Export, add `CommandId` values but keep them hidden until the feature exists.

Palette behavior:

- Fuzzy search over command label, category, and optional aliases.
- Keyboard navigation with Up/Down and Enter.
- Show shortcut text on the right.
- Hide commands that cannot apply.
- Optional: show disabled command with reason only when helpful.

## Acceptance criteria

- `Ctrl+K` opens the palette.
- A keyboard-focused user can perform major existing Library-mode actions without the old top menu.
- Rare commands are discoverable without cluttering the main UI.
- Palette execution uses the same command registry as menu/header/context surfaces.

---

# Phase 4 - Stabilize The Left Sidebar

## Goal

The left sidebar should be navigation only.

## Current implementation

`view_library_tag_sidebar` in `library/view/sidebar.rs` currently chooses between:

- `view_selected_pdf_sidebar`.
- `view_multi_selection_sidebar`.
- `view_selected_folder_sidebar`.
- `view_library_navigation_sidebar`.

This means selecting a PDF or folder replaces folder/tag navigation. This phase should stop that.

## Tasks

Change `view_library_tag_sidebar` so it always renders navigation:

```rust
let sidebar_body = view_library_navigation_sidebar(app, sidebar_width, tokens);
```

Do not delete the existing detail-rendering functions yet. Move or reuse them for the right inspector in Phase 5.

Restructure navigation into stable sections:

```text
Library
  All PDFs
  Recently Added
  Recently Opened
  Continue Reading
  Unfiled
  Trash

Folders
  Folder tree

Tags
  Tag list

Collections
  Saved searches, later if implemented
```

The current sidebar has Files/Tags tabs via `LibrarySidebarTab`. Either:

- Keep the tabs for the first implementation and make them navigation-only, or
- Replace them with stacked sections if that can be done without a large styling rewrite.

Preserve:

- Collapsible sidebar: `CollapseLibrarySidebar` / `ExpandLibrarySidebar`.
- Resizable sidebar: `BeginTagSidebarResize`, `TagSidebarResizeDragged`, `EndTagSidebarResize`.
- Folder tree expand/collapse: `ToggleLibraryTreeRoot`, `ToggleLibraryTreeFolder`.
- Tag filtering: `TagFilterChanged`, `TagTreeClicked`.
- Trash: `OpenTrashCan`.
- Library switcher: `OpenLibrarySwitcher`.

Remove from left sidebar:

- Selected PDF details.
- Multi-selection details.
- Full folder details panel.
- Bulk edit controls.

Add small contextual affordances only where appropriate:

- `+` beside Folders -> `Message::OpenCreateFolderDialog`.
- `...` on folder rows -> open `ContextMenuTarget::Folder(Some(folder.id))`.
- `...` on tag rows -> open `ContextMenuTarget::Tag(tag)`.

## Acceptance criteria

- Selecting a PDF does not replace the sidebar.
- Selecting multiple PDFs does not replace the sidebar.
- Selecting a folder does not replace the sidebar.
- Folder and tag navigation remain available during selection.
- Sidebar behavior feels stable and predictable.

---

# Phase 5 - Add The Right Inspector

## Goal

Move metadata and object-specific actions into a right-side inspector.

## Current implementation

The app already has most inspector content, but it is incorrectly rendered in the left sidebar:

- `view_selected_pdf_sidebar`.
- `view_multi_selection_sidebar`.
- `view_selected_folder_sidebar`.
- `selected_folder_actions_panel`.
- Metadata inputs currently also appear in the selection toolbar under the top menu.

## Tasks

Create a new module:

```text
crates/pdf-folio-ui/src/library/view/inspector.rs
```

Wire it from `library/view.rs` beside `dialogs`, `entries`, `folders`, and `sidebar`.

Add inspector composition to the end of `view_library`:

```rust
let mut layout = row![].height(Length::Fill);
if app.library.library_tag_sidebar_open {
    layout = layout.push(view_library_tag_sidebar(app));
}
layout = layout.push(main_content);
if library_inspector_visible(app) {
    layout = layout.push(view_library_inspector(app));
}
```

Add width/layout tokens to `pdf-folio-style` rather than hard-coding everything. A first pass can use `app.layout().metric("LibraryInspector", "width", 320.0)`.

Inspector states:

```text
Nothing selected
One PDF selected
Multiple PDFs selected
Folder selected
Tag selected
```

Map those states to current fields:

- One PDF: `app.primary_selected_entry()` / `app.library.details_entry_id`.
- Multiple PDFs: `!selected_library_entries.is_empty()` and count > 1.
- Folder: `app.details_folder()` or `app.library.details_folder_id`.
- Tag: `app.library.active_tag_filter` or a new explicit inspector tag field if tag filtering should not always imply tag inspection.
- Nothing: no PDF selection, no folder details, no tag inspector target.

### Nothing selected

Show a lightweight library summary based on `app.library.library_entries`, `library_folders`, active library name, and existing counts:

```text
Library Summary
565 PDFs
Recently added
Recently opened
Untagged PDFs
Unfiled PDFs
```

Include quick actions:

```text
Import PDFs
Create Folder
Open Tag Manager
```

Do not include missing-file recovery in this pass.

### One PDF selected

Reuse and move the content from `view_selected_pdf_sidebar`, then extend it. Show:

```text
Thumbnail
Title
Author
Tags
Folder
Reading status
Progress
Pages
File size
Added date
Opened date
File path
```

Actions:

```text
Open PDF
Export PDF
Reveal in File Manager
Copy File Path
Move to Folder
Refresh Metadata
Rebuild Thumbnail
Reindex Full Text
Move to Trash
```

Use existing messages where available:

- `OpenLibraryEntry(entry.id.clone())`.
- `RevealEntryInFileManager(entry.id.clone())`.
- `OpenEntryContainingFolder(entry.id.clone())`.
- `OpenMoveSelectionDialog`.
- `BulkRefreshPdfMetadata`.
- `BulkRebuildThumbnails`.
- `BulkReindex`.
- `RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)`.

Add new messages only where missing, such as `CopyEntryFilePath(EntryId)` and export.

Metadata fields should be editable directly in the inspector using existing fields:

- `details_title_input` with `DetailsTitleChanged`.
- `details_author_input` with `DetailsAuthorChanged`.
- `SaveDetailsMetadata`.
- `ResetDetailsMetadata(entry_id)`.

Keep `sync_details_editor_to_selection` in `app/library_selection.rs` as the source for editor state synchronization.

### Multiple PDFs selected

Reuse `view_multi_selection_sidebar` as the starting point, then add visible bulk workflows:

```text
3 PDFs selected
Common tags
Mixed tags
Folder summary
Reading status summary
```

Actions:

```text
Add Tags
Remove Tags
Move to Folder
Export Selected
Set Reading Status
Refresh Metadata
Rebuild Thumbnails
Reindex Full Text
Move to Trash
```

Existing bulk messages:

- `BulkTagInputChanged`, `BulkAddTag`, `BulkRemoveTag`.
- `OpenMoveSelectionDialog`.
- `BulkRefreshPdfMetadata`.
- `BulkRebuildThumbnails`.
- `BulkReindex`.
- `RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)`.

### Folder selected

Move `view_selected_folder_sidebar` and `selected_folder_actions_panel` into the inspector. Show:

```text
Folder name
PDF count
Child folder count
Rename
Export Folder
Move Folder
Move to Trash
```

Use existing messages:

- `FolderSelected(Some(folder.id.clone()))`.
- `FolderRenameInputChanged`, `RenameSelectedFolder`.
- `OpenMoveSelectedFolderDialog`.
- `MoveSelectedFolderToRoot`.
- `MoveSelectedFolderUp`.
- `MoveSelectedFolderEarlier`.
- `MoveSelectedFolderLater`.
- `RequestDeleteSelectedFolder`.

### Tag selected

Add a new inspector state for the active tag. Use `app.all_tags()` and `library_entries` to compute counts. Show:

```text
Tag name
PDF count
Rename
Merge
Delete
Export Tagged PDFs
```

Use existing messages:

- `StartTagRename(tag)`.
- `RequestConfirmation(ConfirmationAction::DeleteTag(tag))`.
- `TagFilterChanged(Some(tag))`.

Add merge/export messages in later phases.

## Acceptance criteria

- Metadata editing is done in the right inspector.
- Bulk editing is done in the right inspector.
- Object-specific actions are near the selected object.
- The left sidebar remains navigation-focused.
- The existing details functions are moved/reused, not duplicated wholesale.

---

# Phase 6 - Replace The Selection Row With A Selection Toolbar

## Goal

Make selection feel like a first-class mode.

## Original implementation

There were two selection surfaces:

- `view_library_selection_status_row` in `library/view.rs`, which shows checkbox/count/status/reorder hint in the breadcrumb row.
- `view_selection_context_row` in `app/menu/selection.rs`, which appeared under the top app menu and included title/author fields for single selection, bulk tag input, dropdown menus, and trash.

This phase should consolidate selection into the Library header area, not under the old app menu.

## Tasks

Stop using `library_context_toolbar_visible(app)` to add the selection toolbar under `view_app_menu_bar` for Library mode. Keep the top app menu visually stable.

Move the useful pieces of `view_selection_context_row` into `library/view.rs` or a new:

```text
crates/pdf-folio-ui/src/library/view/selection_toolbar.rs
```

Normal header:

```text
[View title] [Search] [Sort] [View] [Density] [Import] [...]
```

Selection toolbar:

```text
[checkbox] 3 selected     Add Tag   Move   Export   Mark As   More ...   Trash   Clear
```

Required actions:

```text
Add Tag
Move to Folder
Export
Set Reading Status
Refresh Metadata
Rebuild Thumbnails
Reindex Full Text
Move to Trash
Clear Selection
```

Use current messages for:

- Master checkbox -> `MasterCheckboxClicked`.
- Clear -> `ClearLibrarySelection`.
- Add Tag -> inspector chip editor or current `BulkAddTag` during transition.
- Move -> `OpenMoveSelectionDialog`.
- Refresh Metadata -> `BulkRefreshPdfMetadata`.
- Rebuild Thumbnails -> `BulkRebuildThumbnails`.
- Reindex -> `BulkReindex`.
- Trash -> `RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)`.

Keep trash-specific restore/permanent-delete behavior from `view_selection_context_row` when `trash_view_active` is true.

Keyboard behavior should continue to be handled in `app/update/shortcuts.rs`:

```text
Esc        Clear selection
Delete     Move selected PDFs to Trash
Ctrl+A     Select all visible PDFs
Enter      Open selected PDF if exactly one selected
```

Add or adjust tests in `crates/pdf-folio-ui/src/tests/app.rs` for selection state and toolbar behavior where practical.

## Acceptance criteria

- Multi-select actions are visible without right-clicking.
- Common bulk actions are one click away.
- Rare bulk actions are in `More ...`.
- Selection can be cleared easily.
- Selection UI no longer depends on being attached to the old top app menu.

---

# Phase 7 - Redesign Tag Editing

## Goal

Make tags fast to add, remove, and clean up.

## Current implementation

Tags are currently edited through:

- Inline card/row tag entry: `StartTagEntry`, `TagInputChanged`, `SubmitTag`, `EntryTagged`, `EntryUntagged`.
- Sidebar tag rename: `StartTagRename`, `TagRenameInputChanged`, `SubmitTagRename`, `CancelTagRename`.
- Bulk typed tag input: `bulk_tag_input`, `BulkTagInputChanged`, `BulkAddTag`, `BulkRemoveTag`.
- Tag context menu: `RenameTag`, `DeleteTag`.

The hidden typed commands should be replaced by visible chip editors in the right inspector.

## Single PDF tag editor

In the right inspector:

```text
Tags: [statistics x] [estimation x] [graybill-deal x] [+ Add tag]
```

Behavior:

- Click tag field to add a tag.
- Type to search existing tags from `app.all_tags()`.
- Enter applies selected tag or creates a new tag.
- Comma-separated paste creates/applies multiple tags.
- Backspace removes previous chip when input is empty.
- `x` removes a chip.
- Recent tags appear first if recent tag tracking exists; otherwise sort by current tag frequency then alphabetically.
- Existing matching tags are shown before create-new option.

Add state to `LibraryRuntime` as needed:

```rust
pub inspector_tag_input: String,
pub inspector_tag_suggestions_open: bool,
pub inspector_tag_highlighted_index: usize,
```

Prefer replacing `bulk_tag_input` after the inspector workflow is stable, but it can coexist during migration.

## Multi-selection tag editor

Show:

```text
Common tags
[statistics x] [estimation x]

On some selected PDFs
[graybill-deal +/-] [bayes +/-]

Add tags to all
[ input... ]
```

Actions:

```text
Add to all selected
Remove from all selected
Replace all tags
Clear tags from selected
```

Confirm destructive actions such as clearing all tags or replacing all tags.

Use selected entries from `app.selected_entries()` to compute:

- Tags present on every selected PDF.
- Tags present on only some selected PDFs.
- Tag counts for suggestions.

## Tag Manager

Add a Tag Manager screen or modal. It can live in `library/view/dialogs.rs` initially, but a dedicated module is preferable once it grows:

```text
crates/pdf-folio-ui/src/library/view/tag_manager.rs
```

Features:

```text
List all tags
Show PDF count for each tag
Rename tag
Merge tags
Delete tag
Find unused tags
Find case duplicates
```

Examples of duplicate cleanup:

```text
Math
math

Graybill Deal
graybill-deal
Graybill--Deal
```

Use existing `rename_tag_task` and `delete_tag_task` from `library/tasks.rs`. Add a merge task that rewrites entries using the source tag to the destination tag and records a `LibraryHistoryAction` if the existing history system supports it.

## Acceptance criteria

- Users can add and remove tags without hidden typed commands.
- Bulk tag editing is visible and understandable.
- Tags can be renamed, merged, and deleted.
- Tag cleanup has a dedicated surface.

---

# Phase 8 - Add Export Workflows

## Goal

Export should be a normal library action.

## Current implementation

There is no library export workflow in the current menu inventory. `Message::ExportAnnotations` exists for viewer annotations and is out of scope.

## Entry points

Add export to:

```text
Selection toolbar
Single PDF inspector
Multi-selection inspector
PDF context menu
Folder context menu
Tag context menu
Command palette
```

## Implementation tasks

Add new state to `LibraryRuntime`:

```rust
pub export_dialog: Option<LibraryExportDialog>,
pub export_progress: Option<LibraryExportProgress>,
```

Add messages:

```rust
OpenExportDialog(ExportSource)
ExportDestinationSelected(PathBuf)
ExportModeChanged(ExportMode)
ExportFilenameTemplateChanged(...)
ExportMetadataOptionsChanged(...)
ExportConflictBehaviorChanged(...)
StartExport
ExportProgressUpdated(...)
ExportFinished(Result<ExportSummary, String>)
CloseExportDialog
RevealExportedFolder
CopyExportPath
```

Add task code in `library/tasks.rs` or a new `library/export.rs`. Use `std::fs` for copying and a ZIP crate only if already present or intentionally added.

## Export sources

Support:

```text
One selected PDF
Multiple selected PDFs
Current folder
Current tag
Current search/filter result, optional
```

Do not require whole-library export in this pass unless it is easy.

## Export dialog

Fields:

```text
Export source
Destination folder
Export mode
Filename template
Metadata options
Conflict behavior
```

## Export modes

```text
Copy PDFs to folder
Copy PDFs and preserve folder structure
Export as ZIP
```

## Filename templates

```text
Original filename
{title}.pdf
{author} - {title}.pdf
{year} - {author} - {title}.pdf
Custom
```

## Metadata options

```text
Include metadata.csv
Include metadata.json
Include tags
Include reading progress
```

## Conflict behavior

```text
Skip
Overwrite
Keep both
Append number
```

## Progress states

```text
Preparing
Copying
Zipping
Complete
Failed
```

## Completion actions

```text
Reveal exported folder
Copy export path
Close
```

## Acceptance criteria

- A user can export one PDF.
- A user can export selected PDFs.
- A user can export a folder.
- A user can export a tag.
- Export can optionally include metadata.
- Export shows progress and errors.

---

# Phase 9 - Improve Import Flow

## Goal

Make import feel like a managed workflow instead of a blind add operation.

## Current implementation

The app currently supports:

- Opening a standalone PDF: `OpenFileDialog` / `FileSelected`.
- Importing a folder: `ImportFolderDialog` / `ImportFolderSelected` / `ImportFinished`.
- Importing one PDF: `ImportPdfDialog` / `ImportPdfSelected` / `ImportFinished`.
- Raindrop import: `ImportRaindrop`, Raindrop preview/progress messages, and `RaindropImportFinished`.

The UI exposes these through the File/Library app menus, background context menu, and Raindrop dialogs.

## Tasks

Add one primary Import button in the new Library header.

Import menu:

```text
Import PDFs
Import Folder
Import from Raindrop
```

Routes:

- Import PDFs -> `Message::ImportPdfDialog`.
- Import Folder -> `Message::ImportFolderDialog`.
- Import from Raindrop -> `Message::ImportRaindrop`.

After import, show an Import Review screen/modal. Add state to `LibraryRuntime`, for example:

```rust
pub import_review: Option<ImportReviewState>,
```

Populate it from `ImportFinished(ImportSummary)` and `RaindropImportFinished(RaindropImportSummary)` in `app/update.rs`.

Import Review should show:

```text
Imported count
Duplicate count
Failed count
Missing metadata count
Destination folder
Suggested tags
```

Actions:

```text
Add tags to all
Move to folder
Fix metadata
Open imported set
Dismiss
```

Add or improve an `Unfiled` or `Recently Imported` view. A minimal first pass can preserve the IDs of newly imported PDFs in `ImportReviewState` and filter visible entries when the user chooses `Open imported set`.

New imports should be easy to find immediately.

## Acceptance criteria

- All import paths are available from one Import button.
- Imported PDFs can be reviewed immediately.
- User can tag and move imported PDFs before losing track of them.
- Recently imported PDFs are easy to access.

---

# Phase 10 - Clean Up Context Menus

## Goal

Keep context menus powerful but scannable.

## Current implementation

`app/context_menu.rs` builds all context menus with local `ContextMenuItemSpec` groups:

- `library_entry_context_groups`.
- `folder_context_groups`.
- `library_background_context_groups`.
- `tag_context_groups`.
- `viewer_context_groups`.

Viewer context menus are out of scope and should be left alone.

## Tasks

Rebuild Library-mode context menus from the command registry where possible. Keep `ContextMenuAction` as an adapter only if needed.

## PDF context menu

```text
Open
Reveal in File Manager
Copy File Path

Add Tag...
Move to Folder...
Export PDF

Edit Details
Refresh Metadata
Reset Display Metadata

Rebuild Thumbnail
Reindex Full Text

Move to Trash
```

Implementation notes:

- `Open` -> `OpenLibraryEntry(entry_id)`.
- `Reveal in File Manager` -> `RevealEntryInFileManager(entry_id)`.
- `Copy File Path` needs a new message/task.
- `Add Tag...` should focus/open the inspector tag editor rather than using hidden typed commands once Phase 7 lands.
- `Edit Details` should open/focus the right inspector title field.
- `Move to Trash` remains destructive and visually separated.

## Multi-selection context menu

```text
Open, if one selected only
Add Tag...
Move to Folder...
Export Selected

Refresh Metadata
Rebuild Thumbnails
Reindex Full Text

Move to Trash
Clear Selection
```

## Folder context menu

```text
Open Folder
New Folder
Rename
Move To...
Export Folder
Move to Trash
```

Use existing folder messages and add export.

## Tag context menu

```text
Show PDFs with Tag
Rename Tag
Merge Tag
Export Tagged PDFs
Delete Tag
```

Use `TagFilterChanged(Some(tag))`, `StartTagRename(tag)`, delete confirmation, and new merge/export messages.

## Acceptance criteria

- Context menus are grouped by purpose.
- Dangerous actions are visually separated.
- Context menus do not become the only way to access common actions.
- Viewer context menu behavior remains unchanged.

---

# Phase 11 - Manual Ordering UX

## Goal

Make manual ordering understandable without overbuilding it.

## Current implementation

`view_library_breadcrumb_row` shows a small reorder hint:

```rust
if app.can_drag_reorder_library() {
    "Manual reorder enabled"
} else {
    "Reordering requires unfiltered Manual sort"
}
```

Drag/reorder logic lives in `app/library_drag.rs`, `library/drag.rs`, and `library/selection.rs`.

## Tasks

When manual ordering is unavailable because filters or non-manual sort are active, show a clearer message near the grid/list content or header:

```text
Manual ordering is only available in an unfiltered folder using Manual sort.
```

Add action:

```text
Go to manual folder view
```

That action should:

- Clear search, tag, reading, and missing filters via the same logic as `ClearLibraryFilters`.
- Set sort to `LibrarySortMode::Manual`.
- Keep or navigate to the active folder if possible.

Optional but preferred:

```text
Enter Reorder Mode
Done
Cancel
```

During reorder mode:

- Show drag handles.
- Show drop indicators.
- Reduce unrelated hover actions.
- Preserve undo support through existing `LibraryHistoryAction` flows.

## Acceptance criteria

- Users understand why they cannot reorder.
- Reordering has a clear mode or clear visual affordance.
- The app does not rely only on a small hint text.

---

# Phase 12 - Final Polish

## Goal

Make the narrowed redesign feel complete.

## Current implementation

The library already has empty states for root library, empty folder, and empty Trash. It also has error banners, library status text, and bulk operation progress. This phase should expand those into consistent user feedback without building a large notification system unless needed.

## Tasks

Add empty states for:

```text
Empty library
Empty folder
Empty tag
Empty search result
Empty trash
No selected item
No tags
No folders
```

Add useful actions to empty states:

```text
Import PDFs
Create Folder
Clear Filters
Show All PDFs
Open Tag Manager
```

Add toasts or status banners for:

```text
PDF moved
Tags updated
Export complete
Export failed
Metadata refreshed
Thumbnail rebuild started
Reindex started
Import complete
```

Use existing fields first:

- `library_status`.
- `library_error`.
- `bulk_operation_progress`.
- existing confirmation dialogs.

Add undo action where safe:

```text
Undo move
Undo tag change
Undo trash
```

Connect undo actions to existing `UndoLibraryAction` and history support.

Add progress indicators for:

```text
Import
Export
Reindex
Thumbnail rebuild
Metadata refresh
Search
```

Search already has debounce/results state; use it to show subtle progress only when helpful.

## Acceptance criteria

- Empty states explain what to do next.
- Long-running actions show progress.
- Errors explain what happened.
- Common actions provide undo when feasible.

---

# Recommended Implementation Order

```text
1. Central command registry
2. Contextual library header
3. Command palette
4. Stable left sidebar
5. Right inspector
6. Selection toolbar
7. Tag chip editor
8. Multi-selection tag editor
9. Tag manager
10. Export selected PDFs
11. Export folder/tag workflows
12. Unified import button
13. Import review screen
14. Context menu cleanup
15. Manual ordering clarification
16. Empty states, toasts, and progress polish
```

---

# Definition Of Done

This narrowed refinement is complete when:

1. The old top menu is no longer the main interaction surface for Library mode.
2. The left sidebar stays stable and navigation-focused.
3. The right inspector handles PDF details, metadata editing, folder details, tag details, and bulk selection.
4. Multi-select exposes a clear selection toolbar in the Library surface.
5. Tags are edited through chips and autocomplete.
6. Bulk tag editing is obvious.
7. A Tag Manager exists for rename, merge, delete, and cleanup.
8. Export exists for selected PDFs, folders, and tags.
9. Import is unified under one button and includes a review step.
10. Context menus are grouped and scannable.
11. Manual ordering rules are clear.
12. Viewer mode is unchanged.
13. Missing-file recovery is unchanged or ignored for now.
14. No table view is introduced in this pass.

---

# Implementation Notes

## 2026-07-06 tranche

Completed and verified with `cargo check -p pdf-folio-ui`:

- Added `crates/pdf-folio-ui/src/app/commands.rs` as the central command registry.
  - `CommandId`, command metadata, visibility/enabled checks, fuzzy matching, and existing-message dispatch now live there for current Library actions.
  - Newly touched app-menu Library actions route through the registry where possible.
- Added command-palette state to `ChromeRuntime` and messages to `Message`.
  - `Ctrl+K` opens the palette in Library mode.
  - The palette renders as an overlay in `app/view.rs`, filters registry commands, shows category/target/shortcut hints, and dispatches through `CommandId`.
- Refactored the Library header in `library/view.rs`.
  - Header title is derived from active scope: Trash, tag, folder breadcrumb, or active library name.
  - Existing search, undo/redo, sort, grid/list, density, viewer, and new-folder controls are preserved.
  - Import actions are surfaced in the header through a single Import chooser.
  - Header `More` opens the command palette.
- Moved selection actions into the Library surface.
  - The old app-menu-attached selection strip is no longer rendered.
  - Selection now replaces the normal Library header with a toolbar containing master checkbox, count, move, refresh metadata, More, trash/restore/delete, and clear.
- Stabilized the left sidebar.
  - `view_library_tag_sidebar` now always renders navigation plus the library switcher.
  - Selecting PDFs, folders, or tags no longer replaces folder/tag navigation.
- Added `crates/pdf-folio-ui/src/library/view/inspector.rs`.
  - The right inspector is visible in Library mode.
  - It reuses the existing single-PDF, multi-selection, and folder detail panels.
  - It adds lightweight Library Summary and Tag inspector states.

Known follow-up:

- Command-palette mouse/click and Enter execution work; Up/Down navigation is completed in the follow-up tranche below.

## 2026-07-06 completion tranche

Completed and verified with `cargo check -p pdf-folio-ui` before final tests:

- Finished command-palette keyboard execution.
  - `Ctrl+K` opens the palette.
  - Enter runs the highlighted command.
  - Up/Down shortcut handling moves the highlighted command while the palette is open.
- Added inspector tag editing.
  - Single-PDF inspector now shows removable tag chips and an add-tag input.
  - Multi-selection inspector now shows common removable tags and an add-tags-to-selection input.
  - Comma-separated tag input is supported through the inspector add action.
- Expanded right inspector actions.
  - Single PDF: editable title/author, save/reset metadata, export, reveal, open containing folder, copy path, move, refresh metadata, rebuild thumbnail, reindex, trash.
  - Multiple PDFs: tag editing, move, export selected, refresh metadata, rebuild thumbnails, reindex, trash.
  - Folder: export folder.
  - Tag: export tagged PDFs and open Tag Manager.
- Added Tag Manager modal.
  - Lists tags and counts.
  - Supports rename, delete, and merge into the typed destination.
  - Uses the existing history-aware rename/delete tag tasks.
- Added export workflow.
  - Entry points: selection toolbar, PDF inspector, multi-selection inspector, folder inspector, tag inspector, PDF/folder/tag/background context menus, and command palette for selected PDFs.
  - Export sources: one PDF, selected PDFs, folder PDFs, tag PDFs.
  - Export modes: copy flat, preserve folder structure, ZIP.
  - Filename templates: original filename, title, author-title, year-author-title.
  - Metadata sidecars: optional CSV and JSON, with optional tags and reading progress fields.
  - Conflict behavior: skip, overwrite, keep both.
  - Completion actions: reveal exported folder/path and copy export path.
- Added import review modal.
  - Local and Raindrop imports populate a review summary.
  - Review shows imported, duplicate, failed, and destination counts/details.
  - Review can select imported PDFs, add tags to selected imported PDFs, and open the move picker.
- Context menu cleanup continued.
  - Export is now placed beside the objects it acts on: PDF, selected PDFs, folder, and tag.

## 2026-07-06 polish tranche

Completed and verified with `cargo fmt`, `cargo check -p pdf-folio-ui`, and `cargo test -p pdf-folio-ui`:

- Removed the old app-menu selection strip implementation.
  - `crates/pdf-folio-ui/src/app/menu/selection.rs` was deleted.
  - Selection actions now live in the Library header selection toolbar, right inspector, context menus, and command palette.
- Unified import entry points.
  - The Library header now exposes one Import button.
  - `view_import_menu_dialog` presents PDF, folder, and Raindrop import choices while reusing the existing import flows.
- Finished inspector tag autocomplete.
  - The inspector tag editor now offers existing matching tag suggestions and applies them directly to the current selection.
- Split Tag Manager filter and merge controls.
  - Tag filtering and merge destination now use separate inputs.
  - Escape/close clears both transient fields.
- Preserved folder structure in ZIP export.
  - ZIP archive names now use the same folder-path builder as preserve-folder copy export.

## 2026-07-07 corrective polish

Completed and verified with `cargo fmt`, `cargo check -p pdf-folio-ui`, and `cargo test -p pdf-folio-ui`:

- Made the right-side Library inspector a first-class adjustable panel.
  - Added inspector open/width/resizing state.
  - Added a left-edge resize handle for the right panel.
  - Library grid/list width now subtracts the inspector width when it is visible.
  - Inspector summary and tag views use the same sidebar scrollbar style with the scrollbar anchored on the right.
- Added keyboard visibility toggle for the right inspector.
  - Pressing `i` toggles the inspector in Library mode.
  - The shortcut only fires when keyboard input is not captured by a text field/control.
- Reworked the left sidebar into one stable stacked navigation layout.
  - Removed the Files/Tags tab body from the active sidebar.
  - The sidebar now shows Library quick links, Folders, and Tags in a single scrollable layout.
  - Selected PDF, multi-selection, folder, and tag details remain in the right inspector instead of replacing navigation.

## 2026-07-07 top menu removal

Completed and verified with `cargo fmt`, `cargo check -p pdf-folio-ui`, and `cargo test -p pdf-folio-ui`:

- Audited the former top menu actions and moved executable actions into the command registry.
  - Added command-palette coverage for Library clipboard/history actions, typed tag actions, current-folder selection actions, sort modes, theme/style commands, and Viewer document/zoom/scroll/spread commands.
  - Kept shared commands available in both modes where appropriate, such as Open PDF, Toggle Theme, and Reload Styles.
- Made the command palette mode-aware.
  - Library-only commands are hidden when opened in Viewer mode.
  - Viewer-only commands are hidden when opened in Library mode.
- Removed the top menu bar entirely from the app shell.
  - Deleted `crates/pdf-folio-ui/src/app/menu.rs`.
  - Removed app-menu chrome state and app-menu messages/update branches.
  - Removed menu-height layout offsets so Viewer and Library reclaim the former menu-bar space.
