use super::*;
use iced::widget::column;

const FOLDER_CARD_ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/folder-svgrepo-com.svg");
const PARENT_DIRECTORY_ICON_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 24"><path fill="currentColor" d="M3.4 1.25h6.2c.75 0 1.3.25 1.85.8l3.1 3.1H26c1.1 0 2 .9 2 2V20.7c0 1.1-.9 2-2 2h-5.2v-2.05h5.1V7.3H13.65l-4.05-4H4.05v17.35h10.95V22.7H3.4c-1.1 0-2-.9-2-2V3.25c0-1.1.9-2 2-2z"/><path fill="currentColor" d="M16 9.25l5.25 5.25-1.55 1.55-2.45-2.45v7.65c0 .78-.62 1.4-1.4 1.4h-1.1V13.6l-2.45 2.45-1.55-1.55z"/></svg>"##;

pub(crate) fn view_folder_cards<'a>(
    app: &'a PDFolioApp,
    folders: Vec<Folder>,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let active_folder_drag = app.library.folder_drag.as_ref().filter(|drag| drag.active);
    let mut rows = column![].spacing(Spacing::SM);
    for chunk in folders.chunks(folder_cards_per_row(app)) {
        let mut card_row = row![].spacing(app.library_grid_column_gap());
        for folder in chunk {
            let mode = if active_folder_drag.is_some_and(|drag| drag.folder_id == folder.id) {
                if active_folder_drag
                    .and_then(|drag| drag.drop_target.as_ref())
                    .is_some()
                {
                    FolderCardRenderMode::NestingTarget
                } else {
                    FolderCardRenderMode::Placeholder
                }
            } else {
                FolderCardRenderMode::Normal
            };
            card_row = card_row.push(folder_grid_card(app, folder.clone(), tokens, mode));
        }
        rows = rows.push(card_row);
    }
    rows.into()
}

pub(crate) fn folder_cards_per_row(app: &PDFolioApp) -> usize {
    let available_width = app.library_available_grid_width();
    let card_pitch = app.library_grid_card_width() + app.layout().library_masonry_gap;
    ((available_width + app.layout().library_masonry_gap) / card_pitch)
        .floor()
        .max(1.0) as usize
}

pub(crate) fn folder_cards_section_height(app: &PDFolioApp, folder_count: usize) -> f32 {
    let parent_height = parent_directory_drop_section_height(app);
    if folder_count == 0 {
        return parent_height;
    }

    parent_height + folder_cards_height(app, folder_count)
}

pub(crate) fn folder_cards_height(app: &PDFolioApp, folder_count: usize) -> f32 {
    if folder_count == 0 {
        return 0.0;
    }

    let rows = folder_count.div_ceil(folder_cards_per_row(app)).max(1);
    rows as f32 * app.layout().library_folder_grid_row_height
        + rows.saturating_sub(1) as f32 * Spacing::SM
        + Spacing::MD
}

pub(crate) fn parent_directory_drop_section_height(app: &PDFolioApp) -> f32 {
    if app.parent_directory_drop_box_visible() {
        parent_directory_drop_box_height(app) + Spacing::MD
    } else {
        0.0
    }
}

pub(crate) fn parent_directory_drop_box_height(app: &PDFolioApp) -> f32 {
    app.layout().library_folder_grid_row_height
}

pub(crate) fn view_parent_directory_drop_box<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let active = app.parent_directory_drop_target_active();
    let border_color = if active {
        tokens.accent
    } else {
        tokens.text_secondary
    };
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(
        PARENT_DIRECTORY_ICON_SVG,
    ))
    .width(tokens.primitives.folder_parent_icon_width)
    .height(tokens.primitives.folder_parent_icon_height)
    .style(move |_, _| iced::widget::svg::Style {
        color: Some(border_color),
    });
    let content = row![
        icon,
        text("Move to Parent Directory")
            .size(FontSize::CONTROL)
            .font(display_font(FontWeight::SEMIBOLD))
            .color(border_color)
    ]
    .spacing(Spacing::SM)
    .align_y(iced::Alignment::Center);

    let drop_box = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::LibraryFolderCard);
            let state = if active {
                ComponentState::Active
            } else {
                ComponentState::Normal
            };
            let state_style = tokens.class_styles[Class::LibraryFolderCard.index()].resolve(state);
            style = style.with_visual_override(state_style);
            style
        });

    mouse_area(
        container(drop_box)
            .width(Length::Fill)
            .height(parent_directory_drop_box_height(app)),
    )
    .on_enter(Message::ParentDirectoryDropTargetChanged(true))
    .on_exit(Message::ParentDirectoryDropTargetChanged(false))
    .into()
}

pub(crate) fn folder_grid_card<'a>(
    app: &'a PDFolioApp,
    folder: Folder,
    tokens: ThemeTokens,
    mode: FolderCardRenderMode,
) -> Element<'a, Message> {
    let folder_id = folder.id.clone();
    let selected = app.library.details_folder_id.as_ref() == Some(&folder.id);
    let drop_active = app.active_folder_drop_target() == Some(&folder.id);
    let flash_active = app.folder_drop_flash_active(&folder.id);
    let smart_counts = app.folder_smart_counts(Some(&folder.id));
    let child_count = app
        .library
        .library_folders
        .iter()
        .filter(|child| child.parent_id.as_ref() == Some(&folder.id))
        .count();
    let meta = folder_meta_label(smart_counts, child_count);
    let folder_title_size = app.library_card_font_size(FontSize::CONTROL + 1);
    let folder_meta_size = app.library_card_font_size(FontSize::MD);
    let folder_text_reserve = tokens.primitives.folder_icon_container_width
        + app.library_card_spacing().max(Spacing::XS)
        + 2.0 * app.library_card_padding().min(Spacing::MD);
    let folder_text_width = (app.library_grid_card_width() - folder_text_reserve)
        .max(tokens.primitives.document_preview_min_line_width);
    let content_alpha = folder_card_content_alpha(app, mode);
    let folder_title_width =
        (folder_text_width - Spacing::SM).max(tokens.primitives.document_preview_min_line_width);
    let title =
        truncate_for_width_with_font(&folder.name, folder_title_width, 0.0, folder_title_size);
    let meta = truncate_for_width_with_font(&meta, folder_text_width, 0.0, folder_meta_size);
    let content = row![
        folder_icon(tokens, content_alpha),
        column![
            text(title)
                .size(folder_title_size)
                .font(display_font(FontWeight::MEDIUM))
                .color(with_alpha(tokens.text_primary, content_alpha))
                .wrapping(Wrapping::None),
            text(meta)
                .size(folder_meta_size)
                .font(ui_font(FontWeight::REGULAR))
                .color(with_alpha(tokens.text_secondary, content_alpha))
                .wrapping(Wrapping::None),
        ]
        .spacing(0)
        .height(tokens.primitives.folder_icon_container_height)
        .width(Length::Fill),
    ]
    .spacing(app.library_card_spacing().max(Spacing::XS))
    .padding(app.library_card_padding().min(Spacing::MD))
    .height(app.layout().library_folder_grid_row_height)
    .align_y(iced::Alignment::Center);

    let card = container(content)
        .width(Length::Fill)
        .style(move |_| {
            let mut style = container_style(tokens, Class::LibraryFolderCard);
            if matches!(mode, FolderCardRenderMode::Placeholder) {
                let placeholder_style = tokens.class_styles[Class::LibraryFolderCard.index()]
                    .resolve(ComponentState::Disabled);
                style = style.with_visual_override(placeholder_style);
            }
            if selected {
                let selected_style = tokens.class_styles[Class::LibraryFolderCard.index()]
                    .resolve(ComponentState::Selected);
                style = style.with_visual_override(selected_style);
                style.border.color = selected_style.border_color.unwrap_or(tokens.focus);
                style.border.width = selected_style.border_width.unwrap_or(1.5).max(1.5);
            }
            if drop_active || flash_active || matches!(mode, FolderCardRenderMode::NestingTarget) {
                let drop_style = container_style(tokens, Class::FolderDropTarget);
                style.background = drop_style.background;
                style.border = drop_style.border;
                style.shadow = drop_style.shadow;
            }
            style
        })
        .width(app.library_grid_card_width());

    if mode == FolderCardRenderMode::Floating {
        return card.into();
    }

    let area = mouse_area(card)
        .on_right_press(Message::ContextMenuOpened(ContextMenuTarget::Folder(Some(
            folder_id.clone(),
        ))))
        .on_enter(Message::FolderDropTargetChanged(Some(folder_id)))
        .on_exit(Message::FolderDropTargetChanged(None));
    if mode == FolderCardRenderMode::Normal {
        if app.library.trash_view_active {
            area.on_press(Message::FolderClicked(Some(folder.id.clone())))
                .into()
        } else {
            area.on_press(Message::BeginFolderDrag(folder.id.clone()))
                .on_release(Message::EndFolderDrag)
                .interaction(mouse::Interaction::Grab)
                .into()
        }
    } else {
        area.into()
    }
}

pub(crate) fn folder_icon<'a>(tokens: ThemeTokens, alpha: f32) -> Element<'a, Message> {
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(FOLDER_CARD_ICON_SVG))
        .width(tokens.primitives.folder_icon_size)
        .height(tokens.primitives.folder_icon_size)
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(with_alpha(tokens.accent, alpha)),
        });
    container(
        container(icon)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .center(tokens.primitives.folder_icon_container_width)
    .height(tokens.primitives.folder_icon_container_height)
    .style(move |_| {
        let mut style = container_style(tokens, Class::TagPill);
        if let Some(iced::Background::Color(mut background)) = style.background {
            background.a *= alpha.clamp(0.0, 1.0);
            style.background = Some(iced::Background::Color(background));
        }
        style
    })
    .into()
}

pub(crate) fn folder_card_content_alpha(app: &PDFolioApp, mode: FolderCardRenderMode) -> f32 {
    if mode == FolderCardRenderMode::Placeholder {
        app.layout().library_drag_placeholder_content_alpha
    } else {
        1.0
    }
}

pub(crate) fn folder_meta_label(counts: FolderSmartCounts, child_count: usize) -> String {
    let mut parts = Vec::new();
    if counts.total > 0 {
        parts.push(format_count(counts.total, "PDF"));
    }
    if child_count > 0 {
        parts.push(format_count(child_count, "Folder"));
    }
    if counts.missing > 0 {
        parts.push(format!("{} missing", counts.missing));
    }

    if parts.is_empty() {
        String::from("Empty")
    } else {
        parts.join(" . ")
    }
}

pub(crate) fn folder_sidebar_count_label(counts: FolderSmartCounts) -> String {
    format_count(counts.total, "PDF")
}

pub(crate) fn format_count(count: usize, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

pub(crate) fn scroll_library_to_offset_task(offset_y: f32) -> Task<Message> {
    operation::scroll_to(
        Id::new(LIBRARY_SCROLLABLE_ID),
        operation::AbsoluteOffset {
            x: Some(0.0),
            y: Some(offset_y.max(0.0)),
        },
    )
}

impl LibraryRenderItem {
    pub(crate) fn entry(&self) -> &LibraryEntry {
        match self {
            Self::Entry(entry) | Self::Ghost(entry) | Self::DropZone(entry) => entry,
        }
    }
}

pub(crate) fn library_render_items(
    app: &PDFolioApp,
    entries: &[LibraryEntry],
) -> Vec<LibraryRenderItem> {
    let Some(drag) = app.library.library_drag.as_ref().filter(|drag| drag.active) else {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    };
    if !drag.multi {
        let Some(ghost_entry) = entries
            .iter()
            .find(|entry| entry.id == drag.entry_id)
            .cloned()
        else {
            return entries
                .iter()
                .cloned()
                .map(LibraryRenderItem::Entry)
                .collect();
        };

        let compact_entries: Vec<_> = entries
            .iter()
            .filter(|entry| entry.id != drag.entry_id)
            .cloned()
            .collect();
        let target_index = drag.target_index.min(compact_entries.len());

        let mut items = Vec::with_capacity(entries.len());
        for index in 0..=compact_entries.len() {
            if target_index == index {
                items.push(LibraryRenderItem::Ghost(ghost_entry.clone()));
            }

            if let Some(entry) = compact_entries.get(index) {
                items.push(LibraryRenderItem::Entry(entry.clone()));
            }
        }

        return items;
    }

    let dragged_ids = drag.entry_ids.iter().cloned().collect::<HashSet<_>>();
    let placeholder_entries = entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .cloned()
        .collect::<Vec<_>>();
    if placeholder_entries.is_empty() {
        return entries
            .iter()
            .cloned()
            .map(LibraryRenderItem::Entry)
            .collect();
    }

    let drop_zone_entry = placeholder_entries[0].clone();
    let target_index = drag
        .target_index
        .min(entries.len().saturating_sub(placeholder_entries.len()));
    let mut compact_index = 0;
    let mut drop_zone_inserted = false;
    let mut items = Vec::with_capacity(entries.len() + 1);
    for entry in entries {
        if dragged_ids.contains(&entry.id) {
            items.push(LibraryRenderItem::Ghost(entry.clone()));
        } else {
            if !drop_zone_inserted && drag.drop_target.is_none() && compact_index == target_index {
                items.push(LibraryRenderItem::DropZone(drop_zone_entry.clone()));
                drop_zone_inserted = true;
            }
            items.push(LibraryRenderItem::Entry(entry.clone()));
            compact_index += 1;
        }
    }
    if !drop_zone_inserted && drag.drop_target.is_none() {
        items.push(LibraryRenderItem::DropZone(drop_zone_entry));
    }

    items
}

pub(crate) fn shortest_column_index(column_heights: &[f32]) -> usize {
    column_heights
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

pub(crate) fn masonry_target_index(
    layout: &LibraryMasonryLayout,
    column_index: usize,
    content_y: f32,
) -> Option<usize> {
    let column = layout.columns.get(column_index)?;
    if column.is_empty() {
        return Some(layout.columns.iter().flatten().count());
    }

    column
        .iter()
        .find(|item| content_y < item.top + item.height / 2.0)
        .map(|item| item.index)
        .or_else(|| column.last().map(|item| item.index + 1))
}

pub(crate) fn floating_library_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app
        .library
        .library_drag
        .as_ref()
        .filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let visible_entries = app.visible_library_entries();
    let entry = visible_entries
        .iter()
        .find(|entry| entry.id == drag.entry_id)?
        .clone();

    let preview = if drag.multi {
        multi_drag_stack_preview(app, drag, &visible_entries, tokens)?
    } else if app.library.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    let x_offset = if app.library.compact_view_mode {
        app.layout().library_drag_preview_list_x_offset
    } else {
        app.layout().library_drag_preview_grid_x_offset
    };
    let y_offset = if app.library.compact_view_mode {
        app.layout().library_drag_preview_list_y_offset
    } else {
        app.layout().library_drag_preview_grid_y_offset
    };

    Some(
        pin(preview)
            .x((cursor.x - x_offset).max(0.0))
            .y((cursor.y - y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

pub(crate) fn floating_folder_drag_preview<'a>(
    app: &'a PDFolioApp,
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let drag = app
        .library
        .folder_drag
        .as_ref()
        .filter(|drag| drag.active)?;
    let cursor = drag.cursor?;
    let folder = app
        .library
        .library_folders
        .iter()
        .find(|folder| folder.id == drag.folder_id)?
        .clone();
    let preview = container(folder_grid_card(
        app,
        folder,
        tokens,
        FolderCardRenderMode::Floating,
    ))
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        pin(preview)
            .x((cursor.x - app.layout().library_drag_preview_grid_x_offset).max(0.0))
            .y((cursor.y - app.layout().library_drag_preview_grid_y_offset).max(0.0))
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

pub(crate) fn multi_drag_stack_preview<'a>(
    app: &'a PDFolioApp,
    drag: &LibraryDragState,
    visible_entries: &[LibraryEntry],
    tokens: ThemeTokens,
) -> Option<Element<'a, Message>> {
    let dragged_ids = drag.entry_ids.iter().collect::<HashSet<_>>();
    let mut entries = visible_entries
        .iter()
        .filter(|entry| dragged_ids.contains(&entry.id))
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }

    while entries.len() < 3 {
        let entry = entries.last().cloned()?;
        entries.push(entry);
    }

    let rear = drag_stack_card(app, entries[2].clone(), tokens);
    let middle = drag_stack_card(app, entries[1].clone(), tokens);
    let front = drag_stack_card(app, entries[0].clone(), tokens);
    let badge = container(
        text(format_count(drag.entry_ids.len(), "PDF"))
            .size(FontSize::SM)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_primary),
    )
    .padding([Spacing::XS, Spacing::MD])
    .style(move |_| container_style(tokens, Class::DragStackGhost));

    Some(
        stack![
            pin(rear).x(Spacing::LG).y(Spacing::LG),
            pin(middle).x(Spacing::SM).y(Spacing::SM),
            pin(front).x(0.0).y(0.0),
            pin(badge).x(Spacing::MD).y(Spacing::MD),
        ]
        .into(),
    )
}

pub(crate) fn drag_stack_card<'a>(
    app: &'a PDFolioApp,
    entry: LibraryEntry,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let card = if app.library.compact_view_mode {
        library_entry_row(app, entry, tokens, LibraryEntryRenderMode::Floating)
    } else {
        library_entry_card(app, entry, tokens, LibraryEntryRenderMode::Floating)
    };

    container(card)
        .style(move |_| container_style(tokens, Class::DragStackGhost))
        .into()
}
