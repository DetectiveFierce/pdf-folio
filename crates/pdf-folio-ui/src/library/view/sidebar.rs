use super::*;
use iced::widget::column;

pub(crate) fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let sidebar_width = app.library.library_tag_sidebar_width;
    let sidebar_body = if let Some(entry) = app.primary_selected_entry() {
        view_selected_pdf_sidebar(app, entry, sidebar_width, tokens)
    } else if !app.library.selected_library_entries.is_empty() {
        view_multi_selection_sidebar(app, sidebar_width, tokens)
    } else if app.library.folder_details_sidebar_open {
        if let Some(folder) = app.details_folder().cloned() {
            view_selected_folder_sidebar(app, folder, sidebar_width, tokens)
        } else {
            view_library_navigation_sidebar(app, sidebar_width, tokens)
        }
    } else {
        view_library_navigation_sidebar(app, sidebar_width, tokens)
    };

    let sidebar = container(sidebar_body)
        .width(sidebar_width)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::Sidebar));

    let handle_color = if app.library.resizing_library_tag_sidebar {
        tokens.focus
    } else {
        tokens.border
    };
    let handle_visual_width = if app.library.resizing_library_tag_sidebar {
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
    .on_press(Message::BeginTagSidebarResize)
    .on_release(Message::EndTagSidebarResize)
    .interaction(mouse::Interaction::ResizingHorizontally);

    row![sidebar, resize_handle].height(Length::Fill).into()
}

pub(crate) fn view_library_navigation_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let heading = container(
        row![
            section_heading("Explorer", tokens).width(Length::Fill),
            sidebar_chevron_button(
                CHEVRON_LEFT_SVG,
                "Collapse Sidebar",
                Message::CollapseLibrarySidebar,
                tokens,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding(Spacing::MD);

    let sidebar_tab_component = tokens.class_styles[Class::SidebarTab.index()];
    let sidebar_tab_layout = sidebar_tab_component.layout;
    let sidebar_tab_style = sidebar_tab_component.resolve(ComponentState::Normal);
    let tab_area_background = sidebar_tab_style
        .background
        .unwrap_or_else(|| sidebar_tab_area_background(tokens));
    let file_tree_component = tokens.class_styles[Class::FileTree.index()];
    let file_tree_layout = file_tree_component.layout;
    let file_tree_style = file_tree_component.resolve(ComponentState::Normal);
    let content_background = file_tree_style
        .background
        .or_else(|| {
            sidebar_tab_component
                .resolve(ComponentState::Active)
                .background
        })
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let tabs = container(
        row![
            sidebar_tab_button(
                LibrarySidebarTab::Files,
                app.library.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
            sidebar_tab_button(
                LibrarySidebarTab::Tags,
                app.library.library_sidebar_tab,
                tokens,
                app.labels(),
            ),
        ]
        .spacing(sidebar_tab_layout.spacing.unwrap_or(Spacing::XS))
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(iced::Padding {
        top: sidebar_tab_layout.margin_top(Spacing::XS),
        right: sidebar_tab_layout.margin_right(Spacing::SM),
        bottom: sidebar_tab_layout.margin_bottom(Spacing::XS),
        left: sidebar_tab_layout.margin_left(Spacing::SM),
    })
    .style(move |_| {
        let mut style = container_style(tokens, Class::Sidebar);
        style.background = Some(iced::Background::Color(tab_area_background));
        style.border.width = 0.0;
        style
    });

    let body = match app.library.library_sidebar_tab {
        LibrarySidebarTab::Files => view_file_tree_sidebar(app, sidebar_width, tokens),
        LibrarySidebarTab::Tags => view_tag_tree_sidebar(app, sidebar_width, tokens),
    };

    let body_scroll = scrollable(body)
        .direction(sidebar_scroll_direction())
        .height(Length::Fill)
        .style(move |_, status| sidebar_scrollable_style(tokens, status));

    let padded_body = container(body_scroll)
        .height(Length::Fill)
        .padding(iced::Padding {
            top: file_tree_layout.padding_top(0.0),
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

    let tabbed_body = container(column![tabs, padded_body].spacing(0).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::FileTree);
            if file_tree_style.background.is_none() {
                style.background = Some(iced::Background::Color(content_background));
            }
            style
        });

    let mut content = column![heading].spacing(Spacing::SM).height(Length::Fill);
    content = content.push(tabbed_body);

    container(content).height(Length::Fill).into()
}

pub(crate) fn sidebar_tab_button<'a>(
    tab: LibrarySidebarTab,
    active_tab: LibrarySidebarTab,
    tokens: ThemeTokens,
    labels: &'a crate::style::AppLabelTokens,
) -> iced::widget::Button<'a, Message> {
    let active = tab == active_tab;
    let component = tokens.class_styles[Class::SidebarTab.index()];
    let layout = component.layout;
    let text_style = component.text;
    let normal_style = component.resolve(ComponentState::Normal);
    let active_style = component.resolve(ComponentState::Active);
    button(
        text(library_sidebar_tab_label(labels, tab))
            .size(text_style.size.unwrap_or(FontSize::MD))
            .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(if active {
                active_style.text_color.unwrap_or(tokens.text_primary)
            } else {
                normal_style.text_color.unwrap_or(tokens.text_secondary)
            }),
    )
    .height(layout.height.unwrap_or(30.0))
    .width(Length::FillPortion(layout.width_portion.unwrap_or(1)))
    .padding(iced::Padding {
        top: layout.padding_top(Spacing::XS),
        right: layout.padding_right(Spacing::MD),
        bottom: layout.padding_bottom(Spacing::XS),
        left: layout.padding_left(Spacing::MD),
    })
    .style(move |_, status| {
        let style = crate::style::button_style(tokens, Class::SidebarTab, status);
        let state = if active {
            ComponentState::Active
        } else {
            match status {
                iced::widget::button::Status::Active => ComponentState::Normal,
                iced::widget::button::Status::Hovered => ComponentState::Hovered,
                iced::widget::button::Status::Pressed => ComponentState::Pressed,
                iced::widget::button::Status::Disabled => ComponentState::Disabled,
            }
        };
        let state_style = component.resolve(state);
        style.with_visual_override(state_style)
    })
    .on_press(Message::LibrarySidebarTabChanged(tab))
}

pub(crate) fn sidebar_tab_area_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, Color::BLACK, 0.34)
    } else {
        mix_color(tokens.surface_raised, Color::BLACK, 0.09)
    }
}

pub(crate) fn sidebar_tab_content_background(tokens: ThemeTokens) -> Color {
    if is_dark_surface(tokens.surface) {
        mix_color(tokens.surface, tokens.surface_raised, 0.62)
    } else {
        tokens.surface
    }
}

pub(crate) fn is_dark_surface(color: Color) -> bool {
    color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722 < 0.5
}

pub(crate) fn view_file_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let library_counts = app.folder_smart_counts(None);
    let root_row = file_tree_row(
        "Library",
        Some(folder_sidebar_count_label(library_counts)),
        0,
        app.library.selected_folder.is_none() && app.library.details_folder_id.is_none(),
        true,
        app.library.library_tree_root_expanded,
        Message::ToggleLibraryTreeRoot,
        Message::FolderTreeClicked(None),
        sidebar_width,
        tokens,
        false,
    );
    let mut tree = column![mouse_area(root_row)
        .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::Folder(None),)),]
    .spacing(0);

    if app.library.library_tree_root_expanded {
        tree = tree.push(folder_sidebar_rows(app, None, 1, sidebar_width, tokens));
    }

    tree.into()
}

pub(crate) fn selected_folder_actions_panel<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let folder = app.selected_folder()?;
    let parent_id = folder.parent_id.clone();
    let has_parent = parent_id.is_some();
    let has_grandparent = parent_id.as_ref().is_some_and(|parent_id| {
        app.library
            .library_folders
            .iter()
            .find(|candidate| &candidate.id == parent_id)
            .and_then(|parent| parent.parent_id.as_ref())
            .is_some()
    });
    let can_move_earlier = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, _, index)| index > 0);
    let can_move_later = app
        .selected_folder_sibling_order()
        .is_some_and(|(_, folder_ids, index)| index + 1 < folder_ids.len());
    let input_width = (sidebar_width - Spacing::XL * 2.0).max(80.0);
    let mut actions = column![
        text("Folder")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(sidebar_folder_card_title_color(tokens)),
        text_input("Folder name", &app.library.folder_rename_input)
            .on_input(Message::FolderRenameInputChanged)
            .on_submit(Message::RenameSelectedFolder)
            .id(Id::new(LIBRARY_FOLDER_RENAME_INPUT_ID))
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(input_width),
        row![
            sidebar_folder_action_button("Rename", tokens).on_press(Message::RenameSelectedFolder),
            sidebar_folder_action_button("Delete", tokens)
                .on_press(Message::RequestDeleteSelectedFolder),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
        row![
            maybe_sidebar_folder_action_button(
                "Earlier",
                tokens,
                can_move_earlier,
                Message::MoveSelectedFolderEarlier,
            ),
            maybe_sidebar_folder_action_button(
                "Later",
                tokens,
                can_move_later,
                Message::MoveSelectedFolderLater,
            ),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::SM);

    if has_parent {
        actions = actions.push(
            sidebar_folder_action_button("Move to root", tokens)
                .on_press(Message::MoveSelectedFolderToRoot)
                .width(Length::Fill),
        );
    }
    if has_grandparent {
        actions = actions.push(
            sidebar_folder_action_button("Move up", tokens)
                .on_press(Message::MoveSelectedFolderUp)
                .width(Length::Fill),
        );
    }

    Some(
        container(actions)
            .width(Length::Fill)
            .padding(Spacing::MD)
            .style(move |_| container_style(tokens, Class::SidebarFolderCard))
            .into(),
    )
}

pub(crate) fn view_selected_folder_sidebar<'a>(
    app: &'a PDFolioApp,
    folder: Folder,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let counts = app.folder_smart_counts(Some(&folder.id));
    let child_count = app
        .library
        .library_folders
        .iter()
        .filter(|child| child.parent_id.as_ref() == Some(&folder.id))
        .count();
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("Folder Details", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let mut content = column![
        heading,
        text(truncate_for_width(&folder.name, details_width, 0.0))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens))
            .wrapping(Wrapping::None),
        sidebar_detail_row("PDFs", counts.total.to_string(), details_width, tokens),
        sidebar_detail_row("Folders", child_count.to_string(), details_width, tokens),
        sidebar_detail_row(
            "Reading",
            counts.in_progress.to_string(),
            details_width,
            tokens
        ),
        sidebar_detail_row("Missing", counts.missing.to_string(), details_width, tokens),
        sidebar_action_button("Open folder", tokens)
            .on_press(Message::FolderSelected(Some(folder.id.clone()))),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    if let Some(panel) = selected_folder_actions_panel(app, sidebar_width, tokens) {
        content = content.push(panel);
    }

    content = content.push(
        sidebar_action_button("Clear selection", tokens).on_press(Message::FolderClicked(None)),
    );

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction())
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

pub(crate) fn view_tag_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let all_tags = app.all_tags();
    let mut tags = column![
        file_tree_row(
            "All tags",
            Some(format_count(app.library.library_entries.len(), "PDF")),
            0,
            app.library.active_tag_filter.is_none(),
            !all_tags.is_empty(),
            true,
            Message::TagFilterChanged(None),
            Message::TagFilterChanged(None),
            sidebar_width,
            tokens,
            false,
        ),
        section_heading("Tags", tokens),
    ]
    .spacing(Spacing::SM);

    for tag in all_tags {
        let count = app
            .library
            .library_entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == &tag))
            .count();
        let active = app.library.active_tag_filter.as_ref() == Some(&tag);
        tags = tags.push(file_tree_row(
            tag.clone(),
            Some(format_count(count, "PDF")),
            1,
            active,
            false,
            false,
            Message::TagFilterChanged(Some(tag.clone())),
            Message::TagFilterChanged(Some(tag)),
            sidebar_width,
            tokens,
            false,
        ));
    }

    tags.into()
}

pub(crate) fn view_selected_pdf_sidebar<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let title = entry_title(&entry);
    let author = entry_author(&entry);
    let path_label = entry
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown file");
    let folder_label = if entry.folders.is_empty() {
        String::from("No folders")
    } else {
        entry
            .folders
            .iter()
            .map(|folder| folder.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let tags_label = if entry.tags.is_empty() {
        String::from("No tags")
    } else {
        entry.tags.join(", ")
    };
    let progress_label = selected_pdf_progress_label(&entry);
    let status_label = if entry.missing {
        "Missing file"
    } else {
        "Available"
    };
    let duplicate_label = duplicate_status_label(app, &entry);
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("PDF Details", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let mut content = column![
        heading,
        thumbnail_element(app, &entry.id, tokens, details_width.min(160.0), 1.0),
        text(truncate_for_width(&title, details_width, 0.0))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens))
            .wrapping(Wrapping::None),
        text(truncate_for_width(&author, details_width, 0.0))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(sidebar_detail_secondary_color(tokens))
            .wrapping(Wrapping::None),
        sidebar_detail_row("Status", status_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Pages", page_count_label(&entry), details_width, tokens),
        sidebar_detail_row("Progress", progress_label, details_width, tokens),
        sidebar_detail_row("Size", file_size_label(&entry), details_width, tokens),
        sidebar_detail_row("Duplicates", duplicate_label, details_width, tokens),
        sidebar_detail_row("Opened", last_opened_label(&entry), details_width, tokens),
        sidebar_detail_row(
            "Added",
            format!("Added {}", entry.added_at.format("%b %-d, %Y")),
            details_width,
            tokens
        ),
        sidebar_detail_row("File", path_label.to_owned(), details_width, tokens),
        sidebar_detail_row("Folders", folder_label, details_width, tokens),
        sidebar_detail_row("Tags", tags_label, details_width, tokens),
        sidebar_action_button("Open PDF", tokens)
            .on_press(Message::OpenLibraryEntry(entry.id.clone())),
        sidebar_action_button("Reveal in file manager", tokens)
            .on_press(Message::RevealEntryInFileManager(entry.id.clone())),
        sidebar_action_button("Open containing folder", tokens)
            .on_press(Message::OpenEntryContainingFolder(entry.id.clone())),
    ];
    if entry.missing {
        content = content.push(
            sidebar_action_button("Relink missing file", tokens)
                .on_press(Message::RelinkMissingEntry(entry.id.clone())),
        );
    }
    let content = content
        .push(
            sidebar_action_button("Clear selection", tokens)
                .on_press(Message::ClearLibrarySelection),
        )
        .spacing(Spacing::SM)
        .padding(Spacing::MD);

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction())
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

fn sidebar_scroll_direction() -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(4.0)
            .scroller_width(2.0)
            .anchor(Anchor::End),
    )
}

pub(crate) fn view_multi_selection_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let selected_entries = app.selected_entries();
    let selected_count = selected_entries.len();
    let total_pages: u32 = selected_entries
        .iter()
        .filter_map(|entry| entry.page_count.map(u32::from))
        .sum();
    let missing_count = selected_entries
        .iter()
        .filter(|entry| entry.missing)
        .count();
    let total_size_label = total_file_size_label(&selected_entries);
    let details_width = (sidebar_width - Spacing::MD * 2.0).max(80.0);
    let heading = row![
        section_heading("Selection", tokens).width(Length::Fill),
        sidebar_chevron_button(
            CHEVRON_LEFT_SVG,
            "Collapse Sidebar",
            Message::CollapseLibrarySidebar,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let content = column![
        heading,
        text(format_count(selected_count, "PDF"))
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(sidebar_detail_primary_color(tokens)),
        sidebar_detail_row(
            "Known pages",
            if total_pages == 0 {
                String::from("Unknown")
            } else {
                total_pages.to_string()
            },
            details_width,
            tokens,
        ),
        sidebar_detail_row(
            "Missing files",
            missing_count.to_string(),
            details_width,
            tokens,
        ),
        sidebar_detail_row("Total size", total_size_label, details_width, tokens),
        sidebar_action_button("Clear selection", tokens).on_press(Message::ClearLibrarySelection),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(content)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
        .into()
}

pub(crate) fn sidebar_detail_row<'a>(
    label: &'a str,
    value: String,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(sidebar_detail_secondary_color(tokens)),
            text(truncate_for_width(&value, width, 0.0))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::REGULAR))
                .color(sidebar_detail_primary_color(tokens))
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::XS, Spacing::SM])
    .style(move |_| container_style(tokens, Class::SidebarDetailRow))
    .into()
}

pub(crate) fn sidebar_detail_primary_color(tokens: ThemeTokens) -> Color {
    mix_color(tokens.text_secondary, tokens.text_primary, 0.52)
}

pub(crate) fn sidebar_detail_secondary_color(tokens: ThemeTokens) -> Color {
    with_alpha(tokens.text_secondary, 0.88)
}

pub(crate) fn sidebar_folder_card_title_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarFolderCardTitle.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or_else(|| sidebar_detail_secondary_color(tokens))
}

pub(crate) fn folder_sidebar_text_input_style(
    tokens: ThemeTokens,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    text_input_style(tokens, Class::SidebarFolderTextInput, status)
}

pub(crate) fn selected_pdf_progress_label(entry: &LibraryEntry) -> String {
    entry.page_count.map_or_else(
        || format!("Page {}", u32::from(entry.last_page) + 1),
        |page_count| {
            let current_page = entry.last_page.saturating_add(1).min(page_count.max(1));
            format!(
                "{} of {} ({:.0}%)",
                current_page,
                page_count,
                f32::from(current_page) / f32::from(page_count.max(1)) * 100.0
            )
        },
    )
}

pub(crate) fn duplicate_status_label(app: &PDFolioApp, entry: &LibraryEntry) -> String {
    let duplicate_count = app
        .library
        .library_entries
        .iter()
        .filter(|candidate| candidate.id == entry.id)
        .count()
        .saturating_sub(1);
    duplicate_status_label_for_count(duplicate_count)
}

pub(crate) fn duplicate_status_label_for_count(duplicate_count: usize) -> String {
    if duplicate_count == 0 {
        String::from("Unique content hash")
    } else {
        format_count(duplicate_count, "matching duplicate")
    }
}

pub(crate) fn folder_sidebar_rows<'a>(
    app: &'a PDFolioApp,
    parent_id: Option<&'a FolderId>,
    depth: usize,
    sidebar_width: f32,
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
        let expanded = !app
            .library
            .collapsed_library_tree_folders
            .contains(&folder.id);
        let active = app.library.details_folder_id.as_ref() == Some(&folder.id);
        let drop_active = app.active_folder_drop_target() == Some(&folder.id);
        let flash_active = app.folder_drop_flash_active(&folder.id);
        let counts = app.folder_smart_counts(Some(&folder.id));
        let row = file_tree_row(
            &folder.name,
            Some(folder_sidebar_count_label(counts)),
            depth,
            active,
            has_children,
            expanded,
            Message::ToggleLibraryTreeFolder(folder.id.clone()),
            Message::FolderTreeClicked(Some(folder.id.clone())),
            sidebar_width,
            tokens,
            drop_active || flash_active,
        );
        rows = rows.push(
            mouse_area(row)
                .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::Folder(Some(
                    folder.id.clone(),
                ))))
                .on_enter(Message::FolderDropTargetChanged(Some(folder.id.clone())))
                .on_exit(Message::FolderDropTargetChanged(None))
                .on_press(Message::BeginFolderTreeDrag(folder.id.clone()))
                .on_release(Message::EndFolderDrag),
        );
        if expanded {
            rows = rows.push(folder_sidebar_rows(
                app,
                Some(&folder.id),
                depth.saturating_add(1),
                sidebar_width,
                tokens,
            ));
        }
    }

    rows.into()
}

pub(crate) fn file_tree_row<'a>(
    label: impl Into<String>,
    meta: Option<String>,
    depth: usize,
    active: bool,
    has_children: bool,
    expanded: bool,
    toggle_message: Message,
    message: Message,
    sidebar_width: f32,
    tokens: ThemeTokens,
    drop_active: bool,
) -> Element<'a, Message> {
    let label = label.into();
    let file_tree_style = tokens.class_styles[Class::FileTree.index()];
    let fold_button_component = tokens.class_styles[Class::FileTreeFoldButton.index()];
    let fold_button_layout = fold_button_component.layout;
    let fold_button_normal_style = fold_button_component.resolve(ComponentState::Normal);
    let fold_button_hovered_style = fold_button_component.resolve(ComponentState::Hovered);
    let normal_style = file_tree_style.resolve(ComponentState::Normal);
    let active_style = file_tree_style.resolve(ComponentState::Active);
    let content_background = normal_style
        .background
        .unwrap_or_else(|| sidebar_tab_content_background(tokens));
    let indent = (depth as f32 * 12.0).min(72.0);
    let fold_width = fold_button_layout.width.unwrap_or(16.0);
    let meta_width = meta
        .as_ref()
        .map_or(0.0, |value| (value.len() as f32 * 6.0).clamp(52.0, 128.0));
    let row_padding = Spacing::SM * 2.0;
    let row_spacing = Spacing::XS * if meta.is_some() { 3.0 } else { 2.0 };
    let label_width =
        (sidebar_width - row_padding - indent - fold_width - meta_width - row_spacing).max(42.0);
    let text_color = if active || drop_active {
        active_style.text_color.unwrap_or(tokens.text_primary)
    } else {
        normal_style.text_color.unwrap_or(tokens.text_secondary)
    };

    let chevron: Element<'_, Message> = if has_children {
        let icon = Svg::new(iced::widget::svg::Handle::from_memory(if expanded {
            CHEVRON_DOWN_SVG
        } else {
            CHEVRON_RIGHT_SVG
        }))
        .width(13.0)
        .height(13.0)
        .style(move |_, status| iced::widget::svg::Style {
            color: Some(match status {
                iced::widget::svg::Status::Hovered => fold_button_hovered_style
                    .text_color
                    .unwrap_or(tokens.text_primary),
                iced::widget::svg::Status::Idle => fold_button_normal_style
                    .text_color
                    .unwrap_or(tokens.text_secondary),
            }),
        });

        button(
            container(icon)
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill),
        )
        .width(fold_button_layout.width.unwrap_or(16.0))
        .height(fold_button_layout.height.unwrap_or(20.0))
        .padding(fold_button_layout.padding_top(0.0))
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::FileTreeFoldButton, status)
        })
        .on_press(toggle_message)
        .into()
    } else {
        container("").width(16.0).height(20.0).into()
    };

    let mut content = row![
        container("").width(indent),
        chevron,
        text(file_tree_label(&label, label_width))
            .size(FILE_TREE_LABEL_SIZE)
            .line_height(1.12)
            .font(file_tree_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(text_color)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(label_width)),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    if let Some(meta) = meta {
        content = content.push(
            text(meta)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None)
                .width(Length::Fixed(meta_width))
                .align_x(iced::alignment::Horizontal::Right),
        );
    }

    let row_button = button(content)
        .height(FILE_TREE_ROW_HEIGHT)
        .width(Length::Fill)
        .padding([3.0, Spacing::SM])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            let state = if active || drop_active {
                ComponentState::Active
            } else if hovered {
                ComponentState::Hovered
            } else {
                ComponentState::Normal
            };
            let mut style = crate::style::button_style(tokens, Class::FileTree, status);
            apply_file_tree_state_style(&mut style, tokens, state, content_background);
            if drop_active {
                let drop_style = crate::style::button_style(
                    tokens,
                    Class::FolderDropTarget,
                    button::Status::Active,
                );
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .on_press(message);

    if active || drop_active {
        if let Some(border) = side_border_for_class(tokens, Class::FileTree, ComponentState::Active)
        {
            side_border(row_button, border)
        } else {
            row_button.into()
        }
    } else {
        row_button.into()
    }
}

pub(crate) fn apply_file_tree_state_style(
    style: &mut button::Style,
    tokens: ThemeTokens,
    state: ComponentState,
    fallback_background: Color,
) {
    let state_style = tokens.class_styles[Class::FileTree.index()].resolve(state);
    let fallback_style = crate::style::tokens::VisualStyle {
        background: Some(state_style.background.unwrap_or(fallback_background)),
        ..state_style
    };
    *style = style.with_visual_override(fallback_style);
}

pub(crate) fn sidebar_chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    chevron_button(icon, tooltip_label, message, tokens, false)
}

pub(crate) fn chevron_button<'a>(
    icon: &'static [u8],
    tooltip_label: &'a str,
    message: Message,
    tokens: ThemeTokens,
    transparent: bool,
) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(icon))
        .width(18.0)
        .height(18.0)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(28.0)
    .height(28.0)
    .padding(0)
    .style(move |_, status| {
        let mut style = crate::style::button_style(tokens, Class::SidebarToggleButton, status);
        if transparent {
            style.background = None;
            style.border.width = 0.0;
            style.shadow = iced::Shadow::default();
        }
        style
    })
    .on_press(message);

    tooltip(
        button,
        container(
            text(tooltip_label)
                .size(FontSize::SM)
                .color(tokens.text_primary),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

pub(crate) fn sidebar_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_detail_primary_color(tokens)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| crate::style::button_style(tokens, Class::SidebarActionButton, status))
}

pub(crate) fn sidebar_folder_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_folder_action_text_color(tokens, true)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SidebarFolderActionButton, status)
    })
}

pub(crate) fn maybe_sidebar_folder_action_button<'a>(
    label: impl Into<String>,
    tokens: ThemeTokens,
    enabled: bool,
    message: Message,
) -> iced::widget::Button<'a, Message> {
    let button = button(
        text(label.into())
            .size(FontSize::MD)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_folder_action_text_color(tokens, enabled)),
    )
    .padding([Spacing::SM, Spacing::LG])
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SidebarFolderActionButton, status)
    });

    if enabled {
        button.on_press(message)
    } else {
        button
    }
}

pub(crate) fn sidebar_folder_action_text_color(tokens: ThemeTokens, enabled: bool) -> Color {
    if enabled {
        tokens.class_styles[Class::SidebarFolderActionButton.index()]
            .resolve(ComponentState::Normal)
            .text_color
            .unwrap_or_else(|| sidebar_detail_primary_color(tokens))
    } else {
        tokens.class_styles[Class::SidebarFolderActionButton.index()]
            .resolve(ComponentState::Disabled)
            .text_color
            .unwrap_or_else(|| with_alpha(tokens.text_secondary, 0.42))
    }
}
