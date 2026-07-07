use super::*;
use iced::widget::column;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};

const FILE_TREE_CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M6.25 4.25 10 8l-3.75 3.75"/></svg>"##;
const FILE_TREE_CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M4.25 6.25 8 10l3.75-3.75"/></svg>"##;
const LIBRARIES_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"><rect x="1.5" y="9" width="5" height="6" rx="1"/><rect x="9.5" y="9" width="5" height="6" rx="1"/><rect x="17.5" y="9" width="5" height="6" rx="1"/></svg>"##;
const ACTIVE_TRASH_CAN_CONTENT_OFFSET: f32 = 4.0;

pub(crate) fn view_library_tag_sidebar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let sidebar_width = app.library.library_tag_sidebar_width;
    let sidebar_body = view_library_navigation_sidebar(app, sidebar_width, tokens);

    let sidebar_content = column![
        container(sidebar_body).height(Length::Fill),
        library_switcher_sidebar_button(app, tokens),
    ]
    .spacing(0)
    .height(Length::Fill);

    let sidebar = container(sidebar_content)
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

fn library_switcher_sidebar_button(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(LIBRARIES_SVG))
        .width(tokens.primitives.library_switcher_sidebar_icon_size)
        .height(tokens.primitives.library_switcher_sidebar_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });
    let active_name = app.active_library_name().to_owned();
    let button = button(
        row![
            container(icon)
                .width(tokens.primitives.library_switcher_sidebar_icon_slot)
                .height(tokens.primitives.library_switcher_sidebar_icon_slot)
                .center(Length::Fill),
            text(truncate_for_width(
                &active_name,
                tokens.primitives.library_switcher_sidebar_text_width,
                0.0
            ))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Shrink)
    .height(tokens.primitives.library_switcher_sidebar_button_height)
    .padding([0.0, Spacing::XS])
    .style(move |_, status| button_style(tokens, Class::SidebarToggleButton, status))
    .on_press(Message::OpenLibrarySwitcher);

    container(
        tooltip(
            button,
            container(
                text("Switch Library")
                    .size(FontSize::SM)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(tokens.text_primary),
            )
            .padding(Spacing::SM)
            .style(move |_| container_style(tokens, Class::Tooltip)),
            tooltip::Position::Right,
        )
        .delay(Duration::from_millis(500)),
    )
    .width(Length::Fill)
    .padding(iced::Padding {
        top: Spacing::SM,
        right: Spacing::SM,
        bottom: Spacing::SM,
        left: Spacing::SM,
    })
    .into()
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

    let file_tree_component = tokens.class_styles[Class::FileTree.index()];
    let file_tree_layout = file_tree_component.layout;
    let file_tree_style = file_tree_component.resolve(ComponentState::Normal);
    let content_background = file_tree_style.background.unwrap_or(tokens.surface);
    let body = view_stacked_library_navigation_sidebar(app, sidebar_width, tokens);

    let body_scroll = scrollable(body)
        .direction(sidebar_scroll_direction(tokens))
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

    let tabbed_body = container(padded_body)
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

fn view_stacked_library_navigation_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let all_active = !app.library.trash_view_active
        && app.library.selected_folder.is_none()
        && app.library.details_folder_id.is_none()
        && app.library.active_tag_filter.is_none()
        && app.library.active_reading_filter.is_none()
        && !app.library.active_recently_opened_filter
        && !app.library.missing_filter_active
        && app.library.search_query.trim().is_empty();
    let recently_opened_count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.opened_at.is_some())
        .count();
    let unfiled_count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.folders.is_empty())
        .count();
    let missing_count = app
        .library
        .library_entries
        .iter()
        .filter(|entry| entry.missing)
        .count();

    let library_section = column![
        sidebar_section_heading("Library", tokens),
        file_tree_row(
            "All PDFs",
            Some(format_count(app.library.library_entries.len(), "PDF")),
            0,
            all_active,
            false,
            false,
            Message::ClearLibraryFilters,
            Message::ClearLibraryFilters,
            sidebar_width,
            tokens,
            false,
        ),
        file_tree_row(
            "Recently Opened",
            Some(format_count(recently_opened_count, "PDF")),
            0,
            app.library.active_recently_opened_filter,
            false,
            false,
            Message::RecentlyOpenedFilterChanged(true),
            Message::RecentlyOpenedFilterChanged(true),
            sidebar_width,
            tokens,
            false,
        ),
        file_tree_row(
            "Unfiled",
            Some(format_count(unfiled_count, "PDF")),
            0,
            !app.library.trash_view_active
                && app.library.selected_folder.is_none()
                && app.library.details_folder_id.is_none()
                && app.library.active_tag_filter.is_none()
                && app.library.active_reading_filter.is_none()
                && !app.library.active_recently_opened_filter
                && !app.library.missing_filter_active
                && !all_active,
            false,
            false,
            Message::FolderSelected(None),
            Message::FolderSelected(None),
            sidebar_width,
            tokens,
            false,
        ),
        file_tree_row(
            "Missing",
            Some(format_count(missing_count, "PDF")),
            0,
            app.library.missing_filter_active,
            false,
            false,
            Message::MissingFilterChanged(!app.library.missing_filter_active),
            Message::MissingFilterChanged(!app.library.missing_filter_active),
            sidebar_width,
            tokens,
            false,
        ),
        trash_can_sidebar_row(app, sidebar_width, tokens),
    ]
    .spacing(0);

    let mut content = column![
        library_section,
        sidebar_section_heading_with_toggle(
            "Folders",
            app.library.library_tree_root_expanded,
            Message::ToggleLibraryTreeRoot,
            tokens,
        ),
    ]
    .spacing(Spacing::SM);

    if app.library.library_tree_root_expanded {
        content = content.push(view_file_tree_sidebar(app, sidebar_width, tokens));
    }

    content = content.push(sidebar_section_heading_with_toggle(
        "Tags",
        app.library.library_tags_expanded,
        Message::ToggleLibraryTags,
        tokens,
    ));
    if app.library.library_tags_expanded {
        content = content.push(view_tag_tree_sidebar(app, sidebar_width, tokens));
    }

    content
        .padding(iced::Padding {
            top: Spacing::SM,
            right: 0.0,
            bottom: Spacing::MD,
            left: 0.0,
        })
        .into()
}

fn sidebar_section_heading(label: &str, tokens: ThemeTokens) -> Element<'_, Message> {
    container(section_heading(label, tokens))
        .padding(iced::Padding {
            top: Spacing::SM,
            right: Spacing::SM,
            bottom: Spacing::XS,
            left: Spacing::SM,
        })
        .into()
}

fn sidebar_section_heading_with_toggle(
    label: &str,
    expanded: bool,
    toggle_message: Message,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    container(
        row![
            section_heading(label, tokens),
            file_tree_fold_button(expanded, toggle_message, tokens),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding(iced::Padding {
        top: Spacing::SM,
        right: Spacing::SM,
        bottom: Spacing::XS,
        left: Spacing::SM,
    })
    .into()
}

pub(crate) fn view_file_tree_sidebar<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    folder_sidebar_rows(app, None, 0, sidebar_width, tokens)
}

pub(crate) fn trash_can_sidebar_row<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let count = app.library.library_trash_entries.len() + app.library.library_trash_folders.len();
    let file_tree_style = tokens.class_styles[Class::FileTree.index()];
    let normal_style = file_tree_style.resolve(ComponentState::Normal);
    let active_style = file_tree_style.resolve(ComponentState::Active);
    let content_background = normal_style.background.unwrap_or(tokens.surface);
    let active = app.library.trash_view_active;
    let text_color = if active {
        active_style.text_color.unwrap_or(tokens.text_primary)
    } else {
        normal_style.text_color.unwrap_or(tokens.text_secondary)
    };
    let label_size = file_tree_style.text.size.unwrap_or(FontSize::MD);
    let row_height = file_tree_style.layout.height.unwrap_or(26.0);
    let meta = format_count(count, "item");
    let meta_width = (meta.len() as f32 * tokens.primitives.file_tree_meta_char_width).clamp(
        tokens.primitives.file_tree_meta_min_width,
        tokens.primitives.file_tree_meta_max_width,
    );
    let icon_slot = 16.0;
    let active_content_offset = if active {
        ACTIVE_TRASH_CAN_CONTENT_OFFSET
    } else {
        0.0
    };
    let label_width = (sidebar_width
        - Spacing::SM * 2.0
        - icon_slot
        - meta_width
        - active_content_offset
        - Spacing::XS * 3.0)
        .max(42.0);
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(TRASH_CAN_SVG))
        .width(Length::Fixed(icon_slot))
        .height(Length::Fixed(icon_slot))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(text_color),
        });
    let content = row![
        container("").width(active_content_offset),
        icon,
        text(file_tree_label("Trash Can", label_width, label_size))
            .size(label_size)
            .line_height(1.12)
            .font(file_tree_font(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            }))
            .color(text_color)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(label_width)),
        text(meta)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(meta_width))
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let row_button = button(content)
        .height(row_height)
        .width(Length::Fill)
        .padding([tokens.primitives.file_tree_row_padding_y, Spacing::SM])
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            let state = if active {
                ComponentState::Active
            } else if hovered {
                ComponentState::Hovered
            } else {
                ComponentState::Normal
            };
            let mut style = crate::style::button_style(tokens, Class::FileTree, status);
            apply_file_tree_state_style(&mut style, tokens, state, content_background);
            style
        })
        .on_press(Message::OpenTrashCan);

    if active {
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

pub(crate) fn selected_folder_actions_panel<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    if app.library.trash_view_active {
        return None;
    }
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
            sidebar_folder_action_button("Trash", tokens)
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
            "Clear selection",
            Message::ClearLibrarySidebarDetails,
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
        sidebar_action_button("Export folder", tokens).on_press(Message::OpenExportDialog(
            ExportSource::Folder(folder.id.clone())
        )),
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
            .direction(sidebar_scroll_direction(tokens))
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
    let mut tags = column![].spacing(Spacing::SM);

    for tag in all_tags {
        let count = app
            .library
            .library_entries
            .iter()
            .filter(|entry| entry.tags.iter().any(|entry_tag| entry_tag == &tag))
            .count();
        let active = app.library.active_tag_filter.as_ref() == Some(&tag);
        if app.library.renaming_tag.as_ref() == Some(&tag) {
            tags = tags.push(tag_rename_row(app, sidebar_width, tokens));
        } else {
            let row = file_tree_row(
                tag.clone(),
                Some(format_count(count, "PDF")),
                0,
                active,
                false,
                false,
                Message::TagFilterChanged(Some(tag.clone())),
                Message::TagTreeClicked(tag.clone()),
                sidebar_width,
                tokens,
                false,
            );
            tags = tags.push(
                mouse_area(row)
                    .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::Tag(tag))),
            );
        }
    }

    tags.into()
}

fn file_tree_fold_button<'a>(
    expanded: bool,
    toggle_message: Message,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    let fold_button_component = tokens.class_styles[Class::FileTreeFoldButton.index()];
    let fold_button_layout = fold_button_component.layout;
    let fold_button_normal_style = fold_button_component.resolve(ComponentState::Normal);
    let fold_button_hovered_style = fold_button_component.resolve(ComponentState::Hovered);
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(if expanded {
        FILE_TREE_CHEVRON_DOWN_SVG
    } else {
        FILE_TREE_CHEVRON_RIGHT_SVG
    }))
    .width(tokens.primitives.sidebar_chevron_icon_size)
    .height(tokens.primitives.sidebar_chevron_icon_size)
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
    .style(move |_, status| crate::style::button_style(tokens, Class::FileTreeFoldButton, status))
    .on_press(toggle_message)
}

fn tag_rename_row<'a>(
    app: &'a PDFolioApp,
    sidebar_width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let fold_button_layout = tokens.class_styles[Class::FileTreeFoldButton.index()].layout;
    let row_height = 34.0;
    let row_padding_y = 2.0;
    let indent = tokens
        .primitives
        .file_tree_indent_width
        .min(tokens.primitives.file_tree_max_indent);
    let input_width = (sidebar_width
        - Spacing::SM * 2.0
        - indent
        - fold_button_layout.width.unwrap_or(16.0)
        - Spacing::XS * 2.0)
        .max(72.0);
    let content = row![
        container("").width(indent),
        container("")
            .width(fold_button_layout.width.unwrap_or(16.0))
            .height(fold_button_layout.height.unwrap_or(20.0)),
        text_input("Tag name", &app.library.tag_rename_input)
            .on_input(Message::TagRenameInputChanged)
            .on_submit(Message::SubmitTagRename)
            .id(Id::new(LIBRARY_TAG_RENAME_INPUT_ID))
            .padding([Spacing::XS, Spacing::MD])
            .size(FontSize::SM)
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(input_width),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    container(content)
        .height(row_height)
        .width(Length::Fill)
        .padding([row_padding_y, Spacing::SM])
        .style(move |_| container_style(tokens, Class::FileTree))
        .into()
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
    let progress_label = selected_pdf_progress_label(&entry);
    let status_label = if app.library.trash_view_active {
        "In Trash"
    } else if entry.missing {
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
            "Clear selection",
            Message::ClearLibrarySidebarDetails,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let mut content = column![
        heading,
        thumbnail_element(app, &entry.id, tokens, details_width.min(160.0), 1.0),
        text_input("Title", &app.library.details_title_input)
            .on_input(Message::DetailsTitleChanged)
            .on_submit(Message::SaveDetailsMetadata)
            .id(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID))
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(Length::Fill),
        text_input("Author", &app.library.details_author_input)
            .on_input(Message::DetailsAuthorChanged)
            .on_submit(Message::SaveDetailsMetadata)
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(Length::Fill),
        row![
            sidebar_action_button("Save", tokens).on_press(Message::SaveDetailsMetadata),
            sidebar_action_button("Reset", tokens).on_press(Message::RequestConfirmation(
                ConfirmationAction::ResetDetailsMetadata(entry.id.clone()),
            )),
        ]
        .spacing(Spacing::XS),
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
        inspector_tag_editor(app, Some(entry.clone()), details_width, tokens),
        sidebar_action_button("Open PDF", tokens)
            .on_press(Message::OpenLibraryEntry(entry.id.clone())),
        sidebar_action_button("Export PDF", tokens).on_press(Message::OpenExportDialog(
            ExportSource::SingleEntry(entry.id.clone(),)
        )),
        sidebar_action_button("Reveal in file manager", tokens)
            .on_press(Message::RevealEntryInFileManager(entry.id.clone())),
        sidebar_action_button("Open containing folder", tokens)
            .on_press(Message::OpenEntryContainingFolder(entry.id.clone())),
        sidebar_action_button("Copy file path", tokens)
            .on_press(Message::CopyEntryFilePath(entry.id.clone())),
        sidebar_action_button("Move to folder", tokens).on_press(Message::OpenMoveSelectionDialog),
        sidebar_action_button("Refresh metadata", tokens).on_press(Message::BulkRefreshPdfMetadata),
        sidebar_action_button("Rebuild thumbnail", tokens).on_press(Message::BulkRebuildThumbnails),
        sidebar_action_button("Reindex full text", tokens).on_press(Message::BulkReindex),
    ];
    if entry.missing && !app.library.trash_view_active {
        content = content.push(
            sidebar_action_button("Relink missing file", tokens)
                .on_press(Message::RelinkMissingEntry(entry.id.clone())),
        );
    }
    let content = content
        .push(sidebar_action_button("Move to Trash", tokens).on_press(
            Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary),
        ))
        .push(
            sidebar_action_button("Clear selection", tokens)
                .on_press(Message::ClearLibrarySelection),
        )
        .spacing(Spacing::SM)
        .padding(Spacing::MD);

    container(
        scrollable(content)
            .direction(sidebar_scroll_direction(tokens))
            .height(Length::Fill)
            .style(move |_, status| sidebar_scrollable_style(tokens, status)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
    .into()
}

pub(crate) fn sidebar_scroll_direction(tokens: ThemeTokens) -> Direction {
    Direction::Vertical(
        Scrollbar::new()
            .width(tokens.primitives.sidebar_scrollbar_width)
            .scroller_width(tokens.primitives.sidebar_scrollbar_scroller_width)
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
            "Clear selection",
            Message::ClearLibrarySidebarDetails,
            tokens,
        ),
    ]
    .spacing(Spacing::XS)
    .align_y(iced::Alignment::Center);

    let content = column![
        heading,
        text(format_count(selected_count, "PDF"))
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
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
        inspector_tag_editor(app, None, details_width, tokens),
        sidebar_action_button("Move to folder", tokens).on_press(Message::OpenMoveSelectionDialog),
        sidebar_action_button("Export selected", tokens)
            .on_press(Message::OpenExportDialog(ExportSource::SelectedEntries)),
        sidebar_action_button("Refresh metadata", tokens).on_press(Message::BulkRefreshPdfMetadata),
        sidebar_action_button("Rebuild thumbnails", tokens)
            .on_press(Message::BulkRebuildThumbnails),
        sidebar_action_button("Reindex full text", tokens).on_press(Message::BulkReindex),
        sidebar_action_button("Move to Trash", tokens).on_press(Message::RequestConfirmation(
            ConfirmationAction::BulkDeleteFromLibrary,
        )),
        sidebar_action_button("Clear selection", tokens).on_press(Message::ClearLibrarySelection),
    ]
    .spacing(Spacing::SM)
    .padding(Spacing::MD);

    container(content)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::SidebarDetailPanel))
        .into()
}

fn inspector_tag_editor<'a>(
    app: &'a PDFolioApp,
    entry: Option<LibraryEntry>,
    width: f32,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let selected_entries = app.selected_entries();
    let tags = if let Some(entry) = entry.as_ref() {
        entry.tags.clone()
    } else {
        common_tags(&selected_entries)
    };
    let current_tags = tags.clone();
    let mut chip_row = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
    if tags.is_empty() {
        chip_row = chip_row.push(
            text("No tags")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(sidebar_detail_secondary_color(tokens)),
        );
    } else {
        for tag in tags.into_iter().take(8) {
            let message = if let Some(entry) = entry.as_ref() {
                Message::InspectorRemoveTag {
                    entry_id: entry.id.clone(),
                    tag: tag.clone(),
                }
            } else {
                Message::InspectorRemoveTagFromSelection(tag.clone())
            };
            chip_row = chip_row.push(
                button(
                    text(format!("{tag} x"))
                        .size(FontSize::SM)
                        .font(ui_font(FontWeight::MEDIUM))
                        .color(tokens.text_secondary)
                        .wrapping(Wrapping::None),
                )
                .padding([Spacing::XS, Spacing::SM])
                .on_press(message)
                .style(move |_, status| button_style(tokens, Class::TagPill, status)),
            );
        }
    }

    let input_width = (width - Spacing::SM * 2.0).max(80.0);
    let mut editor = column![
        text("Tags")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(sidebar_detail_secondary_color(tokens)),
        chip_row,
        text_input("Add tag", &app.library.inspector_tag_input)
            .on_input(Message::InspectorTagInputChanged)
            .on_submit(Message::InspectorAddTag)
            .style(move |_, status| folder_sidebar_text_input_style(tokens, status))
            .width(input_width),
        sidebar_action_button("Add tag", tokens).on_press(Message::InspectorAddTag),
    ]
    .spacing(Spacing::XS);

    if app.library.inspector_tag_suggestions_open {
        let query = app.library.inspector_tag_input.trim().to_lowercase();
        if !query.is_empty() {
            let mut suggestions = row![].spacing(Spacing::XS).align_y(iced::Alignment::Center);
            let mut suggestion_count = 0;
            for tag in app
                .all_tags()
                .into_iter()
                .filter(|tag| {
                    tag.to_lowercase().contains(&query)
                        && !current_tags.iter().any(|current| current == tag)
                })
                .take(5)
            {
                suggestion_count += 1;
                let label = tag.clone();
                suggestions = suggestions.push(
                    button(
                        text(label)
                            .size(FontSize::SM)
                            .font(ui_font(FontWeight::MEDIUM))
                            .color(tokens.text_secondary)
                            .wrapping(Wrapping::None),
                    )
                    .padding([Spacing::XS, Spacing::SM])
                    .on_press(Message::InspectorApplyTag(tag))
                    .style(move |_, status| button_style(tokens, Class::TagPill, status)),
                );
            }
            if suggestion_count > 0 {
                editor = editor.push(suggestions);
            }
        }
    }

    container(editor)
        .width(Length::Fill)
        .padding([Spacing::XS, Spacing::SM])
        .style(move |_| container_style(tokens, Class::SidebarDetailRow))
        .into()
}

fn common_tags(entries: &[LibraryEntry]) -> Vec<String> {
    let Some((first, rest)) = entries.split_first() else {
        return Vec::new();
    };
    let mut tags = first.tags.clone();
    tags.retain(|tag| rest.iter().all(|entry| entry.tags.contains(tag)));
    tags.sort();
    tags
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
    tokens.class_styles[Class::SidebarDetailRow.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_primary)
}

pub(crate) fn sidebar_detail_secondary_color(tokens: ThemeTokens) -> Color {
    tokens.class_styles[Class::SidebarSection.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_secondary)
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
        let active = !app.library.trash_view_active
            && app.library.details_folder_id.as_ref() == Some(&folder.id);
        let drop_active = app.active_folder_drop_target() == Some(&folder.id);
        let flash_active = app.folder_drop_flash_active(&folder.id);
        let counts = app.normal_folder_smart_counts(Some(&folder.id));
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
    let content_background = normal_style.background.unwrap_or(tokens.surface);
    let indent = (depth as f32 * tokens.primitives.file_tree_indent_width)
        .min(tokens.primitives.file_tree_max_indent);
    let fold_width = fold_button_layout.width.unwrap_or(16.0);
    let meta_width = meta.as_ref().map_or(0.0, |value| {
        (value.len() as f32 * tokens.primitives.file_tree_meta_char_width).clamp(
            tokens.primitives.file_tree_meta_min_width,
            tokens.primitives.file_tree_meta_max_width,
        )
    });
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
            FILE_TREE_CHEVRON_DOWN_SVG
        } else {
            FILE_TREE_CHEVRON_RIGHT_SVG
        }))
        .width(tokens.primitives.sidebar_chevron_icon_size)
        .height(tokens.primitives.sidebar_chevron_icon_size)
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
        container("")
            .width(fold_button_layout.width.unwrap_or(16.0))
            .height(fold_button_layout.height.unwrap_or(20.0))
            .into()
    };
    let label_size = file_tree_style.text.size.unwrap_or(FontSize::MD);
    let row_height = file_tree_style.layout.height.unwrap_or(26.0);

    let mut content = row![
        container("").width(indent),
        chevron,
        text(file_tree_label(&label, label_width, label_size))
            .size(label_size)
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
        .height(row_height)
        .width(Length::Fill)
        .padding([tokens.primitives.file_tree_row_padding_y, Spacing::SM])
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
        .width(tokens.primitives.sidebar_chevron_icon_size)
        .height(tokens.primitives.sidebar_chevron_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(tokens.text_secondary),
        });
    let button = button(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(tokens.primitives.sidebar_chevron_button_size)
    .height(tokens.primitives.sidebar_chevron_button_size)
    .padding(0)
    .style(move |_, status| {
        let _ = transparent;
        crate::style::button_style(tokens, Class::SidebarToggleButton, status)
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
            .unwrap_or(tokens.text_secondary)
    }
}
