//! Right-click context menu rendering and action routing.

use super::*;
use iced::widget::{column, row, stack};
#[derive(Debug, Clone, Copy)]
struct ContextMenuItemSpec {
    label: &'static str,
    detail: &'static str,
    enabled: bool,
    destructive: bool,
    action: ContextMenuAction,
}

impl PDFolioApp {
    pub(crate) fn open_context_menu(&mut self, target: ContextMenuTarget) {
        self.viewer.zoom_menu_open = false;

        match &target {
            ContextMenuTarget::LibraryEntry(entry_id) => {
                if !self.library.selected_library_entries.contains(entry_id) {
                    self.clear_library_selection();
                    self.select_library_entry(entry_id.clone());
                } else {
                    self.library.details_entry_id = Some(entry_id.clone());
                    self.sync_details_editor_to_selection();
                }
            }
            ContextMenuTarget::Folder(folder_id) => {
                self.select_folder_for_details(folder_id.clone());
            }
            ContextMenuTarget::Tag(_) => {}
            ContextMenuTarget::LibraryBackground | ContextMenuTarget::ViewerCanvas => {}
        }

        self.chrome.open_context_menu = Some(ContextMenu {
            target,
            position: self.chrome.cursor_position,
        });
    }

    pub(crate) fn context_menu_action_message(&self, action: ContextMenuAction) -> Option<Message> {
        let target = &self.chrome.open_context_menu.as_ref()?.target;
        match action {
            ContextMenuAction::Open => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::OpenLibraryEntry(entry_id.clone()))
                }
                ContextMenuTarget::Folder(folder_id) => {
                    Some(Message::FolderSelected(folder_id.clone()))
                }
                ContextMenuTarget::Tag(_) => None,
                ContextMenuTarget::LibraryBackground => None,
                ContextMenuTarget::ViewerCanvas => None,
            },
            ContextMenuAction::SelectOnly => None,
            ContextMenuAction::AddToSelection => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::EntryCheckboxToggled(entry_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::ClearSelection => Some(Message::ClearLibrarySelection),
            ContextMenuAction::AddTag => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::StartTagEntry(entry_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::MoveTo => Some(Message::OpenMoveSelectionDialog),
            ContextMenuAction::Export => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    if self.library.selected_library_entries.len() > 1
                        && self.library.selected_library_entries.contains(entry_id)
                    {
                        Some(Message::OpenExportDialog(ExportSource::SelectedEntries))
                    } else {
                        Some(Message::OpenExportDialog(ExportSource::SingleEntry(
                            entry_id.clone(),
                        )))
                    }
                }
                ContextMenuTarget::Folder(Some(folder_id)) => Some(Message::OpenExportDialog(
                    ExportSource::Folder(folder_id.clone()),
                )),
                ContextMenuTarget::Tag(tag) => {
                    Some(Message::OpenExportDialog(ExportSource::Tag(tag.clone())))
                }
                ContextMenuTarget::LibraryBackground
                    if !self.library.selected_library_entries.is_empty() =>
                {
                    Some(Message::OpenExportDialog(ExportSource::SelectedEntries))
                }
                _ => None,
            },
            ContextMenuAction::RevealInFileManager => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::RevealEntryInFileManager(entry_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::OpenContainingFolder => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::OpenEntryContainingFolder(entry_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::RelinkMissingFile => match target {
                ContextMenuTarget::LibraryEntry(entry_id) => {
                    Some(Message::RelinkMissingEntry(entry_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::SaveDetails => Some(Message::SaveDetailsMetadata),
            ContextMenuAction::ResetDetails => {
                let entry_id = self.library.details_entry_id.clone()?;
                Some(Message::RequestConfirmation(
                    ConfirmationAction::ResetDetailsMetadata(entry_id),
                ))
            }
            ContextMenuAction::RefreshMetadata => Some(Message::BulkRefreshPdfMetadata),
            ContextMenuAction::ResetMetadata => Some(Message::RequestConfirmation(
                ConfirmationAction::BulkResetDisplayMetadata,
            )),
            ContextMenuAction::RebuildThumbnails => Some(Message::BulkRebuildThumbnails),
            ContextMenuAction::Reindex => Some(Message::BulkReindex),
            ContextMenuAction::DeleteFromLibrary => Some(Message::RequestConfirmation(
                ConfirmationAction::BulkDeleteFromLibrary,
            )),
            ContextMenuAction::SelectFolder => match target {
                ContextMenuTarget::Folder(folder_id) => {
                    Some(Message::FolderSelected(folder_id.clone()))
                }
                _ => None,
            },
            ContextMenuAction::NewFolder => Some(Message::OpenCreateFolderDialog),
            ContextMenuAction::RenameFolder => Some(Message::RenameSelectedFolder),
            ContextMenuAction::RenameTag => match target {
                ContextMenuTarget::Tag(tag) => Some(Message::StartTagRename(tag.clone())),
                _ => None,
            },
            ContextMenuAction::DeleteTag => match target {
                ContextMenuTarget::Tag(tag) => Some(Message::RequestConfirmation(
                    ConfirmationAction::DeleteTag(tag.clone()),
                )),
                _ => None,
            },
            ContextMenuAction::MoveFolderTo => Some(Message::OpenMoveSelectedFolderDialog),
            ContextMenuAction::MoveFolderToRoot => Some(Message::MoveSelectedFolderToRoot),
            ContextMenuAction::MoveFolderUp => Some(Message::MoveSelectedFolderUp),
            ContextMenuAction::MoveFolderEarlier => Some(Message::MoveSelectedFolderEarlier),
            ContextMenuAction::MoveFolderLater => Some(Message::MoveSelectedFolderLater),
            ContextMenuAction::DeleteFolder => Some(Message::RequestDeleteSelectedFolder),
            ContextMenuAction::ImportFolder => Some(Message::ImportFolderDialog),
            ContextMenuAction::RefreshLibrary => Some(Message::LibraryRefresh),
            ContextMenuAction::ToggleLayout => Some(Message::ToggleViewMode),
            ContextMenuAction::SortManual => {
                Some(Message::LibrarySortChanged(LibrarySortMode::Manual))
            }
            ContextMenuAction::SortTitleAsc => {
                Some(Message::LibrarySortChanged(LibrarySortMode::TitleAsc))
            }
            ContextMenuAction::CopyViewerSelection => Some(Message::CopyViewerTextSelection),
            ContextMenuAction::FindInDocument => Some(Message::OpenViewerFind),
            ContextMenuAction::JumpToPage => Some(Message::OpenJumpDialog),
            ContextMenuAction::ZoomIn => Some(Message::ZoomIn),
            ContextMenuAction::ZoomOut => Some(Message::ZoomOut),
            ContextMenuAction::ResetZoom => {
                Some(Message::ZoomPresetSelected(ZoomPreset::Automatic))
            }
            ContextMenuAction::ToggleToc => Some(Message::ToggleSidebar),
            ContextMenuAction::BackToLibrary => Some(Message::BackToLibrary),
        }
    }
}

pub(crate) fn context_menu_capture_layer<'a>(_app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::ContextMenuClosed)
            .on_right_press(Message::ContextMenuClosed),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(crate) fn view_context_menu_dropdown(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let Some(menu) = app.chrome.open_context_menu.as_ref() else {
        return container("").into();
    };
    let width = app.layout().context_menu_panel_width;
    let max_x = app
        .viewer
        .viewport_width
        .max(app.library.library_viewport_width)
        .max(app.layout().window_width)
        - width
        - Spacing::SM;
    let x = menu.position.x.clamp(Spacing::SM, max_x.max(Spacing::SM));
    let height = context_menu_height(app, &menu.target, tokens);
    let max_y = app.viewer.viewport_height.max(app.layout().window_height) - height - Spacing::SM;
    let y = menu.position.y.clamp(Spacing::SM, max_y.max(Spacing::SM));

    stack![pin(context_menu_panel(app, &menu.target, tokens)).x(x).y(y)]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn context_menu_height(app: &PDFolioApp, target: &ContextMenuTarget, tokens: ThemeTokens) -> f32 {
    let groups = context_menu_groups(app, target);
    let item_count = groups.iter().map(Vec::len).sum::<usize>();
    let separator_count = groups.len().saturating_sub(1);
    let child_count = item_count + separator_count;
    let gap_count = child_count.saturating_sub(1);
    let panel_layout = tokens.class_styles[Class::ContextMenuPanel.index()].layout;

    item_count as f32 * app.layout().context_menu_item_height
        + separator_count as f32 * tokens.primitives.context_menu_separator_height
        + gap_count as f32 * panel_layout.spacing.unwrap_or(2.0)
        + panel_layout.padding_y(Spacing::XS) * 2.0
}

fn context_menu_panel<'a>(
    app: &'a PDFolioApp,
    target: &'a ContextMenuTarget,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let panel_layout = tokens.class_styles[Class::ContextMenuPanel.index()].layout;
    let mut panel = column![]
        .spacing(panel_layout.spacing.unwrap_or(2.0))
        .padding([
            panel_layout.padding_y(Spacing::XS),
            panel_layout.padding_x(Spacing::XS),
        ]);
    let groups = context_menu_groups(app, target);
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            panel = panel.push(context_menu_separator(tokens));
        }
        for item in group {
            panel = panel.push(context_menu_item(
                *item,
                tokens,
                app.layout().context_menu_item_height,
            ));
        }
    }

    container(panel)
        .width(app.layout().context_menu_panel_width)
        .style(move |_| container_style(tokens, Class::ContextMenuPanel))
        .into()
}

fn context_menu_groups(
    app: &PDFolioApp,
    target: &ContextMenuTarget,
) -> Vec<Vec<ContextMenuItemSpec>> {
    match target {
        ContextMenuTarget::LibraryEntry(entry_id) => library_entry_context_groups(app, entry_id),
        ContextMenuTarget::Folder(folder_id) => folder_context_groups(app, folder_id.as_ref()),
        ContextMenuTarget::Tag(tag) => tag_context_groups(app, tag),
        ContextMenuTarget::LibraryBackground => library_background_context_groups(app),
        ContextMenuTarget::ViewerCanvas => viewer_context_groups(app),
    }
}

fn library_entry_context_groups(
    app: &PDFolioApp,
    entry_id: &EntryId,
) -> Vec<Vec<ContextMenuItemSpec>> {
    let entry = app
        .active_library_entries()
        .iter()
        .find(|entry| &entry.id == entry_id);
    let has_selection = !app.library.selected_library_entries.is_empty();
    let multi = app.library.selected_library_entries.len() > 1;
    let missing = entry.is_some_and(|entry| entry.missing);

    vec![
        vec![
            spec("Open PDF", "Enter", true, ContextMenuAction::Open),
            spec("Select Only", "", true, ContextMenuAction::SelectOnly),
            spec(
                if app.library.selected_library_entries.contains(entry_id) {
                    "Remove From Selection"
                } else {
                    "Add To Selection"
                },
                "",
                has_selection,
                ContextMenuAction::AddToSelection,
            ),
            spec(
                "Clear Selection",
                "Esc",
                has_selection,
                ContextMenuAction::ClearSelection,
            ),
        ],
        vec![
            spec("Add Tag...", "", true, ContextMenuAction::AddTag),
            spec("Move To...", "", has_selection, ContextMenuAction::MoveTo),
            spec("Export...", "", true, ContextMenuAction::Export),
            spec(
                "Save Details",
                "Enter",
                !multi,
                ContextMenuAction::SaveDetails,
            ),
            spec(
                "Reset Details...",
                "",
                !multi,
                ContextMenuAction::ResetDetails,
            ),
        ],
        vec![
            spec(
                "Reveal in File Manager",
                "",
                true,
                ContextMenuAction::RevealInFileManager,
            ),
            spec(
                "Open Containing Folder",
                "",
                true,
                ContextMenuAction::OpenContainingFolder,
            ),
            spec(
                "Relink Missing File...",
                "",
                missing,
                ContextMenuAction::RelinkMissingFile,
            ),
        ],
        vec![
            spec(
                "Refresh Metadata",
                "",
                true,
                ContextMenuAction::RefreshMetadata,
            ),
            spec(
                "Reset Metadata...",
                "",
                true,
                ContextMenuAction::ResetMetadata,
            ),
            spec(
                "Rebuild Thumbnail",
                "",
                true,
                ContextMenuAction::RebuildThumbnails,
            ),
            spec("Reindex Full Text", "", true, ContextMenuAction::Reindex),
            spec(
                "Move to Trash...",
                "Del",
                true,
                ContextMenuAction::DeleteFromLibrary,
            ),
        ],
    ]
}

fn folder_context_groups(
    app: &PDFolioApp,
    folder_id: Option<&FolderId>,
) -> Vec<Vec<ContextMenuItemSpec>> {
    let has_folder = folder_id.is_some();
    let has_parent = app
        .details_folder()
        .is_some_and(|folder| folder.parent_id.is_some());
    let has_grandparent = app.details_folder().is_some_and(|folder| {
        folder.parent_id.as_ref().is_some_and(|parent_id| {
            app.library
                .library_folders
                .iter()
                .find(|candidate| &candidate.id == parent_id)
                .and_then(|parent| parent.parent_id.as_ref())
                .is_some()
        })
    });
    let can_move_earlier = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, _, index)| index > 0);
    let can_move_later = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, folder_ids, index)| index + 1 < folder_ids.len());

    vec![
        vec![
            spec(
                if has_folder {
                    "Open Folder"
                } else {
                    "Open Library"
                },
                "",
                true,
                ContextMenuAction::SelectFolder,
            ),
            spec("New Folder...", "", true, ContextMenuAction::NewFolder),
            spec(
                "Refresh Library",
                "F5",
                true,
                ContextMenuAction::RefreshLibrary,
            ),
        ],
        vec![
            spec(
                "Rename Folder",
                "Enter",
                has_folder,
                ContextMenuAction::RenameFolder,
            ),
            spec(
                "Move To...",
                "",
                has_folder,
                ContextMenuAction::MoveFolderTo,
            ),
            spec(
                "Export Folder...",
                "",
                has_folder,
                ContextMenuAction::Export,
            ),
            spec(
                "Move To Root",
                "",
                has_parent,
                ContextMenuAction::MoveFolderToRoot,
            ),
            spec(
                "Move Up",
                "",
                has_grandparent,
                ContextMenuAction::MoveFolderUp,
            ),
            spec(
                "Move Earlier",
                "",
                can_move_earlier,
                ContextMenuAction::MoveFolderEarlier,
            ),
            spec(
                "Move Later",
                "",
                can_move_later,
                ContextMenuAction::MoveFolderLater,
            ),
            spec(
                "Move Folder to Trash...",
                "",
                has_folder,
                ContextMenuAction::DeleteFolder,
            ),
        ],
    ]
}

fn library_background_context_groups(app: &PDFolioApp) -> Vec<Vec<ContextMenuItemSpec>> {
    vec![
        vec![
            spec(
                "Import Folder...",
                "",
                true,
                ContextMenuAction::ImportFolder,
            ),
            spec("New Folder...", "", true, ContextMenuAction::NewFolder),
            spec(
                "Refresh Library",
                "F5",
                true,
                ContextMenuAction::RefreshLibrary,
            ),
        ],
        vec![
            spec(
                "Move To...",
                "",
                !app.library.selected_library_entries.is_empty(),
                ContextMenuAction::MoveTo,
            ),
            spec(
                "Export Selection...",
                "",
                !app.library.selected_library_entries.is_empty(),
                ContextMenuAction::Export,
            ),
            spec(
                if app.library.compact_view_mode {
                    "Switch To Grid"
                } else {
                    "Switch To List"
                },
                "",
                true,
                ContextMenuAction::ToggleLayout,
            ),
            spec("Sort Manually", "", true, ContextMenuAction::SortManual),
            spec("Sort By Title", "", true, ContextMenuAction::SortTitleAsc),
        ],
    ]
}

fn tag_context_groups(app: &PDFolioApp, tag: &str) -> Vec<Vec<ContextMenuItemSpec>> {
    let exists = app.all_tags().iter().any(|candidate| candidate == tag);
    vec![vec![
        spec(
            "Export Tagged PDFs...",
            "",
            exists,
            ContextMenuAction::Export,
        ),
        spec("Rename Tag", "", exists, ContextMenuAction::RenameTag),
        destructive_spec("Delete Tag", "", exists, ContextMenuAction::DeleteTag),
    ]]
}

fn viewer_context_groups(app: &PDFolioApp) -> Vec<Vec<ContextMenuItemSpec>> {
    let has_selection = app.viewer.viewer_text_selection.is_some();
    vec![
        vec![
            spec(
                "Copy Selection",
                "Ctrl+C",
                has_selection,
                ContextMenuAction::CopyViewerSelection,
            ),
            spec(
                "Find In Document",
                "Ctrl+F",
                true,
                ContextMenuAction::FindInDocument,
            ),
            spec(
                "Jump To Page...",
                "Ctrl+G",
                true,
                ContextMenuAction::JumpToPage,
            ),
        ],
        vec![
            spec("Zoom In", "Ctrl++", true, ContextMenuAction::ZoomIn),
            spec("Zoom Out", "Ctrl+-", true, ContextMenuAction::ZoomOut),
            spec("Reset Zoom", "Ctrl+0", true, ContextMenuAction::ResetZoom),
        ],
        vec![
            spec(
                if app.viewer.toc_open {
                    "Hide Table Of Contents"
                } else {
                    "Show Table Of Contents"
                },
                "",
                true,
                ContextMenuAction::ToggleToc,
            ),
            spec(
                "Back To Library",
                "Esc",
                true,
                ContextMenuAction::BackToLibrary,
            ),
        ],
    ]
}

fn spec(
    label: &'static str,
    detail: &'static str,
    enabled: bool,
    action: ContextMenuAction,
) -> ContextMenuItemSpec {
    ContextMenuItemSpec {
        label,
        detail,
        enabled,
        destructive: false,
        action,
    }
}

fn destructive_spec(
    label: &'static str,
    detail: &'static str,
    enabled: bool,
    action: ContextMenuAction,
) -> ContextMenuItemSpec {
    ContextMenuItemSpec {
        label,
        detail,
        enabled,
        destructive: true,
        action,
    }
}

fn context_menu_item<'a>(
    item: ContextMenuItemSpec,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let item_layout = tokens.class_styles[Class::ContextMenuItem.index()].layout;
    let item_text = tokens.class_styles[Class::ContextMenuItem.index()].text;
    let state = if item.enabled {
        ComponentState::Normal
    } else {
        ComponentState::Disabled
    };
    let label_color = if item.destructive && item.enabled {
        tokens.error
    } else {
        class_text_color(tokens, Class::ContextMenuItem, state, tokens.text_primary)
    };
    let detail_color =
        class_text_color(tokens, Class::ContextMenuItem, state, tokens.text_secondary);
    let label_size = item_text.size.unwrap_or(FontSize::MD);
    let detail_size = tokens.class_styles[Class::ContextMenuPanel.index()]
        .text
        .size
        .unwrap_or(FontSize::SM);
    let item_weight = item_text.weight.unwrap_or(FontWeight::REGULAR);
    let content = row![
        text(item.label)
            .size(label_size)
            .font(ui_font(item_weight))
            .color(label_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
        text(item.detail)
            .size(detail_size)
            .font(ui_font(item_weight))
            .color(detail_color)
            .wrapping(Wrapping::None),
    ]
    .spacing(item_layout.spacing.unwrap_or(Spacing::MD))
    .align_y(iced::Alignment::Center);

    if item.enabled {
        button(content)
            .height(item_layout.height.unwrap_or(item_height))
            .width(Length::Fill)
            .padding([
                item_layout.padding_y(Spacing::XS),
                item_layout.padding_x(Spacing::MD),
            ])
            .on_press(Message::ContextMenuActionSelected(item.action))
            .style(move |_, status| {
                crate::style::button_style(tokens, Class::ContextMenuItem, status)
            })
            .into()
    } else {
        container(content)
            .height(item_layout.height.unwrap_or(item_height))
            .width(Length::Fill)
            .padding([
                item_layout.padding_y(Spacing::XS),
                item_layout.padding_x(Spacing::MD),
            ])
            .style(move |_| {
                let disabled_style = tokens.class_styles[Class::ContextMenuItem.index()]
                    .resolve(ComponentState::Disabled);
                container_style(tokens, Class::ContextMenuItem).with_visual_override(disabled_style)
            })
            .into()
    }
}

fn context_menu_separator(tokens: ThemeTokens) -> Element<'static, Message> {
    container("")
        .height(tokens.primitives.context_menu_separator_height)
        .width(Length::Fill)
        .style(move |_| {
            let selected_style = tokens.class_styles[Class::ContextMenuPanel.index()]
                .resolve(ComponentState::Selected);
            container_style(tokens, Class::ContextMenuPanel).with_visual_override(selected_style)
        })
        .into()
}

fn class_text_color(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
    fallback: iced::Color,
) -> iced::Color {
    tokens.class_styles[class.index()]
        .resolve(state)
        .text_color
        .unwrap_or(fallback)
}
