//! App menu and selection menu rendering/routing.

use super::*;
use crate::viewer::state::{ViewerScrollMode, ViewerSpreadMode};
use crate::viewer::zoom::ZoomPreset;
use iced::widget::{column, row, stack};
use pdf_folio_ui_components::library::view::with_alpha;

#[path = "menu/selection.rs"]
mod selection;
pub(crate) use selection::*;

const APP_MENU_LABELS: [AppMenu; 7] = [
    AppMenu::File,
    AppMenu::Edit,
    AppMenu::View,
    AppMenu::Document,
    AppMenu::Library,
    AppMenu::Tools,
    AppMenu::Help,
];
const APP_MENU_BUTTON_SPACING: f32 = 2.0;
const APP_MENU_PANEL_SPACING: f32 = 2.0;
const APP_MENU_SEPARATOR_HEIGHT: f32 = 1.0;

pub(crate) fn app_menu_action_message(app: &PDFolioApp, action: AppMenuAction) -> Option<Message> {
    if matches!(
        action,
        AppMenuAction::SetViewerScrollMode(_) | AppMenuAction::SetViewerSpreadMode(_)
    ) {
        return None;
    }

    Some(match action {
        AppMenuAction::OpenFile => Message::OpenFileDialog,
        AppMenuAction::ImportFolder => Message::ImportFolderDialog,
        AppMenuAction::ImportRaindrop => Message::ImportRaindrop,
        AppMenuAction::BackToLibrary => Message::BackToLibrary,
        AppMenuAction::RefreshLibrary => Message::LibraryRefresh,
        AppMenuAction::SelectAllVisible => Message::SelectAllVisibleLibraryEntries,
        AppMenuAction::ClearSelection => Message::ClearLibrarySelection,
        AppMenuAction::SaveDetails => Message::SaveDetailsMetadata,
        AppMenuAction::ResetDetails => {
            let entry_id = app.library.details_entry_id.clone()?;
            Message::RequestConfirmation(ConfirmationAction::ResetDetailsMetadata(entry_id))
        }
        AppMenuAction::AddTag => Message::BulkAddTag,
        AppMenuAction::RemoveTag => Message::BulkRemoveTag,
        AppMenuAction::AddToFolder => Message::BulkAddToCurrentFolder,
        AppMenuAction::RemoveFromFolder => Message::BulkRemoveFromCurrentFolder,
        AppMenuAction::DeleteFromLibrary => {
            Message::RequestConfirmation(ConfirmationAction::BulkDeleteFromLibrary)
        }
        AppMenuAction::ToggleLayout => Message::ToggleViewMode,
        AppMenuAction::ToggleTheme => Message::ThemeToggled,
        AppMenuAction::ReloadStyles => Message::ReloadStyles,
        AppMenuAction::ToggleToc => Message::ToggleSidebar,
        AppMenuAction::JumpToPage => Message::OpenJumpDialog,
        AppMenuAction::FindInDocument => Message::OpenViewerFind,
        AppMenuAction::ZoomIn => Message::ZoomIn,
        AppMenuAction::ZoomOut => Message::ZoomOut,
        AppMenuAction::ResetZoom => Message::ZoomPresetSelected(ZoomPreset::Automatic),
        AppMenuAction::SetViewerScrollMode(_) | AppMenuAction::SetViewerSpreadMode(_) => {
            return None;
        }
        AppMenuAction::SortLibrary(sort_mode) => Message::LibrarySortChanged(sort_mode),
        AppMenuAction::CreateFolder => Message::OpenCreateFolderDialog,
        AppMenuAction::ResetMetadata => {
            Message::RequestConfirmation(ConfirmationAction::BulkResetDisplayMetadata)
        }
        AppMenuAction::SortTitles => Message::BulkApplyTitleSortCleanup,
        AppMenuAction::RefreshMetadata => Message::BulkRefreshPdfMetadata,
        AppMenuAction::RebuildThumbnails => Message::BulkRebuildThumbnails,
        AppMenuAction::Reindex => Message::BulkReindex,
    })
}

pub(crate) fn view_app_menu_bar(app: &PDFolioApp) -> Element<'_, Message> {
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let labels = app.labels();
    let mut menus = row![]
        .spacing(APP_MENU_BUTTON_SPACING)
        .padding([0.0, Spacing::MD])
        .height(app.layout().app_menu_bar_height)
        .align_y(iced::Alignment::Center);

    for menu in APP_MENU_LABELS {
        let active = app.chrome.open_app_menu == Some(menu);
        menus = menus.push(app_menu_button(menu, active, tokens, labels));
    }

    let content: Element<'_, Message> =
        if app.mode == AppMode::Library && !app.library.selected_library_entries.is_empty() {
            column![menus, view_selection_context_row(app, tokens)]
                .spacing(0)
                .into()
        } else {
            menus.into()
        };

    container(content)
        .width(Length::Fill)
        .style(move |_| container_style(tokens, Class::MenuBar))
        .into()
}

pub(crate) fn app_menu_bar_height(app: &PDFolioApp) -> f32 {
    if app.mode == AppMode::Library && !app.library.selected_library_entries.is_empty() {
        app.layout().app_menu_bar_height + app.layout().selection_context_row_height
    } else {
        app.layout().app_menu_bar_height
    }
}

pub(crate) fn app_menu_button<'a>(
    menu: AppMenu,
    active: bool,
    tokens: ThemeTokens,
    labels: &'a crate::style::AppLabelTokens,
) -> Element<'a, Message> {
    let menu_text_color = tokens.class_styles[Class::LibraryControlBar.index()]
        .resolve(ComponentState::Normal)
        .text_color
        .unwrap_or(tokens.text_secondary);
    button(
        container(
            text(app_menu_label(labels, menu))
                .size(FontSize::MD)
                .font(ui_font(FontWeight::MEDIUM))
                .color(menu_text_color)
                .wrapping(Wrapping::None),
        )
        .height(Length::Shrink)
        .center_y(Length::Shrink),
    )
    .padding([0.0, Spacing::MD])
    .width(app_menu_button_width(menu))
    .height(24.0)
    .on_press(Message::AppMenuOpened(menu))
    .style(move |_, status| {
        if active {
            let active_style =
                tokens.class_styles[Class::MenuButton.index()].resolve(ComponentState::Active);
            crate::style::button_style(tokens, Class::MenuButton, status)
                .with_visual_override(active_style)
        } else {
            let mut style = crate::style::button_style(tokens, Class::MenuButton, status);
            style.border.width = 0.0;
            style
        }
    })
    .into()
}

pub(crate) fn app_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::AppMenuClosed),
    )
    .y(app.layout().app_menu_bar_height)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(crate) fn selection_menu_capture_layer<'a>(app: &PDFolioApp) -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::SelectionMenuClosed),
    )
    .y(app_menu_bar_height(app))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(crate) fn view_app_menu_dropdown(
    app: &PDFolioApp,
    tokens: ThemeTokens,
) -> Element<'_, Message> {
    let Some(menu) = app.chrome.open_app_menu else {
        return container("").into();
    };
    let menu_x = app_menu_x(menu);
    let menu_y = app.layout().app_menu_bar_height;
    let mut dropdown = stack![pin(app_menu_panel(app, menu, tokens)).x(menu_x).y(menu_y)]
        .width(Length::Fill)
        .height(Length::Fill);

    if menu == AppMenu::View {
        if let Some(flyout) = app.chrome.open_view_menu_flyout {
            dropdown = dropdown.push(
                pin(view_menu_flyout_panel(app, flyout, tokens))
                    .x(menu_x + app.layout().app_menu_panel_width)
                    .y(menu_y + view_menu_flyout_y_offset(app, flyout)),
            );
        }
    }

    dropdown.into()
}

pub(crate) fn app_menu_x(menu: AppMenu) -> f32 {
    let mut x = Spacing::MD;
    for candidate in APP_MENU_LABELS {
        if candidate == menu {
            break;
        }
        x += app_menu_button_width(candidate) + APP_MENU_BUTTON_SPACING;
    }
    x
}

pub(crate) fn app_menu_button_width(menu: AppMenu) -> f32 {
    match menu {
        AppMenu::File => 48.0,
        AppMenu::Edit => 48.0,
        AppMenu::View => 56.0,
        AppMenu::Document => 88.0,
        AppMenu::Library => 68.0,
        AppMenu::Tools => 58.0,
        AppMenu::Help => 56.0,
    }
}

pub(crate) fn app_menu_panel<'a>(
    app: &'a PDFolioApp,
    menu: AppMenu,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let labels = app.labels();
    let mut panel = column![].spacing(2.0).padding(Spacing::XS);
    match menu {
        AppMenu::File => {
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "OpenFile", "Open PDF..."),
                    "Ctrl+O",
                    true,
                    AppMenuAction::OpenFile,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportFolder", "Import Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportRaindrop", "Import from Raindrop.io..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportRaindrop,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshLibrary", "Refresh Library"),
                    "F5",
                    app.mode == AppMode::Library,
                    AppMenuAction::RefreshLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "BackToLibrary", "Back to Library"),
                    "Esc",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::BackToLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Edit => {
            let has_selection = !app.library.selected_library_entries.is_empty();
            let single_selection = app.library.selected_library_entries.len() == 1;
            let has_bulk_tag = has_selection && !app.library.bulk_tag_input.trim().is_empty();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "SelectAllVisible", "Select All Visible PDFs"),
                    "Ctrl+A",
                    app.mode == AppMode::Library,
                    AppMenuAction::SelectAllVisible,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ClearSelection", "Clear Selection"),
                    "Esc",
                    has_selection,
                    AppMenuAction::ClearSelection,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "SaveDetails", "Save Details"),
                    "Enter",
                    single_selection,
                    AppMenuAction::SaveDetails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetDetails", "Reset Details..."),
                    "",
                    single_selection,
                    AppMenuAction::ResetDetails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "AddTag", "Add Typed Tag"),
                    "",
                    has_bulk_tag,
                    AppMenuAction::AddTag,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RemoveTag", "Remove Typed Tag"),
                    "",
                    has_bulk_tag,
                    AppMenuAction::RemoveTag,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "DeleteFromLibrary", "Delete From Library..."),
                    "Delete",
                    has_selection,
                    AppMenuAction::DeleteFromLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::View => {
            panel = panel
                .push(app_menu_item(
                    if app.library.compact_view_mode {
                        app_menu_action_label(labels, "ToggleLayoutGrid", "Switch to Grid")
                    } else {
                        app_menu_action_label(labels, "ToggleLayoutList", "Switch to List")
                    },
                    "",
                    true,
                    AppMenuAction::ToggleLayout,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    match app.appearance.theme {
                        AppTheme::Light => {
                            app_menu_action_label(labels, "ToggleThemeDark", "Switch to Dark Theme")
                        }
                        AppTheme::Dark => app_menu_action_label(
                            labels,
                            "ToggleThemeLight",
                            "Switch to Light Theme",
                        ),
                    },
                    "",
                    true,
                    AppMenuAction::ToggleTheme,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ReloadStyles", "Reload Styles"),
                    "",
                    true,
                    AppMenuAction::ReloadStyles,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    if app.viewer.toc_open {
                        app_menu_action_label(labels, "ToggleTocHide", "Hide Table of Contents")
                    } else {
                        app_menu_action_label(labels, "ToggleTocShow", "Show Table of Contents")
                    },
                    "",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ToggleToc,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_submenu_item(
                    app_menu_action_label(labels, "ViewerScrolling", "Scrolling"),
                    app.viewer.viewer_scroll_mode.label(),
                    app.mode == AppMode::Viewer,
                    ViewMenuFlyout::Scrolling,
                    app.chrome.open_view_menu_flyout == Some(ViewMenuFlyout::Scrolling),
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_submenu_item(
                    app_menu_action_label(labels, "ViewerSpreads", "Spreads"),
                    app.viewer.viewer_spread_mode.label(),
                    app.mode == AppMode::Viewer,
                    ViewMenuFlyout::Spreads,
                    app.chrome.open_view_menu_flyout == Some(ViewMenuFlyout::Spreads),
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomIn", "Zoom In"),
                    "Ctrl++",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomIn,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomOut", "Zoom Out"),
                    "Ctrl+-",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomOut,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetZoom", "Reset Zoom"),
                    "Ctrl+0",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ResetZoom,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Document => {
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "JumpToPage", "Jump to Page..."),
                    "Ctrl+G",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::JumpToPage,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "FindInDocument", "Find in Document"),
                    "Ctrl+F",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::FindInDocument,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    if app.viewer.toc_open {
                        app_menu_action_label(labels, "ToggleTocHide", "Hide Table of Contents")
                    } else {
                        app_menu_action_label(labels, "ToggleTocShow", "Show Table of Contents")
                    },
                    "",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ToggleToc,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomIn", "Zoom In"),
                    "Ctrl++",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomIn,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ZoomOut", "Zoom Out"),
                    "Ctrl+-",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ZoomOut,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetZoom", "Reset Zoom"),
                    "Ctrl+0",
                    app.mode == AppMode::Viewer,
                    AppMenuAction::ResetZoom,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Library => {
            let has_selection = !app.library.selected_library_entries.is_empty();
            let has_active_folder = app.library.selected_folder.is_some();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportFolder", "Import Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ImportRaindrop", "Import from Raindrop.io..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::ImportRaindrop,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshLibrary", "Refresh Library"),
                    "F5",
                    app.mode == AppMode::Library,
                    AppMenuAction::RefreshLibrary,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "CreateFolder", "New Folder..."),
                    "",
                    app.mode == AppMode::Library,
                    AppMenuAction::CreateFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "AddToFolder", "Add Selection to Current Folder"),
                    "",
                    has_selection && has_active_folder,
                    AppMenuAction::AddToFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(
                        labels,
                        "RemoveFromFolder",
                        "Remove Selection from Current Folder",
                    ),
                    "",
                    has_selection && has_active_folder,
                    AppMenuAction::RemoveFromFolder,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens));
            for sort_mode in LIBRARY_SORT_OPTIONS {
                panel = panel.push(app_menu_item(
                    sort_mode.label(),
                    if app.library.library_sort_mode == sort_mode {
                        label_text(labels, "sort_selected", "Selected")
                    } else {
                        ""
                    },
                    app.mode == AppMode::Library,
                    AppMenuAction::SortLibrary(sort_mode),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
            }
        }
        AppMenu::Tools => {
            let has_selection = !app.library.selected_library_entries.is_empty();
            panel = panel
                .push(app_menu_item(
                    app_menu_action_label(labels, "SortTitles", "Apply Title Sort Cleanup"),
                    "",
                    has_selection,
                    AppMenuAction::SortTitles,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RefreshMetadata", "Refresh PDF Metadata"),
                    "",
                    has_selection,
                    AppMenuAction::RefreshMetadata,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "ResetMetadata", "Reset Display Metadata..."),
                    "",
                    has_selection,
                    AppMenuAction::ResetMetadata,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_separator(tokens))
                .push(app_menu_item(
                    app_menu_action_label(labels, "RebuildThumbnails", "Rebuild Thumbnails"),
                    "",
                    has_selection,
                    AppMenuAction::RebuildThumbnails,
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_item(
                    app_menu_action_label(labels, "Reindex", "Reindex Full Text"),
                    "",
                    has_selection,
                    AppMenuAction::Reindex,
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
        AppMenu::Help => {
            panel = panel
                .push(app_menu_static_item(
                    label_text(labels, "help_product_name", "PDF-Folio"),
                    label_text(
                        labels,
                        "help_product_detail",
                        "Local PDF library and reader",
                    ),
                    tokens,
                    app.layout().app_menu_item_height,
                ))
                .push(app_menu_static_item(
                    label_text(labels, "help_status_label", "Status"),
                    label_text(
                        labels,
                        "help_status_detail",
                        "No help actions available yet",
                    ),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
        }
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

fn view_menu_flyout_y_offset(app: &PDFolioApp, flyout: ViewMenuFlyout) -> f32 {
    let item_height = app.layout().app_menu_item_height;
    let scrolling_y = Spacing::XS
        + item_height * 4.0
        + APP_MENU_SEPARATOR_HEIGHT * 2.0
        + APP_MENU_PANEL_SPACING * 6.0;
    match flyout {
        ViewMenuFlyout::Scrolling => scrolling_y,
        ViewMenuFlyout::Spreads => scrolling_y + item_height + APP_MENU_PANEL_SPACING,
    }
}

fn view_menu_flyout_panel<'a>(
    app: &'a PDFolioApp,
    flyout: ViewMenuFlyout,
    tokens: ThemeTokens,
) -> Element<'a, Message> {
    let mut panel = column![].spacing(2.0).padding(Spacing::XS);

    match flyout {
        ViewMenuFlyout::Scrolling => {
            for mode in ViewerScrollMode::ALL {
                panel = panel.push(app_menu_item(
                    mode.label(),
                    if app.viewer.viewer_scroll_mode == mode {
                        "Selected"
                    } else {
                        mode.detail()
                    },
                    app.mode == AppMode::Viewer,
                    AppMenuAction::SetViewerScrollMode(mode),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
            }
        }
        ViewMenuFlyout::Spreads => {
            for mode in ViewerSpreadMode::ALL {
                panel = panel.push(app_menu_item(
                    mode.label(),
                    if app.viewer.viewer_spread_mode == mode {
                        "Selected"
                    } else {
                        ""
                    },
                    app.mode == AppMode::Viewer,
                    AppMenuAction::SetViewerSpreadMode(mode),
                    tokens,
                    app.layout().app_menu_item_height,
                ));
            }
        }
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

pub(crate) fn app_menu_item<'a>(
    label: &'a str,
    shortcut: &'a str,
    enabled: bool,
    action: AppMenuAction,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let label_color = if enabled {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let shortcut_color = if enabled {
        tokens.text_secondary
    } else {
        with_alpha(tokens.text_secondary, 0.58)
    };
    let content = row![
        text(label)
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(label_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
        text(shortcut)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(shortcut_color)
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::MD)
    .align_y(iced::Alignment::Center);

    if enabled {
        button(content)
            .height(item_height)
            .width(Length::Fill)
            .padding([Spacing::XS, Spacing::MD])
            .on_press(Message::AppMenuActionSelected(action))
            .style(move |_, status| crate::style::button_style(tokens, Class::MenuItem, status))
            .into()
    } else {
        container(content)
            .height(item_height)
            .width(Length::Fill)
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| {
                let disabled_style =
                    tokens.class_styles[Class::MenuItem.index()].resolve(ComponentState::Disabled);
                container_style(tokens, Class::MenuItem).with_visual_override(disabled_style)
            })
            .into()
    }
}

pub(crate) fn app_menu_submenu_item<'a>(
    label: &'a str,
    value: &'a str,
    enabled: bool,
    flyout: ViewMenuFlyout,
    active: bool,
    tokens: ThemeTokens,
    item_height: f32,
) -> Element<'a, Message> {
    let label_color = if enabled {
        tokens.text_primary
    } else {
        tokens.text_secondary
    };
    let value_color = if enabled {
        tokens.text_secondary
    } else {
        with_alpha(tokens.text_secondary, 0.58)
    };
    let content = row![
        text(label)
            .size(FontSize::MD)
            .font(ui_font(FontWeight::REGULAR))
            .color(label_color)
            .wrapping(Wrapping::None)
            .width(Length::Fill),
        text(value)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::REGULAR))
            .color(value_color)
            .wrapping(Wrapping::None),
        text(">")
            .size(FontSize::SM)
            .font(ui_font(FontWeight::SEMIBOLD))
            .color(value_color)
            .wrapping(Wrapping::None),
    ]
    .spacing(Spacing::SM)
    .align_y(iced::Alignment::Center);

    if enabled {
        mouse_area(
            button(content)
                .height(item_height)
                .width(Length::Fill)
                .padding([Spacing::XS, Spacing::MD])
                .on_press(Message::ViewMenuFlyoutOpened(flyout))
                .style(move |_, status| {
                    let mut style = crate::style::button_style(tokens, Class::MenuItem, status);
                    if active {
                        let active_style = tokens.class_styles[Class::MenuItem.index()]
                            .resolve(ComponentState::Active);
                        style = style.with_visual_override(active_style);
                    }
                    style
                }),
        )
        .on_enter(Message::ViewMenuFlyoutOpened(flyout))
        .into()
    } else {
        container(content)
            .height(item_height)
            .width(Length::Fill)
            .padding([Spacing::XS, Spacing::MD])
            .style(move |_| {
                let disabled_style =
                    tokens.class_styles[Class::MenuItem.index()].resolve(ComponentState::Disabled);
                container_style(tokens, Class::MenuItem).with_visual_override(disabled_style)
            })
            .into()
    }
}

pub(crate) fn app_menu_static_item<'a>(
    label: &'a str,
    detail: &'a str,
    tokens: ThemeTokens,
    _item_height: f32,
) -> Element<'a, Message> {
    container(
        column![
            text(label)
                .size(FontSize::MD)
                .font(ui_font(FontWeight::SEMIBOLD))
                .color(tokens.text_primary),
            text(detail)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary),
        ]
        .spacing(Spacing::XS),
    )
    .width(Length::Fill)
    .padding([Spacing::SM, Spacing::MD])
    .style(move |_| {
        let selected_style =
            tokens.class_styles[Class::MenuItem.index()].resolve(ComponentState::Selected);
        container_style(tokens, Class::MenuItem).with_visual_override(selected_style)
    })
    .into()
}
