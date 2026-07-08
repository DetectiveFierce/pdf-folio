# PDF-Folio UI Surface And Menu Inventory

PDF-Folio is a local-first PDF library and reader. The app helps a user collect PDFs, organize them into folders and tags, clean and edit display metadata, track reading-oriented attributes, find missing files, and perform maintenance tasks such as thumbnail rebuilds and full-text reindexing. It also includes a built-in PDF viewer for opening documents, navigating pages and outlines, changing zoom/scroll/spread modes, searching document text, and copying text selections.

At a high level, the app has two primary working modes:

- Library mode - manage the PDF collection, imports, folders, tags, metadata, sorting, filtering, selection, and batch maintenance.
- Viewer mode - read one PDF, navigate pages and table-of-contents entries, search text, adjust viewing layout, zoom, and return to the library.

This file inventories the current runtime UI surfaces and the functionality they expose. It is meant as a compact handoff reference for agents that cannot inspect the app in one full-screen screenshot.

Primary sources:

- `crates/pdf-folio-ui/src/app/menu.rs` - top application menu bar and dropdowns.
- `crates/pdf-folio-ui/src/app/view.rs` - app shell, viewer toolbar, viewer find bar, overlays.
- `crates/pdf-folio-ui/src/app/context_menu.rs` - right-click context menus.
- `crates/pdf-folio-ui/src/library/view.rs` - library toolbar, breadcrumbs, filters, entry grid/list.
- `crates/pdf-folio-ui/src/library/view/sidebar.rs` - library sidebar, folder/tag navigation, details panels.
- `crates/pdf-folio-ui/src/library/view/dialogs.rs` - library dialogs and Raindrop import flows.
- `crates/pdf-folio-ui/src/viewer/outline.rs` - viewer sidebar, table of contents, thumbnails, jump dialog.
- `crates/pdf-folio-ui/src/viewer/zoom.rs` - viewer zoom control and zoom preset menu.

## Global App Shell

### Top Application Menu Bar

Always renders the same top-level menu names:

- File
- Edit
- View
- Document
- Library
- Tools
- Help

Menu items can be disabled depending on mode and state. Library-only actions are disabled while viewing a PDF. Viewer-only actions are disabled in the library. Selection-dependent actions are disabled when no matching selection exists.

### Global Modal And Overlay Layer

The shell can overlay these surfaces over the current mode:

- Application menu dropdowns.
- Library selection menu dropdowns.
- Viewer zoom preset dropdown.
- Right-click context menu.
- Library name dialog.
- Confirmation dialogs.
- Create folder dialog.
- Move picker dialog.
- Raindrop connection dialog.
- Raindrop import dialog.
- Raindrop import progress dialog.
- Floating drag previews for library entries or folders.
- Startup library loading layer.
- Pending document loading layer.
- Library history restore spinner.

## Library Mode

### File Menu

- Open PDF... (`Ctrl+O`) - open a PDF directly in the viewer.
- Import Folder... - import PDFs from a folder into the library.
- Import from Raindrop.io... - start the Raindrop import flow.
- Refresh Library (`F5`) - reload library data.
- Back to Library (`Esc`) - present in the menu but disabled in Library mode.

### Edit Menu

- Undo (`Ctrl+Z`) - undo latest library organization edit.
- Redo (`Ctrl+Y`) - redo library organization edit.
- Cut (`Ctrl+X`) - cut selected library PDFs or folder.
- Copy (`Ctrl+C`) - copy selected library PDFs or folder.
- Paste (`Ctrl+V`) - paste the internal library clipboard.
- Select All Visible PDFs (`Ctrl+A`) - select all currently visible library PDFs.
- Clear Selection (`Esc`) - clear current library selection.
- Save Details (`Enter`) - save metadata edits for a single selected PDF.
- Reset Details... - reset metadata edits for a single selected PDF.
- Add Typed Tag - add the current bulk tag input to selected PDFs.
- Remove Typed Tag - remove the current bulk tag input from selected PDFs.
- Move To... - open folder destination picker for selected PDFs.
- Move to Trash... (`Delete`) - move selected PDFs to trash.

### View Menu

- Switch to Grid / Switch to List - toggle library layout mode.
- Switch to Dark Theme / Switch to Light Theme - toggle app theme.
- Reload Styles - reload style definitions.
- Hide Table of Contents / Show Table of Contents - viewer-only, disabled in Library mode.
- Scrolling submenu - viewer-only, disabled in Library mode.
- Spreads submenu - viewer-only, disabled in Library mode.
- Zoom In / Zoom Out / Reset Zoom - viewer-only, disabled in Library mode.

### Document Menu

Viewer-only commands, disabled in Library mode:

- Jump to Page... (`Ctrl+G`)
- Find in Document (`Ctrl+F`)
- Hide Table of Contents / Show Table of Contents
- Zoom In (`Ctrl++`)
- Zoom Out (`Ctrl+-`)
- Reset Zoom (`Ctrl+0`)

### Library Menu

- `< Previous Library View` - restore the previous library view before a tag-pill navigation.
- Upload Folder... - import a folder.
- Upload PDF... - import a single PDF into the library.
- Import from Raindrop.io... - start Raindrop import.
- Refresh Library (`F5`) - reload library data.
- New Folder... - create a folder in the current library location.
- Add Selection to Current Folder - add selected PDFs to active folder.
- Remove Selection from Current Folder - remove selected PDFs from active folder.
- Show Missing Files - toggle missing-file filter. Shows the current missing count as item detail.
- Manual - sort manually.
- Title A-Z - sort by title ascending.
- Title Z-A - sort by title descending.
- Author A-Z - sort by author ascending.
- Author Z-A - sort by author descending.
- Recently Added - sort newest imports first.
- Recently Opened - sort recently opened first.
- Progress - sort by reading progress.
- Page Count - sort by page count.
- Missing - sort missing files first.

### Tools Menu

Selection-dependent library maintenance actions:

- Apply Title Sort Cleanup - clean selected display titles for sorting.
- Refresh PDF Metadata - refresh selected PDFs from embedded metadata.
- Reset Display Metadata... - reset display metadata for selected PDFs.
- Rebuild Thumbnails - regenerate selected thumbnails.
- Reindex Full Text - rebuild selected full-text index.

### Help Menu

Static informational rows:

- PDF-Folio - Local PDF library and reader.
- Status - No help actions available yet.

### Library Toolbar

The library toolbar sits below the app menu bar in Library mode.

- Sidebar expand button - shown when the library sidebar is collapsed.
- Search library input - filters visible entries by text.
- Search clear button - appears when search text is present.
- Undo icon button - undo library edit.
- Redo icon button - redo library edit.
- Sort picker - choose one of the library sort modes listed in the Library menu.
- Grid/List layout toggle - switch between card grid and compact list.
- Metadata density picker - shown when the toolbar is wide enough.
- Viewer button - returns to an already-open viewer document when one exists.
- New folder button - opens the create folder dialog, hidden in Trash view.

### Library Breadcrumb And Filter Row

Shown under the library toolbar.

- Breadcrumb trail - navigates folder hierarchy.
- Filter summary pills - show active tag, reading, missing, and search filters.
- Clear filters pill - clears active library filters.
- Grid zoom control - adjusts card/grid zoom.
- Reorder hint - reports whether manual drag reorder is currently available.

When PDFs are selected, this row becomes a selection status row:

- Master checkbox - select or clear visible selection.
- Selected-count label.
- Optional library status text.
- Reorder hint.

### Library Content Area

The main content area supports:

- Empty states for root library, empty folder, and empty Trash Can.
- Compact list rows.
- Grid/card layout.
- Folder cards above PDFs.
- Parent-directory drop box during folder drag operations.
- Entry drop zones for drag reorder.
- Ghost placeholder entries during drag operations.
- Bulk operation progress banner.
- Dismissible library error banner.
- Right-click background context menu.

### Library Sidebar

The sidebar can be collapsed, expanded, and horizontally resized.

Navigation state:

- Explorer heading.
- Collapse Sidebar button.
- Files tab.
- Tags tab.
- Switch Library button at the bottom.

Files tab:

- Library root row with smart count.
- Expand/collapse root folder tree.
- Nested folder rows with expand/collapse, selection, and counts.
- Trash Can row with item count.

Tags tab:

- All tags row.
- Tag list with per-tag PDF counts.
- Tag filtering.
- Inline tag rename row when renaming.
- Right-click tag context menu.

Selected folder details sidebar:

- Folder Details heading.
- Clear selection button.
- Folder name.
- PDF count.
- child-folder count.
- Reading count.
- Missing count.
- Open folder action.
- Rename input.
- Rename action.
- Trash action.
- Move Earlier / Move Later actions.
- Move to root action when applicable.
- Move up action when applicable.
- Clear selection action.

Selected PDF or multi-selection sidebars also replace the navigation sidebar when PDFs are selected. They expose metadata/details and bulk selection workflows through the same selection and edit actions listed above.

### Library Context Menus

Library entry right-click:

- Open PDF (`Enter`)
- Select Only
- Add To Selection / Remove From Selection
- Clear Selection (`Esc`)
- Add Tag...
- Move To...
- Save Details (`Enter`)
- Reset Details...
- Reveal in File Manager
- Open Containing Folder
- Relink Missing File...
- Refresh Metadata
- Reset Metadata...
- Rebuild Thumbnail
- Reindex Full Text
- Move to Trash... (`Del`)

Folder right-click:

- Open Folder / Open Library
- New Folder...
- Refresh Library (`F5`)
- Rename Folder (`Enter`)
- Move To...
- Move To Root
- Move Up
- Move Earlier
- Move Later
- Move Folder to Trash...

Library background right-click:

- Import Folder...
- New Folder...
- Refresh Library (`F5`)
- Move To...
- Switch To Grid / Switch To List
- Sort Manually
- Sort By Title

Tag right-click:

- Rename Tag
- Delete Tag

### Library Dialogs

Confirmation dialogs:

- Generic confirmation with Cancel and action-specific confirm button.
- Move Folder to Trash confirmation with PDF/folder counts and optional warning suppression.

Create Folder dialog:

- Folder name input.
- Cancel.
- Create.

Move Picker dialog:

- Destination folder tree.
- Library root destination.
- Nested folder destination rows.
- Invalid destinations are marked unavailable.
- Cancel.
- Select.

Raindrop connection dialog:

- Open Raindrop Integrations.
- Copy Redirect URI.
- Client ID input.
- Client Secret input.
- Cancel.
- Sign in.

Raindrop import dialog:

- PDF list from Raindrop.
- Selected PDF count.
- Select all.
- Select none.
- Import destination selector.
- Optional new-folder destination.
- Cancel.
- Import.

Raindrop import progress dialog:

- Shows current import phase/progress while selected Raindrop PDFs are imported.

## Viewer Mode

### File Menu

- Open PDF... (`Ctrl+O`) - choose another PDF.
- Import Folder... - library-only, disabled in Viewer mode.
- Import from Raindrop.io... - library-only, disabled in Viewer mode.
- Refresh Library (`F5`) - library-only, disabled in Viewer mode.
- Back to Library (`Esc`) - return to library.

### Edit Menu

Library selection and metadata commands are present but generally disabled in Viewer mode.

### View Menu

- Switch to Grid / Switch to List - still available globally, though it affects library layout.
- Switch to Dark Theme / Switch to Light Theme - toggle app theme.
- Reload Styles - reload style definitions.
- Hide Table of Contents / Show Table of Contents - toggle viewer sidebar.
- Scrolling submenu:
  - Page Scrolling
  - Vertical Scrolling
  - Horizontal Scrolling
  - Wrapped Scrolling
- Spreads submenu:
  - No Spreads
  - Odd Spreads
  - Even Spreads
- Zoom In (`Ctrl++`)
- Zoom Out (`Ctrl+-`)
- Reset Zoom (`Ctrl+0`)

### Document Menu

- Jump to Page... (`Ctrl+G`) - open jump dialog.
- Find in Document (`Ctrl+F`) - open viewer find bar.
- Hide Table of Contents / Show Table of Contents - toggle viewer sidebar.
- Zoom In (`Ctrl++`)
- Zoom Out (`Ctrl+-`)
- Reset Zoom (`Ctrl+0`)

### Library Menu

Library actions are present but disabled in Viewer mode.

### Tools Menu

Library selection maintenance actions are present but disabled in Viewer mode.

### Help Menu

Static informational rows:

- PDF-Folio - Local PDF library and reader.
- Status - No help actions available yet.

### Viewer Toolbar

The viewer toolbar sits below the app menu bar in Viewer mode.

- Library button - return to Library mode.
- Open PDF button - choose a PDF from disk.
- Document title - current file name, with tooltip if truncated.
- Page control:
  - Previous page button.
  - Current page number.
  - Double-click page number to edit.
  - Total page count.
  - Next page button.
- Zoom out button.
- Zoom control:
  - Current zoom percent.
  - Double-click zoom value to edit.
  - Chevron opens zoom preset dropdown.
- Zoom in button.
- Selection status - appears when text is selected.
- Copy button - appears when text is selected.
- Clear button - appears when text is selected.
- Dark / Light theme toggle button.

Viewer zoom preset dropdown:

- Automatic Zoom
- Actual Size
- Page Fit
- Page Width
- 50%
- 75%
- 100%
- 125%
- 150%
- 200%
- 300%
- 400%

### Viewer Sidebar

Shown when the table of contents/sidebar is open.

- Contents tab.
- Thumbnails tab.
- Hide Contents button.

Contents tab:

- Nested document outline.
- Expand/collapse outline nodes.
- Jump to page from outline entries.
- Empty state: No table of contents.

Thumbnails tab:

- Page thumbnail list.
- Page number labels.
- Jump to page by clicking a thumbnail.

When the sidebar is closed, the viewer canvas shows a floating Show Contents button.

### Viewer Canvas

The main PDF canvas supports:

- Rendering visible PDF pages.
- Placeholder page surfaces while rendering.
- Scrollable viewport.
- Page/spread layout according to scroll and spread mode.
- Mouse wheel scrolling.
- Ctrl+wheel zoom behavior.
- Horizontal wheel handling when horizontal scroll mode is active.
- Text hit testing.
- Drag-to-select text.
- Find result highlights.
- Selected find match highlight.
- Text selection highlight.
- Click empty canvas to clear text selection.
- Right-click viewer context menu.

### Viewer Find Bar

Opened by `Find in Document` or `Ctrl+F`.

- Find in Text input.
- Match counter, formatted as current/total.
- Previous match button.
- Next match button.
- Highlight All checkbox.
- Match Case checkbox.
- Match Diacritics checkbox.
- Close button.

### Viewer Jump Dialog

Opened by `Jump to Page...`, `Ctrl+G`, or page input workflows.

- Page input.
- Total page count display.
- Go button.
- Cancel button.

### Viewer Context Menu

Right-click on viewer canvas:

- Copy Selection (`Ctrl+C`)
- Find In Document (`Ctrl+F`)
- Jump To Page... (`Ctrl+G`)
- Zoom In (`Ctrl++`)
- Zoom Out (`Ctrl+-`)
- Reset Zoom (`Ctrl+0`)
- Hide Table Of Contents / Show Table Of Contents
- Back To Library (`Esc`)

## Signed-Out And Library Switcher Surfaces

### Signed-Out Surface

Shown when sync auth requires sign-in.

- PDF-Folio title.
- Account/status message.
- Sign in with Google / Signing in... button.
- Optional error banner.

### Library Switcher Surface

The library switcher is opened from the library sidebar.

- List of available libraries.
- Active library indication.
- Create/rename/delete style library management flows are backed by the library name dialog.
- Back to Library button.

### Library Name Dialog

Used for library creation/renaming flows.

- Name input.
- Cancel.
- Confirm action button.
