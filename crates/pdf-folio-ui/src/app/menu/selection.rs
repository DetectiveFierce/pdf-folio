use super::*;
use iced::widget::column;

pub(crate) fn view_selection_context_row(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let selected_count = app.library.selected_library_entries.len();
    let title_input_width = selection_title_input_width(app);
    let author_input_width = selection_author_input_width(app);
    let tag_input_width = selection_tag_input_width(app);
    let selected_label = text(format!("{selected_count} selected"))
        .size(FontSize::CONTROL)
        .font(ui_font(FontWeight::SEMIBOLD))
        .color(tokens.text_primary)
        .wrapping(Wrapping::None);

    let mut controls = row![]
        .spacing(Spacing::SM)
        .padding([Spacing::SM, Spacing::MD])
        .height(app.layout().selection_context_row_height)
        .align_y(iced::Alignment::Center)
        .push(selected_label)
        .push(toolbar_button("Clear", tokens).on_press(Message::ClearLibrarySelection));

    if selected_count == 1 {
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

    controls = controls.push(
        text("PDF-Folio")
            .size(FontSize::HEADING)
            .font(ui_font(FontWeight::BOLD))
            .color(tokens.text_secondary)
            .width(Length::Fill)
            .align_x(iced::alignment::Horizontal::Right)
            .wrapping(Wrapping::None),
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

pub(crate) fn selection_menu_button<'a>(
    label: &'a str,
    menu: SelectionMenu,
    active: bool,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    button(
        row![
            text(label)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_primary)
                .wrapping(Wrapping::None),
            text("v")
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS)
        .align_y(iced::Alignment::Center),
    )
    .padding([Spacing::SM, Spacing::MD])
    .height(30.0)
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
    let base = Spacing::MD + 128.0;
    if app.library.selected_library_entries.len() == 1 {
        return base + selection_title_input_width(app) + selection_author_input_width(app) + 88.0;
    }

    match menu {
        SelectionMenu::Tags => base + selection_tag_input_width(app),
        SelectionMenu::Folders => base + selection_tag_input_width(app) + 92.0,
        SelectionMenu::Metadata => base + selection_tag_input_width(app) + 202.0,
        SelectionMenu::Maintenance => base + selection_tag_input_width(app) + 330.0,
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
    let mut panel = column![].spacing(2.0).padding(Spacing::XS);
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
        .style(move |_| {
            let mut style = container_style(tokens, Class::MenuPanel);
            style.shadow = iced::Shadow {
                color: tokens.shadow,
                offset: iced::Vector::new(0.0, 8.0),
                blur_radius: 18.0,
            };
            style
        })
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
    button(
        text(selection_toolbar_action_label(labels, action))
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(tokens.text_primary)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
    )
    .height(item_height)
    .width(Length::Fill)
    .padding([Spacing::XS, Spacing::MD])
    .on_press(Message::SelectionToolbarActionSelected(action))
    .style(move |_, status| crate::style::button_style(tokens, Class::MenuItem, status))
    .into()
}

pub(crate) fn app_menu_separator<'a>(tokens: ThemeTokens) -> Element<'a, Message> {
    container("")
        .height(1.0)
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
