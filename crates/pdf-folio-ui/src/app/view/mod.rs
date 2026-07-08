//! App shell and viewer-surface rendering.

use crate::app::commands::{command_matches, library_commands, CommandDanger};
use crate::app::context_menu::{context_menu_capture_layer, view_context_menu_dropdown};
use crate::library::view::{
    chevron_button, floating_folder_drag_preview, floating_library_drag_preview,
    view_confirmation_dialog, view_create_folder_dialog, view_export_dialog,
    view_import_menu_dialog, view_import_review_dialog, view_library,
    view_library_move_picker_dialog, view_raindrop_connect_dialog, view_raindrop_import_dialog,
    view_raindrop_import_progress_dialog, view_tag_manager_dialog,
};
use crate::viewer::canvas::{HistoryRestoreSpinner, ViewerCanvas, ViewerSelectionOverlay};
use crate::viewer::outline::{view_jump_dialog, view_sidebar};
use crate::viewer::zoom::{zoom_control, zoom_menu};
use crate::*;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::{canvas, column, row, stack};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const OVERFLOW_HORIZONTAL_SVG: &[u8] =
    include_bytes!("../../../assets/icons/overflow-horizontal.svg");
const OVERFLOW_VERTICAL_SVG: &[u8] = include_bytes!("../../../assets/icons/overflow-vertical.svg");
static VIEW_PROBE_LOGS: AtomicUsize = AtomicUsize::new(0);

mod library_switcher;
mod viewer_toolbar;

use library_switcher::view_library_switcher;
use viewer_toolbar::{
    view_viewer_toolbar, view_zoom_menu_dropdown, viewer_floating_sidebar_toggle,
    zoom_menu_capture_layer,
};

pub(crate) fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let probe_started_at = std::env::var_os("PDF_FOLIO_STARTUP_PROBE").map(|_| Instant::now());
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let base_content: Element<'_, Message> = if app.mode == AppMode::SignedOut {
        view_signed_out(app, tokens)
    } else if app.mode == AppMode::LibrarySwitcher {
        view_library_switcher(app, tokens)
    } else if app.mode == AppMode::Viewer && app.viewer.doc.is_some() {
        let sidebar: Element<'_, Message> = if app.viewer.toc_open {
            view_sidebar(app).into()
        } else {
            container("").width(Length::Shrink).into()
        };

        let content_size = app.viewer_content_size(app.viewer.viewer_viewport_width);
        let viewer = canvas(ViewerCanvas { app })
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let selection_overlay = canvas(ViewerSelectionOverlay { app })
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let viewer_content = stack![viewer, selection_overlay]
            .width(Length::Fixed(content_size.width))
            .height(Length::Fixed(content_size.height));
        let viewer_scroll = scrollable(viewer_content)
            .id(Id::new(VIEWER_SCROLLABLE_ID))
            .direction(Direction::Both {
                vertical: Scrollbar::default(),
                horizontal: Scrollbar::default(),
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_, status| scrollable_style(tokens, Class::ViewerCanvas, status))
            .on_scroll(|viewport| {
                let offset = viewport.absolute_offset();
                let bounds = viewport.bounds();
                Message::ViewportChanged {
                    horizontal_offset: offset.x,
                    scroll_offset: offset.y,
                    width: bounds.width,
                    height: bounds.height,
                }
            });
        let mut viewer_stack = stack![viewer_scroll]
            .width(Length::Fill)
            .height(Length::Fill);
        if !app.viewer.toc_open {
            viewer_stack = viewer_stack.push(
                pin(viewer_floating_sidebar_toggle(tokens))
                    .x(Spacing::SM)
                    .y(Spacing::SM),
            );
        }
        if app.viewer.viewer_find.open {
            let find_width = app
                .layout()
                .viewer_find_bar_width
                .min((app.viewer.viewer_viewport_width - Spacing::MD * 2.0).max(320.0));
            viewer_stack = viewer_stack.push(viewer_find_anchor(app, tokens, find_width));
        }
        let mut main = column![].spacing(0);
        if let Some(error) = app.viewer.document_error.as_deref() {
            main = main.push(dismissible_error_banner(
                error,
                tokens,
                app.layout(),
                Message::DismissDocumentError,
            ));
        }
        if app.viewer.jump_dialog_open {
            main = main.push(view_jump_dialog(app));
        }
        main = main.push(viewer_stack);

        column![
            view_viewer_toolbar(app),
            row![sidebar, main.width(Length::Fill)].height(Length::Fill)
        ]
        .into()
    } else {
        let mut library_shell = column![];
        if let Some(error) = app.viewer.document_error.as_deref() {
            library_shell = library_shell.push(dismissible_error_banner(
                error,
                tokens,
                app.layout(),
                Message::DismissDocumentError,
            ));
        }
        library_shell.push(view_library(app)).into()
    };

    let menu_content = if app.chrome.command_palette_open {
        stack![
            base_content,
            command_palette_capture_layer(),
            view_command_palette(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.viewer.zoom_menu_open {
        stack![
            base_content,
            zoom_menu_capture_layer(app),
            view_zoom_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else if app.chrome.open_context_menu.is_some() {
        stack![
            base_content,
            context_menu_capture_layer(app),
            view_context_menu_dropdown(app, tokens)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        base_content
    };

    let content = if app.libraries.name_dialog.is_some() {
        stack![menu_content, view_library_name_dialog(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.chrome.pending_confirmation.is_some() {
        stack![menu_content, view_confirmation_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.create_folder_dialog_open {
        stack![menu_content, view_create_folder_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.move_picker.is_some() {
        stack![menu_content, view_library_move_picker_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.import_menu_open {
        stack![menu_content, view_import_menu_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.import_review.is_some() {
        stack![menu_content, view_import_review_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.tag_manager_open {
        stack![menu_content, view_tag_manager_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.export_dialog.is_some()
        || app.library.export_progress.is_some()
        || app.library.last_export_summary.is_some()
    {
        stack![menu_content, view_export_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_connect_dialog_open {
        stack![menu_content, view_raindrop_connect_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_import_dialog_open {
        stack![menu_content, view_raindrop_import_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.library.raindrop_import_progress.is_some() {
        stack![menu_content, view_raindrop_import_progress_dialog(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_folder_drag_preview(app, tokens) {
        stack![menu_content, floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(floating) = floating_library_drag_preview(app, tokens) {
        stack![menu_content, floating]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        menu_content
    };

    let shell: Element<'_, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into();

    let shell = if app.library.library_startup_loading {
        stack![shell, startup_library_loading_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if app.viewer.pending_document_open {
        stack![shell, document_loading_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    };

    let element = if app.library.library_history_restore_started_at.is_some() {
        stack![shell, history_restore_spinner_layer(app, tokens)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        shell
    };
    if let Some(started_at) = probe_started_at {
        if VIEW_PROBE_LOGS.fetch_add(1, Ordering::Relaxed) < 8 {
            tracing::warn!(
                elapsed_ms = started_at.elapsed().as_millis(),
                mode = ?app.mode,
                "PDF-Folio view tree constructed"
            );
        }
    }
    element
}

fn command_palette_capture_layer<'a>() -> Element<'a, Message> {
    pin(
        mouse_area(container("").width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseCommandPalette),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_command_palette(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let panel_width = app
        .layout()
        .metric("CommandPalette", "width", 520.0)
        .min((app.viewer.viewport_width - Spacing::XL * 2.0).max(320.0));
    let list_height = app
        .layout()
        .metric("CommandPalette", "list_height", 420.0)
        .min((app.viewer.viewport_height - Spacing::XL * 2.0 - 148.0).max(180.0));
    let commands = library_commands(app)
        .into_iter()
        .filter(|command| command.visible && command.enabled)
        .filter(|command| command_matches(command.spec, &app.chrome.command_palette_query))
        .collect::<Vec<_>>();

    let input = text_input("Search commands", &app.chrome.command_palette_query)
        .on_input(Message::CommandPaletteQueryChanged)
        .on_submit(Message::CommandPaletteRunSelected)
        .padding([Spacing::SM, Spacing::MD])
        .size(FontSize::MD)
        .font(ui_font(FontWeight::REGULAR))
        .style(move |_, status| text_input_style(tokens, Class::LibrarySearchInput, status))
        .width(Length::Fill);

    let mut list = column![].spacing(Spacing::XS).width(Length::Fill);
    for (index, command) in commands.iter().enumerate() {
        let selected = index == app.chrome.command_palette_selected_index;
        let text_color = if selected {
            tokens.text_primary
        } else if command.spec.danger == CommandDanger::Destructive {
            tokens.error
        } else {
            tokens.text_secondary
        };
        let shortcut = command.spec.shortcut.unwrap_or("");
        let target_label = match command.spec.target {
            crate::app::commands::CommandTargetKind::None => "",
            crate::app::commands::CommandTargetKind::Library => "Library",
            crate::app::commands::CommandTargetKind::Folder => "Folder",
            crate::app::commands::CommandTargetKind::Tag => "Tag",
            crate::app::commands::CommandTargetKind::SinglePdf => "PDF",
            crate::app::commands::CommandTargetKind::MultiplePdfs => "Selection",
            crate::app::commands::CommandTargetKind::SearchResult => "Visible",
            crate::app::commands::CommandTargetKind::Viewer => "Viewer",
            crate::app::commands::CommandTargetKind::Document => "Document",
        };
        let icon_slot = if command.spec.icon.is_some() {
            "•"
        } else {
            ""
        };
        let row_content = row![
            text(icon_slot)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::MEDIUM))
                .color(tokens.text_secondary)
                .width(Length::Fixed(app.layout().metric(
                    "CommandPalette",
                    "icon_slot_width",
                    12.0,
                ))),
            column![
                text(command.spec.label)
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::MEDIUM))
                    .color(text_color)
                    .wrapping(Wrapping::None),
                text(
                    format!("{} {}", command.spec.category.label(), target_label)
                        .trim()
                        .to_owned()
                )
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
            ]
            .spacing(
                app.layout()
                    .metric("CommandPalette", "metadata_spacing", 1.0,)
            )
            .width(Length::Fill),
            text(shortcut)
                .size(FontSize::SM)
                .font(ui_font(FontWeight::REGULAR))
                .color(tokens.text_secondary)
                .wrapping(Wrapping::None),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center);
        list = list.push(
            button(row_content)
                .padding([Spacing::SM, Spacing::MD])
                .width(Length::Fill)
                .on_press(Message::CommandPaletteRun(command.spec.id))
                .style(move |_, status| {
                    let class = if selected {
                        Class::MenuButton
                    } else {
                        Class::MenuItem
                    };
                    button_style(tokens, class, status)
                }),
        );
    }
    if commands.is_empty() {
        list = list.push(
            container(
                text("No commands found")
                    .size(FontSize::MD)
                    .font(ui_font(FontWeight::REGULAR))
                    .color(tokens.text_secondary),
            )
            .padding(Spacing::MD),
        );
    }

    let list_scroll = scrollable(list)
        .direction(Direction::Vertical(
            Scrollbar::new()
                .width(tokens.primitives.scrollbar_width)
                .scroller_width(tokens.primitives.scrollbar_scroller_width)
                .anchor(Anchor::End),
        ))
        .height(list_height)
        .width(Length::Fill)
        .style(move |_, status| scrollable_style(tokens, Class::MenuPanel, status));

    let panel = column![
        text("Command Palette")
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        input,
        list_scroll,
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG)
    .width(panel_width);

    pin(container(
        container(panel)
            .width(panel_width)
            .style(move |_| container_style(tokens, Class::MenuPanel)),
    )
    .center(Length::Fill))
    .into()
}

fn view_signed_out(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let signing_in = matches!(app.sync_auth.state, SyncAuthState::SigningIn);
    let button_label = if signing_in {
        "Signing in..."
    } else {
        "Sign in with Google"
    };
    let mut action = button(text(button_label).size(FontSize::MD))
        .padding([10, 16])
        .style(move |_, status| button_style(tokens, Class::LibraryImportButton, status));
    if !signing_in {
        action = action.on_press(Message::SyncSignInRequested);
    }

    let status_text = match &app.sync_auth.state {
        SyncAuthState::WrongAccount { email: Some(email) } => format!(
            "Signed in as {email}. This library is locked to {}.",
            app.sync_auth.expected_email
        ),
        SyncAuthState::WrongAccount { email: None } => {
            format!(
                "This library is locked to {}.",
                app.sync_auth.expected_email
            )
        }
        SyncAuthState::SigningIn => String::from("Waiting for Google sign-in to finish..."),
        _ => format!(
            "Sign in as {} to open your library.",
            app.sync_auth.expected_email
        ),
    };

    let mut panel = column![
        text("PDF-Folio")
            .size(FontSize::HEADING)
            .wrapping(Wrapping::None),
        text(status_text)
            .size(FontSize::MD)
            .wrapping(Wrapping::Word),
        action
    ]
    .spacing(Spacing::MD)
    .align_x(iced::Alignment::Center)
    .width(Length::Fixed(app.layout().metric(
        "SignedOutPanel",
        "width",
        420.0,
    )));

    if let Some(error) = app.sync_auth.error.as_deref() {
        panel = panel.push(text(error).size(FontSize::SM).wrapping(Wrapping::Word));
    }

    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(Spacing::LG)
        .style(move |_| container_style(tokens, Class::AppShell))
        .into()
}

fn view_library_name_dialog(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(dialog) = app.libraries.name_dialog.as_ref() else {
        return container("").into();
    };
    let (title, confirm_label) = match dialog {
        LibraryNameDialog::Create => ("Create Library", "Create"),
        LibraryNameDialog::Rename(_) => ("Rename Library", "Rename"),
    };
    let dialog = column![
        text(title)
            .size(FontSize::HEADING)
            .font(display_font(FontWeight::MEDIUM))
            .color(tokens.text_primary),
        text_input("Library name", &app.libraries.new_library_name)
            .id(Id::new(LIBRARY_NAME_DIALOG_INPUT_ID))
            .on_input(Message::NewLibraryNameChanged)
            .on_submit(Message::ConfirmLibraryNameDialog)
            .style(move |_, status| text_input_style(tokens, Class::SearchInput, status))
            .width(Length::Fill),
        row![
            toolbar_button("Cancel", tokens).on_press(Message::CancelLibraryNameDialog),
            container("").width(Length::Fill),
            toolbar_button(confirm_label, tokens).on_press(Message::ConfirmLibraryNameDialog),
        ]
        .spacing(Spacing::SM)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(Spacing::MD)
    .padding(Spacing::LG);

    container(
        container(dialog)
            .width(app.layout().metric("LibraryNameDialog", "width", 360.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(move |_| container_style(tokens, Class::PresentationOverlay))
    .into()
}

fn viewer_find_anchor(app: &PDFolioApp, tokens: ThemeTokens, width: f32) -> Element<'_, Message> {
    container(view_viewer_find_bar(app, tokens, width))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

fn view_viewer_find_bar(app: &PDFolioApp, tokens: ThemeTokens, width: f32) -> Element<'_, Message> {
    let current = app.viewer.viewer_find.selected.map_or(0, |index| index + 1);
    let total = app.viewer.viewer_find.matches.len();
    let fraction = format!("{current}/{total}");

    let content = row![
        search_input_with_class(
            "Find in Text",
            &app.viewer.viewer_find.query,
            tokens,
            Class::ViewerFindInput,
            Message::ViewerFindQueryChanged,
        )
        .id(Id::new(VIEWER_FIND_INPUT_ID))
        .on_submit(Message::ViewerFindNext)
        .width(Length::Fixed(app.layout().metric(
            "ViewerFindBar",
            "input_width",
            140.0
        ),)),
        text(fraction)
            .size(FontSize::SM)
            .font(ui_font(FontWeight::MEDIUM))
            .color(tokens.text_secondary)
            .wrapping(Wrapping::None)
            .width(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "counter_width",
                44.0
            ),)),
        viewer_find_icon_button(app.layout(), CHEVRON_UP_SVG, "Previous match", tokens)
            .on_press(Message::ViewerFindPrevious),
        viewer_find_icon_button(app.layout(), CHEVRON_DOWN_SVG, "Next match", tokens)
            .on_press(Message::ViewerFindNext),
        checkbox(app.viewer.viewer_find.highlight_all)
            .label("Highlight All")
            .on_toggle(Message::ViewerFindHighlightAllToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_case)
            .label("Match Case")
            .on_toggle(Message::ViewerFindMatchCaseToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        checkbox(app.viewer.viewer_find.match_diacritics)
            .label("Match Diacritics")
            .on_toggle(Message::ViewerFindMatchDiacriticsToggled)
            .size(app.layout().metric("ViewerFindBar", "checkbox_size", 16.0))
            .text_size(FontSize::SM),
        icon_button("x", tokens)
            .on_press(Message::CloseViewerFind)
            .width(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "button_size",
                30.0
            ),))
            .height(Length::Fixed(app.layout().metric(
                "ViewerFindBar",
                "button_size",
                30.0
            ),)),
    ]
    .spacing(Spacing::XS)
    .padding([Spacing::XS, Spacing::SM])
    .height(app.layout().viewer_find_bar_height)
    .align_y(iced::Alignment::Center);

    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(app.layout().viewer_find_bar_height))
        .style(move |_| container_style(tokens, Class::ViewerFindBar))
        .into()
}

fn viewer_find_icon_button<'a>(
    layout: &crate::style::AppLayoutTokens,
    icon: &'static [u8],
    label: &'static str,
    tokens: ThemeTokens,
) -> iced::widget::Button<'a, Message> {
    button(
        tooltip(
            container(
                Svg::new(iced::widget::svg::Handle::from_memory(icon))
                    .width(layout.metric("ViewerFindBar", "icon_size", 16.0))
                    .height(layout.metric("ViewerFindBar", "icon_size", 16.0))
                    .style(move |_, _| iced::widget::svg::Style {
                        color: Some(tokens.text_primary),
                    }),
            )
            .center(Length::Fill),
            label,
            tooltip::Position::Top,
        )
        .style(move |_| container_style(tokens, Class::Tooltip)),
    )
    .width(Length::Fixed(layout.metric(
        "ViewerFindBar",
        "button_size",
        30.0,
    )))
    .height(Length::Fixed(layout.metric(
        "ViewerFindBar",
        "button_size",
        30.0,
    )))
    .padding(layout.metric("ViewerFindBar", "button_padding", 0.0))
    .style(move |_, status| crate::style::button_style(tokens, Class::ViewerFindButton, status))
}

fn history_restore_spinner_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let Some(started_at) = app.library.library_history_restore_started_at else {
        return container("").into();
    };
    let spinner_size = app.layout().metric("HistoryRestoreSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));
    let mut background = tokens.background;
    background.a = 0.54;

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(iced::Background::Color(background)),
                ..iced::widget::container::Style::default()
            }),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

fn document_loading_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let started_at = app
        .viewer
        .document_open_started_at
        .unwrap_or(app.library.animation_now);
    let spinner_size = app.layout().metric("DocumentLoadingSpinner", "size", 48.0);
    let spinner = canvas(HistoryRestoreSpinner {
        started_at,
        now: app.library.animation_now,
        color: tokens.text_primary,
    })
    .width(Length::Fixed(spinner_size))
    .height(Length::Fixed(spinner_size));

    mouse_area(
        container(spinner)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

fn startup_library_loading_layer(app: &PDFolioApp, tokens: ThemeTokens) -> Element<'_, Message> {
    let status = app
        .library
        .raindrop_rollback_recovery_status
        .as_deref()
        .unwrap_or("Preparing library...");
    mouse_area(
        container(
            container(
                column![
                    text("Restoring library")
                        .size(FontSize::HEADING)
                        .font(display_font(FontWeight::MEDIUM))
                        .color(tokens.text_primary),
                    text(status).size(FontSize::MD).color(tokens.text_secondary),
                    container(progress_bar(0.42, tokens)).width(Length::Fill),
                ]
                .spacing(Spacing::MD)
                .padding(Spacing::LG),
            )
            .width(app.layout().metric("StartupLoadingDialog", "width", 460.0))
            .style(move |_| container_style(tokens, Class::JumpOverlay)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center(Length::Fill)
        .style(move |_| container_style(tokens, Class::PresentationOverlay)),
    )
    .interaction(mouse::Interaction::Progress)
    .into()
}

pub(crate) fn dismissible_error_banner<'a>(
    message: &'a str,
    tokens: ThemeTokens,
    layout: &crate::style::AppLayoutTokens,
    dismiss_message: Message,
) -> Element<'a, Message> {
    container(
        row![
            text(message)
                .size(FontSize::MD)
                .color(tokens.text_primary)
                .width(Length::Fill),
            icon_button("x", tokens)
                .on_press(dismiss_message)
                .width(Length::Fixed(layout.metric(
                    "ErrorBannerAction",
                    "action_width",
                    32.0
                ))),
        ]
        .spacing(Spacing::MD)
        .align_y(iced::Alignment::Center),
    )
    .padding(Spacing::MD)
    .width(Length::Fill)
    .style(move |_| container_style(tokens, Class::ErrorBanner))
    .into()
}

fn class_text_color(
    tokens: ThemeTokens,
    class: Class,
    state: ComponentState,
    fallback: iced::Color,
) -> iced::Color {
    tokens.class_styles[class.index()]
        .resolve(state)
        .text_color
        .unwrap_or(fallback)
}
