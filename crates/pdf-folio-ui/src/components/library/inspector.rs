use crate::library::view::*;
use crate::*;
use iced::widget::{column, row};

pub(crate) fn library_inspector_visible(app: &PDFolioApp) -> bool {
    app.mode == AppMode::Library && app.library.library_inspector_open
}

pub(crate) fn view_library_inspector(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let width = app.library.library_inspector_width.clamp(
        app.layout().metric("LibraryInspector", "min_width", 260.0),
        app.layout().metric("LibraryInspector", "max_width", 520.0),
    );

    let body = if let Some(entry) = app.primary_selected_entry() {
        view_selected_pdf_sidebar(app, entry, width, tokens)
    } else if !app.library.selected_library_entries.is_empty() {
        view_multi_selection_sidebar(app, width, tokens)
    } else if let Some(folder) = app.details_folder().cloned() {
        view_selected_folder_sidebar(app, folder, width, tokens)
    } else if let Some(tag) = app.library.active_tag_filter.as_ref() {
        view_tag_inspector(app, tag, width, tokens)
    } else {
        view_library_summary_inspector(app, width, tokens)
    };

    let inspector = container(body)
        .width(width)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::Sidebar));

    let handle_color = if app.library.resizing_library_inspector {
        tokens.focus
    } else {
        tokens.border
    };
    let handle_visual_width = if app.library.resizing_library_inspector {
        app.layout().sidebar_resize_handle_width
    } else {
        app.layout().sidebar_resize_handle_visual_width
    };
    let resize_handle = mouse_area(
        container(
            container("")
                .width(handle_visual_width)
                .height(Length::Fill)
                .style(move |_| {
                    let mut style = container_style(tokens, Class::Sidebar);
                    style.background = Some(iced::Background::Color(handle_color));
                    style
                }),
        )
        .width(app.layout().sidebar_resize_handle_width)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center),
    )
    .on_press(Message::BeginLibraryInspectorResize)
    .on_release(Message::EndLibraryInspectorResize)
    .interaction(mouse::Interaction::ResizingHorizontally);

    row![resize_handle, inspector].height(Length::Fill).into()
}

fn view_library_summary_inspector(
    app: &PDFolioApp,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let pdf_count = app.library.library_entries.len();
    let tag_count = app.all_tags().len();
    let missing_count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.missing)
        .count();
    let unfiled_count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.folders.is_empty())
        .count();
    let details_width = (width - Spacing::MD * 2.0).max(80.0);
    let content = column![
        section_heading("Library Summary", tokens),
        text(app.active_library_name())
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens)),
        sidebar_detail_row("PDFs", pdf_count.to_string(), details_width, tokens),
        sidebar_detail_row("Tags", tag_count.to_string(), details_width, tokens),
        sidebar_detail_row("Unfiled", unfiled_count.to_string(), details_width, tokens),
        sidebar_detail_row("Missing", missing_count.to_string(), details_width, tokens),
        sidebar_action_button("Import PDFs", tokens).on_press(Message::ImportPdfDialog),
        sidebar_action_button("Create Folder", tokens).on_press(Message::OpenCreateFolderDialog),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction(tokens))
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

fn view_tag_inspector<'a>(
    app: &'a PDFolioApp,
    tag: &'a str,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == tag))
        .count();
    let details_width = (width - Spacing::MD * 2.0).max(80.0);
    let content = column![
        section_heading("Tag", tokens),
        text(truncate_for_width(tag, details_width, 0.0))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens))
            .wrapping(Wrapping::None),
        sidebar_detail_row("PDFs", count.to_string(), details_width, tokens),
        sidebar_action_button("Show tagged PDFs", tokens)
            .on_press(Message::TagFilterChanged(Some(tag.to_owned()))),
        sidebar_action_button("Export tagged PDFs", tokens)
            .on_press(Message::OpenExportDialog(ExportSource::Tag(tag.to_owned()))),
        sidebar_action_button("Rename tag", tokens)
            .on_press(Message::StartTagRename(tag.to_owned())),
        sidebar_action_button("Tag Manager", tokens).on_press(Message::OpenTagManager),
        sidebar_action_button("Delete tag", tokens).on_press(Message::RequestConfirmation(
            ConfirmationAction::DeleteTag(tag.to_owned()),
        )),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction(tokens))
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}
