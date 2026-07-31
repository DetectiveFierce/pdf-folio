//! # Root surface composition
//!
//! Top-level application `view` under `components::shared::root_surface`.
//! Switches on `AppMode` (signed-out, library switcher, library, viewer) and
//! stacks global overlays: command palette, context menus, confirmation and
//! import/export dialogs, error banners, and loading layers.
//!
//! ## Scroll-stable overlay stack
//!
//! Floating chrome (zoom menu, visibility menu, context menu, command palette)
//! always occupies a fixed three-layer stack: base content, capture layer, menu
//! panel. When nothing is open the upper slots are zero-size placeholders.
//! Switching between a bare base and a stack remounts the viewer scrollable and
//! jumps reading position to the origin — keep the stack shape stable.
//!
//! ## Ownership
//!
//! Single entry point composed by the iced `Application::view` path. Domain
//! modules supply library and viewer subtrees; this module only routes and
//! layers chrome. Prefer adding new global overlays here rather than inside
//! domain views so stacking order stays consistent.
//!
//! Related children: [`super::command_palette`], [`super::context_menu`],
//! [`super::loading`], [`super::error_banner`], [`super::menus`].

use crate::components::shared::command_palette::{
    command_palette_capture_layer, view_command_palette,
};
use crate::components::shared::context_menu::{
    context_menu_capture_layer, view_context_menu_dropdown,
};
use crate::components::shared::error_banner::dismissible_error_banner;
use crate::components::shared::loading::{
    document_loading_layer, history_restore_spinner_layer, startup_library_loading_layer,
};
use crate::components::shared::menus::view_library_switcher;
use crate::components::viewer::toolbar::{
    view_visibility_menu_dropdown, view_zoom_menu_dropdown, visibility_menu_capture_layer,
    zoom_menu_capture_layer,
};
use crate::library::view::{
    floating_folder_drag_preview, floating_library_drag_preview, view_confirmation_dialog,
    view_create_folder_dialog, view_export_dialog, view_import_menu_dialog,
    view_import_review_dialog, view_library, view_library_move_picker_dialog,
    view_raindrop_connect_dialog, view_raindrop_import_dialog,
    view_raindrop_import_progress_dialog, view_tag_manager_dialog,
};
use crate::*;
use iced::widget::{column, row, stack, Space};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Cap on startup-probe view timing logs when `PDF_FOLIO_STARTUP_PROBE` is set.
static VIEW_PROBE_LOGS: AtomicUsize = AtomicUsize::new(0);

/// Zero-size non-interactive stack slot so overlay open/close does not remount
/// the base tree (which would reset the viewer scrollable to the origin).
fn overlay_slot_placeholder<'a>() -> Element<'a, Message> {
    Space::new().width(0).height(0).into()
}

/// Compose the full application surface for the current `PDFolioApp` state.
pub(crate) fn view(app: &PDFolioApp) -> Element<'_, Message> {
    let probe_started_at = std::env::var_os("PDF_FOLIO_STARTUP_PROBE").map(|_| Instant::now());
    let tokens = app.appearance.theme.tokens(&app.appearance.style_book);
    let base_content: Element<'_, Message> = if app.mode == AppMode::SignedOut {
        view_signed_out(app, tokens)
    } else if app.mode == AppMode::LibrarySwitcher {
        view_library_switcher(app, tokens)
    } else if app.mode == AppMode::Viewer && app.viewer.doc.is_some() {
        crate::viewer::view::view_viewer(app, tokens)
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

    // Always use a 3-layer stack (base + capture + menu). Switching between a
    // bare base and a stack remounts the viewer scrollable and wipes scroll.
    let (overlay_capture, overlay_menu): (Element<'_, Message>, Element<'_, Message>) =
        if app.chrome.command_palette_open {
            (
                command_palette_capture_layer(),
                view_command_palette(app, tokens),
            )
        } else if app.viewer.zoom_menu_open {
            (
                zoom_menu_capture_layer(app),
                view_zoom_menu_dropdown(app, tokens),
            )
        } else if app.viewer.visibility_menu_open {
            (
                visibility_menu_capture_layer(app),
                view_visibility_menu_dropdown(app, tokens),
            )
        } else if app.chrome.open_context_menu.is_some() {
            (
                context_menu_capture_layer(app),
                view_context_menu_dropdown(app, tokens),
            )
        } else {
            (overlay_slot_placeholder(), overlay_slot_placeholder())
        };

    let menu_content: Element<'_, Message> = stack![base_content, overlay_capture, overlay_menu]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

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

/// Signed-out landing: branding, auth status copy, and Google sign-in action.
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

/// Create/rename library name modal stacked over the switcher or library shell.
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
