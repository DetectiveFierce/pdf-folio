use crate::library::view::*;
use crate::*;
use iced::widget::{button, column, row, Svg};

const FILE_TREE_CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M6.25 4.25 10 8l-3.75 3.75"/></svg>"##;
const FILE_TREE_CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="#000" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M4.25 6.25 8 10l3.75-3.75"/></svg>"##;

pub(crate) fn file_tree_fold_button<'a>(
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
