use super::*;
use iced::widget::column;

pub(crate) fn view_confirmation_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let Some(action) = app.chrome.pending_confirmation.as_ref() else {
        return container("").into();
    };
    let (title, body, confirm_label) = confirmation_copy(action, app);
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
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
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

pub(crate) fn bulk_operation_progress_banner<'a>(
    app: &'a PDFolioApp,
    progress: &'a BulkOperationProgress,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let elapsed = app
        .library
        .animation_now
        .saturating_duration_since(progress.started_at)
        .as_secs_f32();
    let value = indeterminate_progress_value(elapsed);
    let label = format!("{} {} PDFs...", progress.label, progress.total);

    container(
        column![
            row![
                text(label)
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::SEMIBOLD))
                    .color(tokens.text_primary),
                text("Working in background")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            ]
            .spacing(Spacing::MD)
            .align_y(iced::Alignment::Center),
            progress_bar(value, tokens),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

pub(crate) fn indeterminate_progress_value(elapsed_secs: f32) -> f32 {
    let sweep = (elapsed_secs * 0.72).fract();
    (0.18 + 0.64 * (0.5 - (sweep - 0.5).abs()) * 2.0).clamp(0.0, 1.0)
}

pub(crate) fn view_create_folder_dialog(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let parent = app
        .selected_folder_name()
        .unwrap_or_else(|| String::from("Library"));
    let dialog = column![
        text("New Folder")
            .size(FontSize::HEADING)
            .color(tokens.text_primary),
        text(format!("Create a folder in {parent}."))
            .size(FontSize::MD)
            .color(tokens.text_secondary),
        text_input("Folder name", &app.library.new_folder_name)
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
            .width(420.0)
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

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
            "Delete from library?",
            format!(
                "This removes library metadata for {} selected PDFs. The PDF files remain on disk.",
                app.library.selected_library_entries.len()
            ),
            "Delete",
        ),
        ConfirmationAction::ResetDetailsMetadata(_) => (
            "Reset PDF details?",
            String::from("This clears the edited display title and author for this PDF."),
            "Reset",
        ),
        ConfirmationAction::DeleteFolder(folder_id) => (
            "Delete folder?",
            format!(
                "This removes the folder \"{}\" and any nested folders. PDFs remain in the library and on disk.",
                app.library.library_folders
                    .iter()
                    .find(|folder| &folder.id == folder_id)
                    .map_or("Selected folder", |folder| folder.name.as_str())
            ),
            "Delete",
        ),
    }
}
