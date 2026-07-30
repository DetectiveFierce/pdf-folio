//! # Library modal dialogs
//!
//! View builders for blocking/overlay dialogs used in library mode: confirm
//! destructive actions, import menus, create folder, import review, export,
//! tag manager, move picker, and Raindrop connect/import flows.
//!
//! ## Ownership
//!
//! Presentation only: reads dialog state from `PDFolioApp` / chrome and emits
//! `Message`s. Business logic for the confirmed actions lives in
//! `crate::library::{update, tasks, actions}`.
//!
//! Most dialogs share `modal_container` styling; confirmation copy is
//! centralized in [`confirmation_copy`].

use crate::components::library::cards::{document_preview_lines, ghost_tags_row};
use crate::library::view::*;
use crate::shell::commands::{command_message, command_visible, CommandId, CommandSurface};
use crate::*;
use iced::widget::{column, row, scrollable};
use pdf_folio_cloud::raindrop::RaindropImportDestination;

const RAINDROP_INTEGRATIONS_URL: &str = "https://app.raindrop.io/settings/integrations";
const FOLDER_PLUS_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 10v6"/><path d="M9 13h6"/><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>"##;

/// Generic yes/cancel confirmation overlay; special-cases folder delete.
pub(crate) fn view_confirmation_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let Some(action) = app.chrome.pending_confirmation.as_ref() else {
        return container("").into();
    };
    if let ConfirmationAction::DeleteFolder(folder_id) = action {
        return view_delete_folder_confirmation_dialog(app, folder_id, tokens);
    }
    let (title, body, confirm_label) = confirmation_copy(action, app);
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text(body).size(FontSize::MD).color(tokens.text_secondary),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelConfirmation),
            toolbar_button(confirm_label, tokens).on_press(Message::ConfirmPendingAction),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("ConfirmationDialog", "width", 420.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

/// Rich confirmation for moving a folder tree to trash (counts + suppress checkbox).
pub(crate) fn view_delete_folder_confirmation_dialog<'a>(
    app: &'a PDFolioApp,
    folder_id: &'a FolderId,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let folder_name = app
        .library
        .library_folders
        .iter()
        .find(|folder| &folder.id == folder_id)
        .map_or("Selected folder", |folder| folder.name.as_str());
    let pdf_count = folder_delete_entry_count(app, folder_id);
    let nested_folder_count = folder_delete_nested_folder_count(app, folder_id);
    let warning_panel = container(
        column![
            text("This moves the folder tree to the Trash Can.")
                .size(FontSize::MD)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary),
            text(format!(
                "{} and {} in this folder tree can be restored from the Trash Can. Files on disk will not be deleted.",
                format_count(pdf_count, "PDF"),
                format_count(nested_folder_count, "nested folder")
            ))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding(Spacing::MD)
    .style(move |_| container_style(tokens, Class::ErrorBanner));

    let counts = row![
        delete_folder_count_card("PDFs", pdf_count, tokens),
        delete_folder_count_card("Nested folders", nested_folder_count, tokens),
    ]
    .spacing(Spacing::SM)
    .width(Length::Fill);

    let dialog = column![
        column![
            text("Move Folder to Trash")
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary),
            text(truncate_for_width(folder_name, 460.0, 0.0))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::XS),
        warning_panel,
        counts,
        checkbox(app.chrome.folder_delete_skip_warning_checked)
            .label("Do not show this warning again")
            .on_toggle(Message::FolderDeleteWarningSuppressionToggled)
            .size(FontSize::MD)
            .text_size(FontSize::MD),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelConfirmation),
            toolbar_button("Move to Trash", tokens).on_press(Message::ConfirmPendingAction),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::LG)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(
                app.layout()
                    .metric("DeleteFolderConfirmationDialog", "width", 520.0),
            )
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

/// Chooser for PDF / folder / Raindrop import entry points.
pub(crate) fn view_import_menu_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let mut actions = column![].spacing(Spacing::SM);
    if command_visible(app, CommandId::ImportPdf, CommandSurface::ImportMenu) {
        actions = actions.push(toolbar_button("Import PDFs", tokens).on_press(
            command_message(app, CommandId::ImportPdf).unwrap_or(Message::ImportPdfDialog),
        ));
    }
    if command_visible(app, CommandId::ImportFolder, CommandSurface::ImportMenu) {
        actions = actions.push(toolbar_button("Import Folder", tokens).on_press(
            command_message(app, CommandId::ImportFolder).unwrap_or(Message::ImportFolderDialog),
        ));
    }
    if command_visible(app, CommandId::ImportRaindrop, CommandSurface::ImportMenu) {
        actions = actions.push(toolbar_button("Import from Raindrop", tokens).on_press(
            command_message(app, CommandId::ImportRaindrop).unwrap_or(Message::ImportRaindrop),
        ));
    }

    let dialog = column![
        text("Import")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        actions,
        row![toolbar_button("Cancel", tokens).on_press(Message::CloseImportMenu)]
            .spacing(Spacing::SM)
            .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    modal_container(app, tokens, dialog, "ImportMenuDialog", 360.0)
}

fn delete_folder_count_card<'a>(
    label: &'a str,
    count: usize,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        column![
            text(count.to_string())
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary),
            text(label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding(Spacing::MD)
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

/// Name input modal for creating a folder under the current parent.
pub(crate) fn view_create_folder_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let parent = app
        .selected_folder_name()
        .unwrap_or_else(|| String::from("Library"));
    let dialog = column![
        text("New Folder")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text(format!("Create a folder in {parent}."))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        text_input("Folder name", &app.library.new_folder_name)
            .id(Id::new(LIBRARY_CREATE_FOLDER_INPUT_ID))
            .on_input(Message::NewFolderNameChanged)
            .on_submit(Message::CreateFolder)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
            toolbar_button("Create", tokens).on_press(Message::CreateFolder),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("CreateFolderDialog", "width", 420.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

/// Post-import summary with optional tagging of newly imported entries.
pub(crate) fn view_import_review_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let Some(review) = app.library.import_review.as_ref() else {
        return container("").into();
    };
    let mut details = column![
        sidebar_detail_row(
            "Imported",
            format_count(review.imported_count, "new PDF"),
            360.0,
            tokens
        ),
        sidebar_detail_row(
            "Duplicates",
            format_count(review.duplicate_count, "PDF"),
            360.0,
            tokens
        ),
        sidebar_detail_row(
            "Failed",
            format_count(review.failed_count, "PDF"),
            360.0,
            tokens
        ),
        sidebar_detail_row(
            "Destination",
            review.destination_label.clone(),
            360.0,
            tokens
        ),
    ]
    .spacing(Spacing::SM);
    if !review.errors.is_empty() {
        details = details.push(
            container(
                text(review.errors.join("\n"))
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            )
            .padding(Spacing::SM)
            .style(move |_| container_style(tokens, Class::ErrorBanner)),
        );
    }

    let dialog = column![
        text(&review.title)
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text("Review the PDFs that just finished importing.")
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(tokens.text_secondary),
        details,
        text_input(
            "Tags to add to selected imported PDFs",
            &app.library.inspector_tag_input
        )
        .on_input(Message::InspectorTagInputChanged)
        .on_submit(Message::InspectorAddTag)
        .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
        .width(Length::Fill),
        row![
            toolbar_button("Select imported", tokens).on_press(Message::SelectImportReviewEntries),
            toolbar_button("Add tag", tokens).on_press(Message::InspectorAddTag),
            toolbar_button("Move", tokens).on_press(Message::OpenMoveSelectionDialog),
            toolbar_button("Done", tokens).on_press(Message::CloseImportReview),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    modal_container(app, tokens, dialog, "ImportReviewDialog", 500.0)
}

/// Configure destination, naming, and options for exporting library PDFs.
pub(crate) fn view_export_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    if let Some(summary) = app.library.last_export_summary.as_ref() {
        let dialog = column![
            text("Export Complete")
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary),
            text(format!(
                "Exported {} PDFs to {}.",
                summary.exported,
                summary.destination.display()
            ))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
            row![
                toolbar_button("Reveal", tokens).on_press(Message::RevealExportedFolder),
                toolbar_button("Copy path", tokens).on_press(Message::CopyExportPath),
                toolbar_button("Close", tokens).on_press(Message::CloseExportDialog),
            ]
            .spacing(Spacing::SM)
            .align_y(iced::Alignment::Center),
        ]
        .spacing(Spacing::MD)
        .padding(Spacing::LG);
        return modal_container(app, tokens, dialog, "ExportDialog", 520.0);
    }
    if let Some(progress) = app.library.export_progress.as_ref() {
        let elapsed = app
            .library
            .animation_now
            .saturating_duration_since(progress.started_at)
            .as_secs_f32();
        let dialog = column![
            text(&progress.label)
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary),
            text(format!("Preparing {} PDFs.", progress.total))
                .size(FontSize::MD)
                .color(tokens.text_secondary),
            progress_bar(indeterminate_progress_value(elapsed), tokens),
        ]
        .spacing(Spacing::MD)
        .padding(Spacing::LG);
        return modal_container(app, tokens, dialog, "ExportDialog", 520.0);
    }
    let Some(dialog_state) = app.library.export_dialog.as_ref() else {
        return container("").into();
    };

    let destination = dialog_state
        .destination
        .as_ref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| String::from("Choose a destination folder"));
    let dialog = column![
        text("Export PDFs")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        sidebar_detail_row("Destination", destination, 430.0, tokens),
        row![
            toolbar_button("Choose folder", tokens).on_press(Message::ChooseExportDestination),
            export_choice_button(
                "Flat",
                dialog_state.mode == ExportMode::CopyFlat,
                Message::ExportModeChanged(ExportMode::CopyFlat),
                tokens
            ),
            export_choice_button(
                "Folders",
                dialog_state.mode == ExportMode::PreserveFolders,
                Message::ExportModeChanged(ExportMode::PreserveFolders),
                tokens
            ),
            export_choice_button(
                "ZIP",
                dialog_state.mode == ExportMode::Zip,
                Message::ExportModeChanged(ExportMode::Zip),
                tokens
            ),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
        row![
            export_choice_button(
                "Original",
                dialog_state.filename_template == ExportFilenameTemplate::OriginalFilename,
                Message::ExportFilenameTemplateChanged(ExportFilenameTemplate::OriginalFilename),
                tokens
            ),
            export_choice_button(
                "Title",
                dialog_state.filename_template == ExportFilenameTemplate::Title,
                Message::ExportFilenameTemplateChanged(ExportFilenameTemplate::Title),
                tokens
            ),
            export_choice_button(
                "Author-title",
                dialog_state.filename_template == ExportFilenameTemplate::AuthorTitle,
                Message::ExportFilenameTemplateChanged(ExportFilenameTemplate::AuthorTitle),
                tokens
            ),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
        row![
            checkbox(dialog_state.include_metadata_csv)
                .label("metadata.csv")
                .on_toggle(Message::ExportMetadataCsvToggled)
                .text_size(FontSize::SM),
            checkbox(dialog_state.include_metadata_json)
                .label("metadata.json")
                .on_toggle(Message::ExportMetadataJsonToggled)
                .text_size(FontSize::SM),
        ]
        .spacing(Spacing::SM),
        row![
            checkbox(dialog_state.include_tags)
                .label("tags")
                .on_toggle(Message::ExportTagsToggled)
                .text_size(FontSize::SM),
            checkbox(dialog_state.include_reading_progress)
                .label("reading progress")
                .on_toggle(Message::ExportReadingProgressToggled)
                .text_size(FontSize::SM),
        ]
        .spacing(Spacing::SM),
        row![
            export_choice_button(
                "Skip",
                dialog_state.conflict_behavior == ExportConflictBehavior::Skip,
                Message::ExportConflictBehaviorChanged(ExportConflictBehavior::Skip),
                tokens
            ),
            export_choice_button(
                "Overwrite",
                dialog_state.conflict_behavior == ExportConflictBehavior::Overwrite,
                Message::ExportConflictBehaviorChanged(ExportConflictBehavior::Overwrite),
                tokens
            ),
            export_choice_button(
                "Keep both",
                dialog_state.conflict_behavior == ExportConflictBehavior::KeepBoth,
                Message::ExportConflictBehaviorChanged(ExportConflictBehavior::KeepBoth),
                tokens
            ),
        ]
        .spacing(Spacing::SM),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseExportDialog),
            toolbar_button("Export", tokens).on_press(Message::StartExport),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    modal_container(app, tokens, dialog, "ExportDialog", 560.0)
}

/// Rename/delete tags library-wide.
pub(crate) fn view_tag_manager_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let query = app.library.tag_manager_filter.trim().to_lowercase();
    let destination = app.library.tag_manager_merge_destination.trim().to_owned();
    let mut list = column![].spacing(Spacing::XS);
    for tag in app
        .all_tags()
        .into_iter()
        .filter(|tag| query.is_empty() || tag.to_lowercase().contains(&query))
        .take(40)
    {
        let count = app
            .library
            .library_entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == &tag))
            .count();
        let mut row = row![
            text(tag.clone())
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .width(Length::Fill),
            text(format_count(count, "PDF"))
                .size(FontSize::SM)
                .color(tokens.text_secondary),
            toolbar_button("Rename", tokens).on_press(Message::StartTagRename(tag.clone())),
            toolbar_button("Delete", tokens).on_press(Message::RequestConfirmation(
                ConfirmationAction::DeleteTag(tag.clone()),
            )),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center);
        if !destination.is_empty() && destination != tag {
            row = row.push(toolbar_button("Merge", tokens).on_press(Message::MergeTag {
                source: tag.clone(),
                destination: destination.clone(),
            }));
        }
        list = list.push(
            container(row)
                .padding(Spacing::SM)
                .style(move |_| container_style(tokens, Class::SidebarDetailRow)),
        );
    }

    let dialog = column![
        text("Tag Manager")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text_input("Filter tags", &app.library.tag_manager_filter)
            .on_input(Message::TagManagerFilterChanged)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        text_input(
            "Merge destination tag",
            &app.library.tag_manager_merge_destination
        )
        .on_input(Message::TagManagerMergeDestinationChanged)
        .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
        .width(Length::Fill),
        scrollable(list).height(
            app.layout()
                .metric("TagManagerDialog", "list_height", 360.0)
        ),
        row![
            toolbar_button("Close", tokens).on_press(Message::CloseTagManager),
            toolbar_button("Clear filter", tokens)
                .on_press(Message::TagManagerFilterChanged(String::new())),
            toolbar_button("Clear merge", tokens)
                .on_press(Message::TagManagerMergeDestinationChanged(String::new())),
        ]
        .spacing(Spacing::SM),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    modal_container(app, tokens, dialog, "TagManagerDialog", 640.0)
}

fn export_choice_button<'a>(
    label: &'a str,
    active: bool,
    message: Message,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(if active {
                tokens.text_primary
            } else {
                tokens.text_secondary
            })
            .wrapping(Wrapping::None),
    )
    .padding([Spacing::SM, Spacing::MD])
    .on_press(message)
    .style(move |_, status| button_style(tokens, Class::ToolbarButton, status))
}

fn modal_container<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
    dialog: iced::widget::Column<'a, Message>,
    metric_group: &'static str,
    fallback_width: f32,
) -> Element<'a, Message> {
    container(
        container(dialog)
            .width(app.layout().metric(metric_group, "width", fallback_width))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

/// Folder tree picker for moving or adding entries to a destination folder.
pub(crate) fn view_library_move_picker_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let Some(picker) = app.library.move_picker.as_ref() else {
        return container("").into();
    };
    let selected_count = app.library.selected_library_entries.len();
    let title = match &picker.target {
        LibraryMoveTarget::SelectedEntries => {
            format!("Move {}", format_count(selected_count, "PDF"))
        }
        LibraryMoveTarget::Folder(folder_id) => app
            .library
            .library_folders
            .iter()
            .find(|folder| &folder.id == folder_id)
            .map_or_else(
                || String::from("Move Folder"),
                |folder| format!("Move {}", truncate_for_width(&folder.name, 360.0, 0.0)),
            ),
    };
    let destination = picker
        .selected_destination
        .as_ref()
        .and_then(|folder_id| {
            app.library
                .library_folders
                .iter()
                .find(|folder| &folder.id == folder_id)
        })
        .map_or_else(|| String::from("Library"), |folder| folder.name.clone());
    let tree_width = app
        .layout()
        .metric("LibraryMovePickerDialog", "tree_width", 480.0);
    let tree = container(
        scrollable(view_move_picker_tree(app, picker, tree_width, tokens))
            .direction(sidebar_scroll_direction(tokens))
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .height(
        app.layout()
            .metric("LibraryMovePickerDialog", "tree_height", 330.0),
    )
    .width(Length::Fill)
    .style(move |_| container_style(tokens, Class::FileTree));

    let dialog = column![
        column![
            text(title)
                .size(FontSize::HEADING)
                .font(display_font(FontWeight::MEDIUM))
                .color(tokens.text_primary),
            text(format!("Destination: {destination}"))
                .size(FontSize::MD)
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS),
        tree,
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelMovePicker),
            container("").width(Length::Fill),
            toolbar_button("Select", tokens).on_press(Message::ConfirmMovePicker),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(
                app.layout()
                    .metric("LibraryMovePickerDialog", "width", 540.0),
            )
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

fn view_move_picker_tree<'a>(
    app: &'a PDFolioApp,
    picker: &'a LibraryMovePicker,
    tree_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let library_counts = app.normal_folder_smart_counts(None);
    let root_row = file_tree_row(
        "Library",
        Some(folder_sidebar_count_label(library_counts)),
        0,
        picker.selected_destination.is_none(),
        true,
        true,
        Message::ToggleLibraryTreeRoot,
        Message::MovePickerDestinationSelected(None),
        tree_width,
        tokens,
        false,
    );
    column![
        root_row,
        view_move_picker_folder_rows(app, picker, None, 1, tree_width, tokens)
    ]
    .spacing(0)
    .into()
}

fn view_move_picker_folder_rows<'a>(
    app: &'a PDFolioApp,
    picker: &'a LibraryMovePicker,
    parent_id: Option<&'a FolderId>,
    depth: usize,
    tree_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut rows = column![].spacing(0);
    let mut children: Vec<&Folder> = app
        .library
        .library_folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == parent_id)
        .collect();
    children.sort_by_key(|folder| (folder.manual_order, folder.name.to_lowercase()));

    for folder in children {
        let has_children = app
            .library
            .library_folders
            .iter()
            .any(|child| child.parent_id.as_ref() == Some(&folder.id));
        let expanded = picker.expanded_folders.contains(&folder.id);
        let active = picker.selected_destination.as_ref() == Some(&folder.id);
        let invalid = match &picker.target {
            LibraryMoveTarget::SelectedEntries => false,
            LibraryMoveTarget::Folder(folder_id) => {
                &folder.id == folder_id
                    || !folder_can_move_into(&app.library.library_folders, folder_id, &folder.id)
            }
        };
        let counts = app.normal_folder_smart_counts(Some(&folder.id));
        let row = file_tree_row(
            &folder.name,
            Some(if invalid {
                String::from("Unavailable")
            } else {
                folder_sidebar_count_label(counts)
            }),
            depth,
            active,
            has_children,
            expanded,
            Message::ToggleMovePickerFolder(folder.id.clone()),
            Message::MovePickerDestinationSelected(Some(folder.id.clone())),
            tree_width,
            tokens,
            false,
        );
        rows = rows.push(row);
        if expanded {
            rows = rows.push(view_move_picker_folder_rows(
                app,
                picker,
                Some(&folder.id),
                depth.saturating_add(1),
                tree_width,
                tokens,
            ));
        }
    }

    rows.into()
}

/// Prompt to connect a Raindrop.io token / open integrations settings.
pub(crate) fn view_raindrop_connect_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let can_submit = !app.library.raindrop_client_id_input.trim().is_empty()
        && !app.library.raindrop_client_secret_input.trim().is_empty();
    let sign_in_button = if can_submit {
        toolbar_button("Sign in", tokens).on_press(Message::SubmitRaindropSignIn)
    } else {
        toolbar_button("Sign in", tokens)
    };
    let copy_status = if app.library.raindrop_callback_copied {
        "Callback url copied to clipboard!"
    } else {
        "Click to copy"
    };

    let dialog = column![
        text("Connect Raindrop.io")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text("Create a small Raindrop developer app once, then paste its credentials here. PDF-Folio will open your browser so you can sign in and authorize access.")
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        toolbar_button("Open Raindrop Integrations", tokens)
            .on_press(Message::OpenRaindropIntegrations),
        text(format!(
            "1. Open Raindrop Integrations: {RAINDROP_INTEGRATIONS_URL}\n2. In For Developers, choose Create new app.\n3. Use these values:\n   Name: PDF-Folio\n   Description: Import my Raindrop PDF files into PDF-Folio.\n   Site: https://github.com/pdf-folio/pdf-folio\n   Redirect URI: {callback}\n4. Save the app in Raindrop.\n5. Copy the Client ID and Client Secret from Raindrop into the fields below.\n6. Click Sign in.\n\nIf Raindrop says \"Incorrect redirect_uri\", replace the Redirect URI in Raindrop with the exact value below and save the app again.",
            callback = pdf_folio_cloud::raindrop::OAUTH_CALLBACK_URL
        ))
        .size(FontSize::SM)
        .color(tokens.text_secondary),
        row![
            text("Redirect URI:")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary),
            toolbar_button(pdf_folio_cloud::raindrop::OAUTH_CALLBACK_URL, tokens)
                .on_press(Message::CopyRaindropCallbackUrl),
            text(copy_status)
                .size(FontSize::SM)
                .color(if app.library.raindrop_callback_copied {
                    tokens.text_primary
                } else {
                    tokens.text_secondary
                }),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
        text_input("Client ID", &app.library.raindrop_client_id_input)
            .on_input(Message::RaindropClientIdChanged)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        text_input("Client Secret", &app.library.raindrop_client_secret_input)
            .on_input(Message::RaindropClientSecretChanged)
            .on_submit(Message::SubmitRaindropSignIn)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
            sign_in_button,
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("RaindropConnectDialog", "width", 560.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

/// Configure and start a Raindrop collection import (destination + structure).
pub(crate) fn view_raindrop_import_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let selected_count = app.library.selected_raindrop_pdf_ids.len();
    let new_folder_ready = !app.library.raindrop_import_new_folder_active
        || !app
            .library
            .raindrop_import_new_folder_name
            .trim()
            .is_empty();
    let can_import = selected_count > 0 && new_folder_ready;
    let import_button = if can_import {
        toolbar_button("Import", tokens).on_press(Message::ImportSelectedRaindropPdfs)
    } else {
        toolbar_button("Import", tokens)
    };

    let pdf_count = app
        .library
        .raindrop_import_preview
        .as_ref()
        .map_or(0, |preview| preview.pdfs.len());
    let account_label = app
        .library
        .raindrop_import_preview
        .as_ref()
        .map_or("Raindrop.io", |preview| preview.account_label.as_str());

    let pdf_panel: Element<'_, Message> =
        if let Some(preview) = app.library.raindrop_import_preview.as_ref() {
            let mut rows = column![].spacing(Spacing::SM);
            for pdf in &preview.pdfs {
                rows = rows.push(raindrop_pdf_row(app, pdf, tokens));
            }
            container(
                scrollable(rows)
                    .height(
                        app.layout()
                            .metric("RaindropImportDialog", "pdf_panel_height", 360.0),
                    )
                    .style(move |_, status| scrollable_style(tokens, Class::LibraryRow, status)),
            )
            .width(Length::Fill)
            .height(
                app.layout()
                    .metric("RaindropImportDialog", "pdf_panel_height", 360.0),
            )
            .padding(Spacing::SM)
            .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
            .into()
        } else {
            container(
                column![
                    text("Loading PDFs from Raindrop.io")
                        .size(FontSize::MD)
                        .font(ui_font(FontWeight::SEMIBOLD))
                        .color(tokens.text_primary),
                    text("Fetching your remote PDF list and preview images.")
                        .size(FontSize::SM)
                        .color(tokens.text_secondary),
                    container(progress_bar(0.42, tokens)).width(Length::Fill),
                ]
                .spacing(Spacing::MD),
            )
            .width(Length::Fill)
            .height(
                app.layout()
                    .metric("RaindropImportDialog", "pdf_panel_height", 360.0),
            )
            .center(Length::Fill)
            .padding(Spacing::LG)
            .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
            .into()
        };

    let dialog = column![
        row![
            column![
                text("Import from Raindrop.io")
                    .size(FontSize::HEADING)
                    .font(display_font(FontWeight::MEDIUM))
                    .color(tokens.text_primary),
                text(if app.library.raindrop_import_preview.is_some() {
                    format!(
                        "{} found in {}. All PDFs are selected by default.",
                        format_count(pdf_count, "PDF"),
                        account_label
                    )
                } else {
                    format!("Preparing import from {account_label}.")
                })
                .size(FontSize::MD)
                .color(tokens.text_secondary),
            ]
            .spacing(Spacing::XS)
            .width(Length::Fill),
            text(format_count(selected_count, "PDF"))
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::LG)
        .align_y(iced::Alignment::Center),
        row![
            toolbar_button("Select all", tokens).on_press(Message::SelectAllRaindropPdfs),
            toolbar_button("Select none", tokens).on_press(Message::ClearAllRaindropPdfs),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
        pdf_panel,
        row![
            column![
                text("Import destination")
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text("Choose where selected Raindrop PDFs should land.")
                    .size(FontSize::SM)
                    .color(tokens.text_secondary),
            ]
            .spacing(Spacing::XS)
            .width(Length::Fill),
            raindrop_import_location_selector(app, tokens),
        ]
        .spacing(Spacing::LG)
        .align_y(iced::Alignment::Start),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CloseOverlay),
            import_button,
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("RaindropImportDialog", "width", 820.0))
            .height(app.layout().metric("RaindropImportDialog", "height", 660.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

fn raindrop_import_location_selector<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let preserve_structure =
        raindrop_import_preserves_structure(&app.library.raindrop_import_destination);
    let root_folder = raindrop_import_root_folder(&app.library.raindrop_import_destination);
    let selected_label = if app.library.raindrop_import_new_folder_active {
        String::from("New folder")
    } else {
        raindrop_import_root_label(app, root_folder.as_ref())
    };

    let mut content = column![
        checkbox(preserve_structure)
            .label("Preserve Raindrop folder structure")
            .on_toggle(Message::RaindropPreserveFolderStructureToggled)
            .size(FontSize::MD)
            .text_size(FontSize::MD),
        toolbar_button(format!("Import location: {selected_label}"), tokens)
            .on_press(Message::ToggleRaindropImportLocationMenu)
            .width(Length::Fill),
    ]
    .spacing(Spacing::SM)
    .width(
        app.layout()
            .metric("RaindropImportLocationSelector", "width", 320.0),
    );

    if app.library.raindrop_import_location_menu_open {
        content = content.push(raindrop_import_location_menu(
            app,
            root_folder.as_ref(),
            tokens,
        ));
    }

    if app.library.raindrop_import_new_folder_active {
        content = content.push(
            column![
                text("New Folder Name:")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text_input("Folder name", &app.library.raindrop_import_new_folder_name)
                    .on_input(Message::RaindropImportNewFolderNameChanged)
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(Length::Fill),
            ]
            .spacing(Spacing::XS),
        );
    }

    container(content)
        .width(
            app.layout()
                .metric("RaindropImportLocationSelector", "width", 320.0),
        )
        .into()
}

fn raindrop_import_location_menu<'a>(
    app: &'a PDFolioApp,
    selected_folder: Option<&FolderId>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut rows = column![raindrop_import_location_row(
        "Library",
        0,
        !app.library.raindrop_import_new_folder_active && selected_folder.is_none(),
        false,
        false,
        Message::RaindropImportRootChanged(None),
        None,
        tokens,
    )]
    .spacing(Spacing::XS);

    rows = rows.push(raindrop_import_folder_rows(
        app,
        None,
        selected_folder,
        0,
        tokens,
    ));
    rows = rows.push(raindrop_new_folder_row(tokens));

    container(
        scrollable(rows)
            .height(
                app.layout()
                    .metric("RaindropImportLocationSelector", "menu_height", 220.0),
            )
            .style(move |_, status| scrollable_style(tokens, Class::LibraryRow, status)),
    )
    .width(Length::Fill)
    .padding(Spacing::XS)
    .style(move |_| container_style(tokens, Class::MenuPanel))
    .into()
}

fn raindrop_import_folder_rows<'a>(
    app: &'a PDFolioApp,
    parent_id: Option<&FolderId>,
    selected_folder: Option<&FolderId>,
    depth: usize,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut folders = app
        .library
        .library_folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == parent_id)
        .collect::<Vec<_>>();
    folders.sort_by(|a, b| {
        a.manual_order
            .cmp(&b.manual_order)
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut rows = column![].spacing(Spacing::XS);
    for folder in folders {
        let has_children = app
            .library
            .library_folders
            .iter()
            .any(|child| child.parent_id.as_ref() == Some(&folder.id));
        let expanded = app
            .library
            .expanded_raindrop_import_location_folders
            .contains(&folder.id);
        rows = rows.push(raindrop_import_location_row(
            &folder.name,
            depth + 1,
            selected_folder == Some(&folder.id),
            has_children,
            expanded,
            Message::RaindropImportRootChanged(Some(folder.id.clone())),
            has_children.then(|| Message::ToggleRaindropImportLocationFolder(folder.id.clone())),
            tokens,
        ));
        if has_children && expanded {
            rows = rows.push(raindrop_import_folder_rows(
                app,
                Some(&folder.id),
                selected_folder,
                depth + 1,
                tokens,
            ));
        }
    }

    rows.into()
}

fn raindrop_import_location_row<'a>(
    label: &'a str,
    depth: usize,
    selected: bool,
    has_children: bool,
    expanded: bool,
    select_message: Message,
    toggle_message: Option<Message>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let chevron = if has_children {
        if expanded {
            "v"
        } else {
            ">"
        }
    } else {
        ""
    };
    let mut row_content = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    row_content = row_content.push(
        container("").width(
            (depth as f32 * tokens.primitives.raindrop_tree_indent_width)
                .min(tokens.primitives.raindrop_tree_max_indent),
        ),
    );
    let fold_control: Element<'a, Message> = if has_children {
        button(
            text(chevron)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_secondary),
        )
        .padding(
            tokens.class_styles[Class::FileTreeFoldButton.index()]
                .layout
                .padding_top(0.0),
        )
        .width(tokens.primitives.raindrop_tree_fold_width)
        .on_press(toggle_message.unwrap_or_else(|| select_message.clone()))
        .style(move |_, status| button_style(tokens, Class::FileTreeFoldButton, status))
        .into()
    } else {
        container("")
            .width(tokens.primitives.raindrop_tree_fold_width)
            .into()
    };
    row_content = row_content.push(fold_control);
    row_content = row_content.push(
        text(label)
            .size(FontSize::SM)
            .font(ui_font(if selected {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::REGULAR
            }))
            .color(if selected {
                tokens.text_primary
            } else {
                tokens.text_secondary
            })
            .width(Length::Fill),
    );

    button(
        container(row_content)
            .padding([tokens.primitives.raindrop_tree_row_padding_y, Spacing::XS]),
    )
    .width(Length::Fill)
    .on_press(select_message)
    .style(move |_, status| button_style(tokens, Class::MenuItem, status))
    .into()
}

fn raindrop_new_folder_row<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(FOLDER_PLUS_SVG))
        .width(tokens.primitives.raindrop_new_folder_icon_size)
        .height(tokens.primitives.raindrop_new_folder_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_primary),
        });
    let content = row![
        icon,
        text("New folder")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(tokens.text_primary)
    ]
    .spacing(Spacing::SM)
    .padding([tokens.primitives.raindrop_tree_row_padding_y, Spacing::XS])
    .align_y(iced::Alignment::Center);

    button(content)
        .width(Length::Fill)
        .on_press(Message::StartNewRaindropImportFolder)
        .style(move |_, status| button_style(tokens, Class::MenuItem, status))
        .into()
}

fn raindrop_import_root_label(app: &PDFolioApp, root_folder: Option<&FolderId>) -> String {
    root_folder
        .and_then(|folder_id| {
            app.library
                .library_folders
                .iter()
                .find(|folder| &folder.id == folder_id)
        })
        .map_or_else(|| String::from("Library"), |folder| folder.name.clone())
}

fn raindrop_import_preserves_structure(destination: &RaindropImportDestination) -> bool {
    matches!(
        destination,
        RaindropImportDestination::PreserveRaindropFolders
            | RaindropImportDestination::PreserveRaindropFoldersUnder(_)
    )
}

fn raindrop_import_root_folder(destination: &RaindropImportDestination) -> Option<FolderId> {
    match destination {
        RaindropImportDestination::PreserveRaindropFoldersUnder(folder_id) => folder_id.clone(),
        RaindropImportDestination::LocalFolder(folder_id) => Some(folder_id.clone()),
        RaindropImportDestination::PreserveRaindropFolders
        | RaindropImportDestination::LibraryRoot => None,
    }
}

fn raindrop_pdf_row<'a>(
    app: &'a PDFolioApp,
    pdf: &'a RaindropPdfCandidate,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let id = pdf.id;
    let selected = app.library.selected_raindrop_pdf_ids.contains(&id);
    let mut meta = Vec::new();
    if let Some(file_name) = pdf.file_name.as_deref().filter(|name| *name != pdf.title) {
        meta.push(file_name.to_owned());
    }
    if let Some(collection) = pdf.collection_title.as_deref() {
        meta.push(collection.to_owned());
    }
    if let Some(file_size) = pdf.file_size {
        meta.push(format_remote_file_size(file_size));
    }

    let tag_row = if pdf.tags.is_empty() {
        text("No tags")
            .size(FontSize::SM)
            .color(tokens.text_secondary)
            .into()
    } else {
        ghost_tags_row(pdf.tags.clone(), tokens, 1.0)
    };

    let details = column![
        text(truncate_for_width_with_font(
            &pdf.title,
            500.0,
            0.0,
            FontSize::MD
        ))
        .size(FontSize::MD)
        .font(ui_font(FontWeight::SEMIBOLD))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None),
        text(if meta.is_empty() {
            String::from("Remote PDF")
        } else {
            meta.join(" . ")
        })
        .size(FontSize::SM)
        .color(tokens.text_secondary),
        tag_row,
    ]
    .spacing(Spacing::XS)
    .width(Length::Fill);

    let row_content = row![
        selection_checkbox(selected, tokens, Message::RaindropPdfToggled(id, !selected),),
        raindrop_thumbnail(app, pdf, tokens),
        details,
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let surface = container(row_content).width(Length::Fill).style(move |_| {
        library_entry_container_style(
            tokens,
            Class::LibraryRow,
            LibraryEntryRenderMode::Normal,
            selected,
            0.0,
        )
    });

    mouse_area(surface)
        .on_press(Message::RaindropPdfToggled(id, !selected))
        .into()
}

fn raindrop_thumbnail<'a>(
    app: &'a PDFolioApp,
    pdf: &RaindropPdfCandidate,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let width = 54.0;
    let height = 72.0;
    if let Some(handle) = app.library.raindrop_pdf_thumbnails.get(&pdf.id) {
        container(
            image(handle.clone())
                .width(width)
                .height(height)
                .content_fit(ContentFit::Cover),
        )
        .width(width)
        .height(height)
        .clip(true)
        .style(move |_| container_style(tokens, Class::PagePlaceholder))
        .into()
    } else {
        container(document_preview_lines(width, height, tokens, 1.0))
            .width(width)
            .height(height)
            .clip(true)
            .style(move |_| container_style(tokens, Class::PagePlaceholder))
            .into()
    }
}

fn format_remote_file_size(bytes: u64) -> String {
    let kib = bytes as f64 / 1024.0;
    if kib < 1024.0 {
        format!("{kib:.0} KiB")
    } else {
        format!("{:.1} MiB", kib / 1024.0)
    }
}

/// Title, body, and confirm button label for a pending `ConfirmationAction`.
pub(crate) fn confirmation_copy<'a>(
    action: &'a ConfirmationAction,
    app: &'a PDFolioApp,
) -> (&'a str, String, &'a str) {
    match action {
        ConfirmationAction::BulkResetDisplayMetadata => (
            "Reset metadata?",
            format!(
                "This will clear display title and author edits for {} selected PDFs.",
                app.library.selected_library_entries.len()
            ),
            "Reset",
        ),
        ConfirmationAction::BulkDeleteFromLibrary => (
            "Move to trash?",
            format!(
                "This moves {} selected PDFs to the Trash Can. PDF files on disk remain where they are.",
                app.library.selected_library_entries.len()
            ),
            "Move to Trash",
        ),
        ConfirmationAction::PermanentlyDeleteFromTrash => (
            "Permanently delete?",
            format!(
                "This permanently removes metadata for {} selected PDFs from the Trash Can. PDF files on disk remain where they are.",
                app.library.selected_library_entries.len()
            ),
            "Delete",
        ),
        ConfirmationAction::PermanentlyDeleteFolderFromTrash(folder_id) => (
            "Permanently delete folder?",
            format!(
                "This permanently removes the folder \"{}\", any nested folders, and {} in this folder tree from the Trash Can. Files on disk remain where they are.",
                app.library
                    .library_trash_folders
                    .iter()
                    .find(|folder| &folder.id == folder_id)
                    .map_or("Selected folder", |folder| folder.name.as_str()),
                format_count(folder_delete_entry_count(app, folder_id), "PDF")
            ),
            "Delete",
        ),
        ConfirmationAction::ResetDetailsMetadata(_) => (
            "Reset PDF details?",
            String::from("This clears the edited display title and author for this PDF."),
            "Reset",
        ),
        ConfirmationAction::DeleteFolder(folder_id) => (
            "Move folder to trash?",
            format!(
                "This moves the folder \"{}\", any nested folders, and {} in this folder tree to the Trash Can. Files on disk will not be deleted.",
                app.library.library_folders
                    .iter()
                    .find(|folder| &folder.id == folder_id)
                    .map_or("Selected folder", |folder| folder.name.as_str()),
                format_count(folder_delete_entry_count(app, folder_id), "PDF")
            ),
            "Move to Trash",
        ),
        ConfirmationAction::DeleteTag(tag) => (
            "Delete tag?",
            format!(
                "This will remove the tag \"{tag}\" from all files and remove it from the tag menu."
            ),
            "Delete Tag",
        ),
        ConfirmationAction::DeleteLibrary(library_id) => (
            "Delete library?",
            format!(
                "This permanently deletes the \"{}\" library database. PDF files on disk remain where they are.",
                app.libraries
                    .profiles
                    .iter()
                    .find(|profile| &profile.id == library_id)
                    .map_or("Selected library", |profile| profile.name.as_str())
            ),
            "Delete",
        ),
    }
}

fn folder_delete_entry_count(app: &PDFolioApp, folder_id: &FolderId) -> usize {
    let folder_ids = app.folder_subtree_ids(folder_id);
    app.library
        .library_entries
        .iter()
        .filter(|entry| {
            entry
                .folders
                .iter()
                .any(|folder| folder_ids.contains(&folder.id))
        })
        .count()
}

fn folder_delete_nested_folder_count(app: &PDFolioApp, folder_id: &FolderId) -> usize {
    app.folder_subtree_ids(folder_id).len().saturating_sub(1)
}
