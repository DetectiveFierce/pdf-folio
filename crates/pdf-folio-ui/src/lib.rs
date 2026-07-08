//! Top-level application state and launch entrypoint for PDF-Folio.
//!
//! This crate is the main application shell built on the [`iced`] framework.
//! It wires together the library, viewer, and style subsystems into a single
//! [`PDFolioApp`] state machine with an `update`/`view`/`subscription` loop.
//!
//! Key exports:
//!
//! - [`PDFolioApp`] — the root application state holding viewer runtime,
//!   library runtime, chrome state, appearance settings, and the database
//!   handle.
//! - [`run`] — launches the iced application with optional initial file.
//! - [`AppMode`] — switches between the library manager and the PDF viewer.
//! - [`Settings`] — user-configurable application settings.
//! - [`messages`] — the [`Message`] enum and related menu/shortcut types
//!   that drive the update loop.
//!
//! Internal modules are organized into `app/` (state, update, view, layout),
//! `library/` (thumbnails, tasks, filtering, drag-and-drop), `viewer/`
//! (canvas rendering, zoom, outline, text search), and `views/` (top-level
//! view composition).
//!
//! [`iced`]: https://docs.rs/iced

pub use pdf_folio_style as style;
pub use pdf_folio_style::theme;

mod shell;
mod library;
mod ui_components_library;
mod viewer;
mod viewer_crate_state;
pub mod views;

use std::collections::{HashMap, HashSet};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, container, image, mouse_area, pin, scrollable, text, text_input, tooltip, Svg,
};
use iced::widget::{operation, Id};
use iced::{animation, font, keyboard, Animation, Color, ContentFit, Element, Font, Length, Point};
use iced::{clipboard, mouse};
use iced::{Rectangle, Size};
use iced::{Task, Theme};
use pdf_folio_core::{Annotation, OutlineNode, PageTextLayer, PdfDoc, TileCache, TileKey};
#[cfg(test)]
use pdf_folio_db::NewLibraryEntry;
use pdf_folio_db::{
    Db, EntryId, Folder, FolderId, ImportedEntry, LibraryEntry, LibraryLayoutMode,
    LibraryOrganizationSnapshot, LibraryPreferences, LibrarySortMode, LibraryWatchEvent,
};
use pdf_folio_cloud::raindrop::{
    RaindropImportDestination, RaindropImportPhase, RaindropImportPreview, RaindropImportProgress,
    RaindropPdfCandidate,
};

use crate::library::drag::{
    active_folder_drop_target, can_drag_reorder_library as can_drag_reorder_library_for_state,
    drag_auto_scroll_velocity, folder_can_move_into, folder_card_target_at_cursor,
    folder_drop_flash_active_at, reorder_folder_ids_before_target, FolderDragState,
    LibraryDragState, LIBRARY_DRAG_AUTOSCROLL_MAX_DT, LIBRARY_FOLDER_DROP_FLASH_MS,
};
#[cfg(test)]
use crate::library::drag::{
    folder_drop_target_at_cursor, parent_directory_target_at_cursor,
    LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED, LIBRARY_FOLDER_DROP_DWELL_MS,
};
#[cfg(test)]
use crate::library::filters::{
    entry_search_fields_match, library_reading_state, search_match_source_label_for_fields,
};
use crate::library::filters::{
    entry_visible_in_folder_scope, library_entry_reading_state, search_match_source_label,
};
use crate::library::metadata::{
    clean_metadata_input, entry_author, entry_title, file_size_label, last_opened_label,
    library_card_metadata_label, library_row_metadata_label, page_count_label, progress_fraction,
    total_file_size_label,
};
#[cfg(test)]
use crate::library::selection::dragged_placeholder_count;
use crate::library::selection::{
    master_checkbox_state_for_counts, range_selection_ids, reorder_entry_ids_for_drag,
    toggle_selection_entry_id,
};
use crate::library::state::{LibraryMetadataDensity, LibraryReadingFilter};
use crate::library::tasks::{
    add_entries_to_folder_task, apply_watch_event, attribute_pending_metadata_task,
    bulk_delete_metadata_task, bulk_operation_task, bulk_permanently_delete_entries_task,
    bulk_refresh_metadata_task, bulk_reindex_task, bulk_reset_metadata_task,
    bulk_restore_trash_items_task, create_folder_task, delete_folder_task, delete_tag_task,
    edit_metadata_task, export_library_entries_task, import_folder_with_index,
    import_pdf_with_index, move_entries_to_folder_task, move_folder_task,
    paste_library_clipboard_task, permanently_delete_folder_from_trash_task,
    persist_manual_entry_order_task, persist_manual_folder_entry_order_task,
    persist_manual_folder_order_task, relink_entry_task, rename_folder_task, rename_tag_task,
    reset_metadata_task, restore_library_history_snapshot_task, search_library_task,
};
#[cfg(test)]
use crate::library::tasks::{clean_import_title, title_from_path};
use crate::library::thumbnails::{
    bulk_thumbnail_task, load_cached_thumbnail, load_or_render_thumbnail, ThumbnailCacheKey,
    ThumbnailSize, ThumbnailView,
};
#[cfg(test)]
use crate::library::view::{
    duplicate_status_label_for_count, folder_meta_label, folder_sidebar_count_label,
    indeterminate_progress_value,
};
use crate::library::view::{
    folder_cards_per_row, folder_cards_section_height, format_count, masonry_target_index,
    parent_directory_drop_box_height, scroll_library_to_offset_task, shortest_column_index,
};
pub(crate) use shell::constants::*;
pub(crate) use shell::icons::*;
pub use shell::messages;
#[cfg(test)]
pub(crate) use viewer::layout::*;

use crate::shell::messages::{
    ConfirmationAction, ContextMenuAction, ContextMenuTarget, LibrarySidebarTab, Message, Shortcut,
    ViewerSidebarTab,
};
use crate::shell::platform::file_manager_commands;
#[cfg(test)]
use crate::shell::platform::{file_manager_command, file_uri};
#[cfg(test)]
use crate::shell::subscriptions::style_watch_event_should_reload;
use crate::shell::subscriptions::subscription;
use crate::style::{
    button_style, container_style, display_font, empty_state, icon_button, master_checkbox,
    mix_color, progress_bar, scrollable_style, search_input_with_class, section_heading,
    selection_checkbox, side_border, side_border_for_class, sidebar_scrollable_style, tag_pill,
    text_input_style, toc_entry, toolbar_button, ui_font, viewer_primitives, Class, ComponentState,
    FontSize, FontWeight, MasterCheckboxState, Spacing, StyleBook, ThemeTokens, VisualOverride,
    UI_FONT_FAMILY,
};
use crate::theme::AppTheme;
use crate::viewer::canvas::ZoomRenderPolicy;
use crate::viewer::state::{
    RenderedPageView, ViewerFindMatch, ViewerFindState, ViewerScrollMode, ViewerSpreadMode,
    ViewerTextAnchor, ViewerTextSelection,
};
use crate::viewer::tasks::{
    open_document_task, open_library_document_task, render_page, schedule_zoom_render,
};
use crate::viewer::zoom::{
    width_from_percent_input, zoom_percent_label, ZoomPreset, MAX_ZOOM_WIDTH, MIN_ZOOM_WIDTH,
    ZOOM_INPUT_ID,
};
#[cfg(test)]
use notify::EventKind;

use shell::libraries::{
    load_library_registry, LibraryNameDialog, LibraryProfile, LibraryRegistryRuntime,
};
use shell::session::{load_app_session, save_app_session, AppSession};
use shell::sync_auth::{SyncAuthRuntime, SyncAuthState};
use shell::update::{pending_raindrop_rollback_check_task, update};
use shell::view::view;
use crate::ui_components_library::view::with_alpha;

pub use shell::app::*;

/// Launches the PDF-Folio UI.
///
/// # Errors
///
/// Returns an error when startup state cannot be created.
pub fn run(initial_file: Option<PathBuf>) -> Result<()> {
    let launch_started_at = Instant::now();
    let startup_probe_enabled = std::env::var_os("PDF_FOLIO_STARTUP_PROBE").is_some();
    let startup_file = initial_file.clone();
    let startup_session = if startup_file.is_none() {
        match load_app_session() {
            Ok(session) => session,
            Err(error) => {
                tracing::warn!(%error, "Failed to load previous PDF-Folio session");
                None
            }
        }
    } else {
        None
    };
    let initial_size = startup_session
        .as_ref()
        .map(AppSession::window_size)
        .unwrap_or_else(initial_window_size);
    let app = PDFolioApp::with_initial_file_and_session(initial_file, startup_session.clone())?;
    tracing::info!(
        elapsed_ms = launch_started_at.elapsed().as_millis(),
        startup_probe_enabled,
        "PDF-Folio local startup state constructed"
    );

    tracing::info!(
        mode = ?app.mode,
        has_document = app.viewer.doc.is_some(),
        "Initialized PDF-Folio application state"
    );

    let mut application = iced::application(
        move || {
            let app = app.clone();
            let open_task = if app.sync_auth.is_signed_in() {
                startup_file
                    .clone()
                    .or_else(|| startup_session.as_ref()?.viewer.document_path.clone())
                    .map(open_document_task)
                    .unwrap_or_else(Task::none)
            } else {
                Task::none()
            };
            let rollback_task = if app.sync_auth.is_signed_in() {
                pending_raindrop_rollback_check_task()
            } else {
                Task::none()
            };
            let startup_probe_task = if startup_probe_enabled {
                Task::perform(
                    {
                        let launch_started_at = launch_started_at;
                        async move {
                            let probe_started_at = Instant::now();
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            (launch_started_at, probe_started_at, Instant::now())
                        }
                    },
                    |(launch_started_at, probe_started_at, emitted_at)| {
                        Message::StartupResponsivenessProbe {
                            launch_started_at,
                            probe_started_at,
                            emitted_at,
                        }
                    },
                )
            } else {
                Task::none()
            };
            let startup_background_ready_task = Task::perform(
                async {
                    tokio::time::sleep(Duration::from_millis(750)).await;
                },
                |_| Message::StartupBackgroundReady,
            );
            (
                app,
                Task::batch([
                    open_task,
                    rollback_task,
                    startup_probe_task,
                    startup_background_ready_task,
                ]),
            )
        },
        update,
        view,
    )
    .title(PDFolioApp::title)
    .theme(|app: &PDFolioApp| match app.appearance.theme {
        AppTheme::Light => Theme::Light,
        AppTheme::Dark => Theme::Dark,
    });

    for font in pdf_folio_style::BUNDLED_FONT_BYTES {
        application = application.font(*font);
    }

    application
        .default_font(iced::Font::with_name(UI_FONT_FAMILY))
        .antialiasing(false)
        .subscription(subscription)
        .scale_factor(|app| app.viewer.scale_factor)
        .window(iced::window::Settings {
            size: Size::new(initial_size[0], initial_size[1]),
            maximized: true,
            position: iced::window::Position::Centered,
            ..iced::window::Settings::default()
        })
        .run()?;

    Ok(())
}

fn initial_window_size() -> [f32; 2] {
    StyleBook::load()
        .unwrap_or_else(|_| StyleBook::bundled())
        .layout()
        .window_size()
}

fn save_app_session_task(app: &PDFolioApp) -> Task<Message> {
    let session = app.snapshot_session();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || save_app_session(&session)).await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::SessionSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn with_session_save(task: Task<Message>, app: &PDFolioApp) -> Task<Message> {
    Task::batch([task, save_app_session_task(app)])
}

fn open_file_manager_task(path: PathBuf, reveal: bool) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let commands = file_manager_commands(&path, reveal);
                if commands.is_empty() {
                    anyhow::bail!(
                        "Could not determine a containing folder for {}.",
                        path.display()
                    );
                }

                let mut errors = Vec::new();
                for (program, args) in commands {
                    match std::process::Command::new(&program).args(&args).status() {
                        Ok(status) if status.success() => return Ok::<_, anyhow::Error>(()),
                        Ok(status) => {
                            errors.push(format!("{program} exited with status {status}"));
                        }
                        Err(error) => {
                            errors.push(format!("{program}: {error}"));
                        }
                    }
                }

                anyhow::bail!(
                    "File manager command failed for {}. {}",
                    path.display(),
                    errors.join("; ")
                );
            })
            .await?
        },
        |result| match result {
            Ok(()) => Message::LibraryStatus(String::from("File manager opened.")),
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn open_file_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::FileSelected),
    )
}

fn import_folder_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::ImportFolderSelected),
    )
}

fn import_pdf_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        |path| path.map_or(Message::FileDialogCanceled, Message::ImportPdfSelected),
    )
}

fn export_destination_dialog_task() -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        },
        |path| {
            path.map_or(
                Message::FileDialogCanceled,
                Message::ExportDestinationSelected,
            )
        },
    )
}

fn relink_file_dialog_task(entry_id: EntryId) -> Task<Message> {
    Task::perform(
        async {
            rfd::AsyncFileDialog::new()
                .add_filter("PDF documents", &["pdf"])
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        },
        move |path| {
            path.map_or(Message::FileDialogCanceled, |path| {
                Message::RelinkFileSelected {
                    entry_id: entry_id.clone(),
                    path,
                }
            })
        },
    )
}

fn save_library_preferences_task(app: &PDFolioApp) -> Task<Message> {
    let db = Arc::clone(&app.db);
    let preferences = LibraryPreferences {
        sort_mode: app.library.library_sort_mode,
        layout_mode: if app.library.compact_view_mode {
            LibraryLayoutMode::List
        } else {
            LibraryLayoutMode::Grid
        },
        selected_folder: app.library.selected_folder.clone(),
        sidebar_width: app.library.library_tag_sidebar_width,
        grid_zoom: LibraryPreferences::default().grid_zoom,
        visible_metadata_fields: app.library.library_metadata_density.visible_fields(),
        library_tree_root_expanded: app.library.library_tree_root_expanded,
        collapsed_folder_ids: app
            .library
            .collapsed_library_tree_folders
            .iter()
            .cloned()
            .collect(),
    };

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || db.save_library_preferences(&preferences))
                .await??;
            Ok::<_, anyhow::Error>(())
        },
        |result| match result {
            Ok(()) => Message::LibraryPreferencesSaved,
            Err(error) => Message::LibraryError(error.to_string()),
        },
    )
}

fn library_search_match_label(
    app: &PDFolioApp,
    entry: &LibraryEntry,
    entry_id: &EntryId,
) -> Option<String> {
    let query = app.library.search_query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }

    app.library
        .search_hit_pages
        .get(entry_id)
        .map(|page| format!("Match on page {}", u32::from(*page) + 1))
        .or_else(|| search_match_source_label(entry, &query))
}

fn truncated_title<'a>(
    title: String,
    width: f32,
    tokens: ThemeTokens,
    alpha: f32,
    font_size: u32,
) -> Element<'a, Message> {
    let visible = truncate_for_width_with_font(&title, width, 0.0, font_size);
    let is_truncated = visible != title;
    let text_color = with_alpha(tokens.text_primary, alpha);
    let label = text(visible)
        .size(font_size)
        .font(display_font(FontWeight::BOLD))
        .color(text_color)
        .wrapping(Wrapping::None)
        .width(width);

    if !is_truncated {
        return label.into();
    }

    tooltip(
        label,
        container(
            text(title)
                .size(FontSize::SM)
                .color(text_color)
                .wrapping(Wrapping::None),
        )
        .padding(Spacing::SM)
        .style(move |_| container_style(tokens, Class::Tooltip)),
        tooltip::Position::Bottom,
    )
    .delay(Duration::from_millis(600))
    .into()
}

fn truncate_for_width(label: &str, width: f32, reserved_width: f32) -> String {
    truncate_for_width_with_font(label, width, reserved_width, FontSize::SM)
}

fn file_tree_label(label: &str, width: f32, font_size: u32) -> String {
    truncate_for_width_with_font(label, width, 0.0, font_size)
}

fn file_tree_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(UI_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

fn truncate_for_width_with_font(
    label: &str,
    width: f32,
    reserved_width: f32,
    font_size: u32,
) -> String {
    const ELLIPSIS: &str = "...";

    let available = (width - reserved_width).max(0.0);
    let approx_char_width = (font_size as f32 * 0.42).max(4.8);
    let max_chars = (available / approx_char_width).floor().max(0.0) as usize;
    let char_count = label.chars().count();

    if char_count <= max_chars {
        return label.to_owned();
    }

    if max_chars <= ELLIPSIS.len() {
        return ELLIPSIS.chars().take(max_chars).collect();
    }

    let keep = max_chars - ELLIPSIS.len();
    let mut truncated: String = label.chars().take(keep).collect();
    truncated.push_str(ELLIPSIS);
    truncated
}

fn schedule_search(query: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            query
        },
        Message::SearchDebounced,
    )
}

#[cfg(test)]
mod tests;
