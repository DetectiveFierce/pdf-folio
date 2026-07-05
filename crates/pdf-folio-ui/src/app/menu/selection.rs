use super::*;
use iced::widget::column;

pub(crate) fn view_selection_context_row(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    if folder_selection_context_row_visible(app) {
        return view_folder_selection_context_row(app, tokens);
    }

    let selected_entry_count = app.library.selected_library_entries.len();
    let selected_folder_count =
        usize::from(app.library.trash_view_active && app.details_folder().is_some());
    let selected_count = selected_entry_count + selected_folder_count;
    let title_input_width = selection_title_input_width(app);
    let author_input_width = selection_author_input_width(app);
    let tag_input_width = selection_tag_input_width(app);
    let row_spacing = selection_metric(app, "row_spacing");
    let row_padding_x = selection_metric(app, "row_padding_x");
    let row_padding_y = selection_metric(app, "row_padding_y");
    let selected_label = text(format!("{selected_count} selected"))
        .size(FontSize::CONTROL)
        .font(ui_font(FontWeight::SEMIBOLD))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    let mut controls = row![]
        .spacing(row_spacing)
        .padding([row_padding_y, row_padding_x])
        .height(app.layout().selection_context_row_height)
        .align_y(iced::Alignment::Center)
        .push(selected_label)
        .push(toolbar_button("Clear", tokens).on_press(Message::ClearLibrarySelection));

    if app.library.trash_view_active {
        controls =
            controls.push(trash_restore_button(tokens).on_press(Message::RestoreSelectedFromTrash));
        if selected_entry_count > 0 {
            controls = controls.push(trash_delete_button(tokens).on_press(
                Message::RequestConfirmation(ConfirmationAction::PermanentlyDeleteFromTrash),
            ));
        }
        if let Some(folder_id) = app
            .library
            .trash_view_active
            .then(|| app.library.details_folder_id.clone())
            .flatten()
        {
            controls = controls.push(trash_delete_button(tokens).on_press(
                Message::RequestConfirmation(ConfirmationAction::PermanentlyDeleteFolderFromTrash(
                    folder_id,
                )),
            ));
        }
    } else if selected_entry_count == 1 {
        controls = controls
            .push(
                text_input("Title", &app.library.details_title_input)
                    .on_input(Message::DetailsTitleChanged)
                    .on_submit(Message::SaveDetailsMetadata)
                    .id(Id::new(LIBRARY_DETAILS_TITLE_INPUT_ID))
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(title_input_width),
            )
            .push(
                text_input("Author", &app.library.details_author_input)
                    .on_input(Message::DetailsAuthorChanged)
                    .on_submit(Message::SaveDetailsMetadata)
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(author_input_width),
            )
            .push(toolbar_button("Save", tokens).on_press(Message::SaveDetailsMetadata))
            .push(selection_menu_button(
                "More",
                SelectionMenu::More,
                app.chrome.open_selection_menu == Some(SelectionMenu::More),
                tokens,
            ));
    } else {
        controls = controls
            .push(
                text_input("Tag", &app.library.bulk_tag_input)
                    .on_input(Message::BulkTagInputChanged)
                    .on_submit(Message::BulkAddTag)
                    .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                    .width(tag_input_width),
            )
            .push(selection_menu_button(
                "Tags",
                SelectionMenu::Tags,
                app.chrome.open_selection_menu == Some(SelectionMenu::Tags),
                tokens,
            ))
            .push(selection_menu_button(
                "Folders",
                SelectionMenu::Folders,
                app.chrome.open_selection_menu == Some(SelectionMenu::Folders),
                tokens,
            ))
            .push(selection_menu_button(
                "Metadata",
                SelectionMenu::Metadata,
                app.chrome.open_selection_menu == Some(SelectionMenu::Metadata),
                tokens,
            ))
            .push(selection_menu_button(
                "Maintenance",
                SelectionMenu::Maintenance,
                app.chrome.open_selection_menu == Some(SelectionMenu::Maintenance),
                tokens,
            ));
    }

    let trailing_control: Element<'_, Message> = if app.library.trash_view_active {
        text("PDF-Folio")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::SEMIBOLD))
            .color(tokens.text_secondary)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .wrapping(Wrapping::None)
            .into()
    } else {
        container(selection_trash_button(app, tokens))
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .into()
    };
    controls = controls.push(trailing_control);

    container(controls)
        .width(Length::Fill)
        .style(move |_| {
            let active_style =
                tokens.class_styles[Class::MenuBar.index()].resolve(ComponentState::Active);
            container_style(tokens, Class::MenuBar).with_visual_override(active_style)
        })
        .into()
}

pub(crate) fn view_folder_selection_context_row(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let title_input_width = selection_title_input_width(app);
    let row_spacing = selection_metric(app, "row_spacing");
    let row_padding_x = selection_metric(app, "row_padding_x");
    let row_padding_y = selection_metric(app, "folder_row_padding_y");
    let selected_label = text("1 folder selected")
        .size(FontSize::CONTROL)
        .font(ui_font(FontWeight::SEMIBOLD))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    let controls = row![]
        .spacing(row_spacing)
        .padding([row_padding_y, row_padding_x])
        .height(folder_selection_context_row_height(app))
        .align_y(iced::Alignment::Center)
        .push(selected_label)
        .push(toolbar_button("Clear", tokens).on_press(Message::ClearLibrarySidebarDetails))
        .push(
            text_input("Title", &app.library.folder_rename_input)
                .on_input(Message::FolderRenameInputChanged)
                .on_submit(Message::RenameSelectedFolder)
                .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
                .width(title_input_width),
        )
        .push(toolbar_button("Save", tokens).on_press(Message::RenameSelectedFolder))
        .push(
            container(folder_selection_trash_button(app, tokens))
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right),
        );

    container(controls)
        .width(Length::Fill)
        .style(move |_| {
            let active_style =
                tokens.class_styles[Class::MenuBar.index()].resolve(ComponentState::Active);
            container_style(tokens, Class::MenuBar).with_visual_override(active_style)
        })
        .into()
}

pub(crate) fn library_context_toolbar_height(app: &PDFolioApp) -> f32 {
    if folder_selection_context_row_visible(app) {
        folder_selection_context_row_height(app)
    } else {
        app.layout().selection_context_row_height
    }
}

fn folder_selection_context_row_visible(app: &PDFolioApp) -> bool {
    app.library.selected_library_entries.is_empty()
        && !app.library.trash_view_active
        && app.details_folder().is_some()
}

fn folder_selection_context_row_height(app: &PDFolioApp) -> f32 {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    tokens.primitives.selection_menu_button_height
        + selection_metric(app, "folder_row_padding_y") * 2.0
}

pub(crate) fn trash_restore_button<'a>(tokens: ThemeTokens) -> iced::widget::Button<'a, Message> {
    let layout = tokens.class_styles[Class::SelectionRestoreButton.index()].layout;
    let text_style = tokens.class_styles[Class::SelectionRestoreButton.index()].text;
    let text_color = class_text_color(
        tokens,
        Class::SelectionRestoreButton,
        ComponentState::Normal,
        tokens.surface,
    );
    button(
        text("Restore")
            .size(text_style.size.unwrap_or(FontSize::CONTROL))
            .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(text_color)
            .wrapping(Wrapping::None),
    )
    .padding([layout.padding_y(Spacing::XS), layout.padding_x(Spacing::MD)])
    .height(Length::Fixed(layout.height.unwrap_or(26.0)))
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SelectionRestoreButton, status)
    })
}

pub(crate) fn trash_delete_button<'a>(tokens: ThemeTokens) -> iced::widget::Button<'a, Message> {
    let layout = tokens.class_styles[Class::SelectionDangerButton.index()].layout;
    let text_style = tokens.class_styles[Class::SelectionDangerButton.index()].text;
    let text_color = class_text_color(
        tokens,
        Class::SelectionDangerButton,
        ComponentState::Normal,
        tokens.text_primary,
    );
    button(
        text("Permanently Delete")
            .size(text_style.size.unwrap_or(FontSize::CONTROL))
            .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
            .color(text_color)
            .wrapping(Wrapping::None),
    )
    .padding([layout.padding_y(Spacing::XS), layout.padding_x(Spacing::MD)])
    .height(Length::Fixed(layout.height.unwrap_or(26.0)))
    .style(move |_, status| {
        crate::style::button_style(tokens, Class::SelectionDangerButton, status)
    })
}

pub(crate) fn selection_trash_button<'a>(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let layout = tokens.class_styles[Class::SelectionDangerIconButton.index()].layout;
    let icon_size = selection_metric(app, "icon_size");
    let icon_slot = selection_metric(app, "icon_slot_size");
    let tooltip_delay = selection_metric(app, "tooltip_delay_ms").max(0.0) as u64;
    let icon_color = class_text_color(
        tokens,
        Class::SelectionDangerIconButton,
        ComponentState::Normal,
        Color::WHITE,
    );
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(TRASH_CAN_SVG))
        .width(Length::Fixed(icon_size))
        .height(Length::Fixed(icon_size))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });
    let button = button(container(icon).center(Length::Fixed(icon_slot)))
        .padding(layout.padding_x(Spacing::SM))
        .width(Length::Fixed(
            layout
                .width
                .unwrap_or(tokens.primitives.selection_menu_button_height),
        ))
        .height(
            layout
                .height
                .unwrap_or(tokens.primitives.selection_menu_button_height),
        )
        .on_press(Message::RequestConfirmation(
            ConfirmationAction::BulkDeleteFromLibrary,
        ))
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::SelectionDangerIconButton, status)
        });

    tooltip(
        button,
        container(
            text("Move to Trash")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        )
        .padding(
            tokens.class_styles[Class::Tooltip.index()]
                .layout
                .padding_x(Spacing::SM),
        )
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(tooltip_delay))
    .into()
}

pub(crate) fn folder_selection_trash_button<'a>(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let layout = tokens.class_styles[Class::SelectionDangerIconButton.index()].layout;
    let icon_size = selection_metric(app, "icon_size");
    let icon_slot = selection_metric(app, "icon_slot_size");
    let tooltip_delay = selection_metric(app, "tooltip_delay_ms").max(0.0) as u64;
    let icon_color = class_text_color(
        tokens,
        Class::SelectionDangerIconButton,
        ComponentState::Normal,
        Color::WHITE,
    );
    let icon = Svg::new(iced::widget::svg::Handle::from_memory(TRASH_CAN_SVG))
        .width(Length::Fixed(icon_size))
        .height(Length::Fixed(icon_size))
        .style(move |_, _| iced::widget::svg::Style {
            color: Some(icon_color),
        });
    let button = button(container(icon).center(Length::Fixed(icon_slot)))
        .padding(layout.padding_x(Spacing::SM))
        .width(Length::Fixed(
            layout
                .width
                .unwrap_or(tokens.primitives.selection_menu_button_height),
        ))
        .height(
            layout
                .height
                .unwrap_or(tokens.primitives.selection_menu_button_height),
        )
        .on_press(Message::RequestDeleteSelectedFolder)
        .style(move |_, status| {
            crate::style::button_style(tokens, Class::SelectionDangerIconButton, status)
        });

    tooltip(
        button,
        container(
            text("Move folder to Trash")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
        )
        .padding(
            tokens.class_styles[Class::Tooltip.index()]
                .layout
                .padding_x(Spacing::SM),
        )
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(tooltip_delay))
    .into()
}

pub(crate) fn selection_menu_button<'a>(
    label: &'a str,
    menu: SelectionMenu,
    active: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let layout = tokens.class_styles[Class::MenuButton.index()].layout;
    let text_style = tokens.class_styles[Class::MenuButton.index()].text;
    let label_color = class_text_color(
        tokens,
        Class::MenuButton,
        if active {
            ComponentState::Active
        } else {
            ComponentState::Normal
        },
        tokens.text_primary,
    );
    let detail_size = tokens.class_styles[Class::MenuPanel.index()]
        .text
        .size
        .unwrap_or(FontSize::SM);
    button(
        row![
            text(label)
                .size(text_style.size.unwrap_or(FontSize::MD))
                .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
                .color(label_color)
                .wrapping(Wrapping::None),
            text("v")
                .size(detail_size)
                .font(ui_font(text_style.weight.unwrap_or(FontWeight::MEDIUM)))
                .color(label_color),
        ]
        .spacing(layout.spacing.unwrap_or(Spacing::XS))
        .align_y(iced::Alignment::Center),
    )
    .padding([layout.padding_y(Spacing::SM), layout.padding_x(Spacing::MD)])
    .height(
        layout
            .height
            .unwrap_or(tokens.primitives.selection_menu_button_height),
    )
    .on_press(Message::SelectionMenuOpened(menu))
    .style(move |_, status| {
        if active {
            let active_style =
                tokens.class_styles[Class::MenuButton.index()].resolve(ComponentState::Active);
            crate::style::button_style(tokens, Class::MenuButton, status)
                .with_visual_override(active_style)
        } else {
            crate::style::button_style(tokens, Class::MenuButton, status)
        }
    })
    .into()
}

pub(crate) fn view_selection_menu_dropdown(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let Some(menu) = app.chrome.open_selection_menu else {
        return container("").into();
    };
    pin(selection_menu_panel(app, menu, tokens))
        .x(selection_menu_x(app, menu))
        .y(app_menu_bar_height(app))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(crate) fn selection_menu_x(app: &PDFolioApp, menu: SelectionMenu) -> f32 {
    let base = selection_metric(app, "dropdown_base_x");
    if app.library.selected_library_entries.len() == 1 {
        return base
            + selection_title_input_width(app)
            + selection_author_input_width(app)
            + selection_metric(app, "single_dropdown_extra_x");
    }

    match menu {
        SelectionMenu::Tags => base + selection_tag_input_width(app),
        SelectionMenu::Folders => {
            base + selection_tag_input_width(app)
                + selection_metric(app, "folders_dropdown_offset_x")
        }
        SelectionMenu::Metadata => {
            base + selection_tag_input_width(app)
                + selection_metric(app, "metadata_dropdown_offset_x")
        }
        SelectionMenu::Maintenance => {
            base + selection_tag_input_width(app)
                + selection_metric(app, "maintenance_dropdown_offset_x")
        }
        SelectionMenu::More => base,
    }
}

pub(crate) fn selection_menu_panel<'a>(
    app: &'a PDFolioApp,
    menu: SelectionMenu,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let labels = app.labels();
    let actions: &'static [SelectionToolbarAction] = match menu {
        SelectionMenu::More => &SINGLE_MORE_ACTIONS,
        SelectionMenu::Tags => &BULK_TAG_ACTIONS,
        SelectionMenu::Folders => &BULK_FOLDER_ACTIONS,
        SelectionMenu::Metadata => &BULK_METADATA_ACTIONS,
        SelectionMenu::Maintenance => &BULK_MAINTENANCE_ACTIONS,
    };
    let panel_layout = tokens.class_styles[Class::MenuPanel.index()].layout;
    let mut panel = column![]
        .spacing(panel_layout.spacing.unwrap_or(2.0))
        .padding([
            panel_layout.padding_y(Spacing::XS),
            panel_layout.padding_x(Spacing::XS),
        ]);
    for action in actions {
        panel = panel.push(selection_menu_item(
            *action,
            tokens,
            labels,
            app.layout().app_menu_item_height,
        ));
    }

    container(panel)
        .width(app.layout().app_menu_panel_width)
        .style(move |_| container_style(tokens, Class::MenuPanel))
        .into()
}

pub(crate) fn app_menu_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    menu: AppMenu,
) -> &'a str {
    labels.get(LabelSection::AppMenu, app_menu_key(menu), menu.label())
}

pub(crate) fn app_menu_action_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    labels.get(LabelSection::AppMenuAction, key, fallback)
}

pub(crate) fn selection_toolbar_action_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    action: SelectionToolbarAction,
) -> &'a str {
    labels.get(
        LabelSection::SelectionToolbarAction,
        selection_toolbar_action_key(action),
        action.label(),
    )
}

pub(crate) fn library_sidebar_tab_label<'a>(
    labels: &'a crate::style::AppLabelTokens,
    tab: LibrarySidebarTab,
) -> &'a str {
    labels.get(
        LabelSection::LibrarySidebarTab,
        library_sidebar_tab_key(tab),
        tab.label(),
    )
}

pub(crate) fn label_text<'a>(
    labels: &'a crate::style::AppLabelTokens,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    labels.get(LabelSection::Text, key, fallback)
}

pub(crate) fn app_menu_key(menu: AppMenu) -> &'static str {
    match menu {
        AppMenu::File => "File",
        AppMenu::Edit => "Edit",
        AppMenu::View => "View",
        AppMenu::Document => "Document",
        AppMenu::Library => "Library",
        AppMenu::Tools => "Tools",
        AppMenu::Help => "Help",
    }
}

pub(crate) fn library_sidebar_tab_key(tab: LibrarySidebarTab) -> &'static str {
    match tab {
        LibrarySidebarTab::Files => "Files",
        LibrarySidebarTab::Tags => "Tags",
    }
}

pub(crate) fn selection_toolbar_action_key(action: SelectionToolbarAction) -> &'static str {
    match action {
        SelectionToolbarAction::AddTag => "AddTag",
        SelectionToolbarAction::RemoveTag => "RemoveTag",
        SelectionToolbarAction::AddToFolder => "AddToFolder",
        SelectionToolbarAction::RemoveFromFolder => "RemoveFromFolder",
        SelectionToolbarAction::SaveDetails => "SaveDetails",
        SelectionToolbarAction::ResetDetails => "ResetDetails",
        SelectionToolbarAction::SortTitles => "SortTitles",
        SelectionToolbarAction::RefreshMetadata => "RefreshMetadata",
        SelectionToolbarAction::ResetMetadata => "ResetMetadata",
        SelectionToolbarAction::RebuildThumbnails => "RebuildThumbnails",
        SelectionToolbarAction::Reindex => "Reindex",
        SelectionToolbarAction::DeleteMetadata => "DeleteMetadata",
    }
}

pub(crate) fn selection_menu_item(
    action: SelectionToolbarAction,
    tokens: ThemeTokens,
    labels: &crate::style::AppLabelTokens,
    item_height: f32,
) -> Element<'_, Message> {
    let item_layout = tokens.class_styles[Class::MenuItem.index()].layout;
    let item_text = tokens.class_styles[Class::MenuItem.index()].text;
    let text_color = class_text_color(
        tokens,
        Class::MenuItem,
        ComponentState::Normal,
        tokens.text_primary,
    );
    button(
        text(selection_toolbar_action_label(labels, action))
            .size(item_text.size.unwrap_or(FontSize::MD))
            .font(ui_font(item_text.weight.unwrap_or(FontWeight::REGULAR)))
            .color(text_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
    )
    .height(item_layout.height.unwrap_or(item_height))
    .width(Length::Fill)
    .padding([
        item_layout.padding_y(Spacing::XS),
        item_layout.padding_x(Spacing::MD),
    ])
    .on_press(Message::SelectionToolbarActionSelected(action))
    .style(move |_, status| crate::style::button_style(tokens, Class::MenuItem, status))
    .into()
}

pub(crate) fn app_menu_separator<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    container("")
        .height(tokens.primitives.menu_separator_height)
        .width(Length::Fill)
        .style(move |_| {
            let selected_style =
                tokens.class_styles[Class::MenuBar.index()].resolve(ComponentState::Selected);
            container_style(tokens, Class::MenuBar).with_visual_override(selected_style)
        })
        .into()
}

pub(crate) fn selection_title_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().selection_title_input_min_width,
        app.layout().selection_title_input_width,
        0.34,
    )
}

pub(crate) fn selection_author_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().selection_author_input_min_width,
        app.layout().selection_author_input_width,
        0.24,
    )
}

pub(crate) fn selection_tag_input_width(app: &PDFolioApp) -> f32 {
    responsive_selection_input_width(
        app,
        app.layout().bulk_tag_input_min_width,
        app.layout().bulk_tag_input_width,
        0.2,
    )
}

pub(crate) fn responsive_selection_input_width(
    app: &PDFolioApp,
    min_width: f32,
    max_width: f32,
    viewport_fraction: f32,
) -> f32 {
    (app.library.library_viewport_width * viewport_fraction).clamp(min_width, max_width)
}

fn selection_metric(app: &PDFolioApp, property: &str) -> f32 {
    app.layout().metric(
        "SelectionToolbar",
        property,
        selection_metric_fallback(property),
    )
}

fn selection_metric_fallback(property: &str) -> f32 {
    match property {
        "row_spacing" => Spacing::SM,
        "row_padding_x" => Spacing::MD,
        "row_padding_y" => Spacing::SM,
        "folder_row_padding_y" => 4.0,
        "dropdown_base_x" => Spacing::MD + 128.0,
        "single_dropdown_extra_x" => 88.0,
        "folders_dropdown_offset_x" => 92.0,
        "metadata_dropdown_offset_x" => 202.0,
        "maintenance_dropdown_offset_x" => 330.0,
        "icon_size" => 16.0,
        "icon_slot_size" => 20.0,
        "tooltip_delay_ms" => 400.0,
        _ => 0.0,
    }
}

fn class_text_color(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
    fallback: Color,
) -> Color {
    tokens.class_styles[class.index()]
        .resolve(state)
        .text_color
        .unwrap_or(fallback)
}
