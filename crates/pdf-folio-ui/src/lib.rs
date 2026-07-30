//! Top-level application state and launch entrypoint for PDF-Folio.
//!
//! This crate is the iced application shell. It owns the root [`PDFolioApp`]
//! state machine and the `update` / `view` / `subscription` loop that drives
//! both the library manager and the PDF viewer. Domain logic lives in child
//! modules; this file re-exports the public surface, wires launch-time
//! session restore, and holds a few cross-cutting task helpers used by shell,
//! library, and viewer reducers.
//!
//! # Role in the workspace
//!
//! - [`pdf_folio_core`] supplies the SQLite library database, PDF document
//!   runtime, tile cache, and search index.
//! - [`pdf_folio_cloud`] supplies Google sign-in, CRDT sync, Raindrop import,
//!   and the optional sync server client.
//! - [`pdf_folio_style`] supplies the KDL style book, theme tokens, and
//!   reusable widget styling helpers re-exported here as [`style`] / [`theme`].
//!
//! # Key public types
//!
//! - [`PDFolioApp`] — root state: mode, viewer/library/chrome/appearance
//!   runtimes, settings, sync auth, and the active database handle.
//! - [`run`] — boots iced with optional CLI file path and previous session.
//! - [`AppMode`] — signed-out gate, library, viewer, or library switcher.
//! - [`Settings`] — default zoom, tile cache size, and watch directories.
//! - [`messages`] — the crate-wide [`Message`] vocabulary and related
//!   context-menu / confirmation / shortcut enums.
//! - [`ViewerRuntime`] — open-document state re-exported from the viewer.
//!
//! # Module ownership
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`shell`] | `PDFolioApp`, messages, top-level update, subscriptions, session, shortcuts, command registry, platform helpers |
//! | [`library`] | Library filtering, drag/selection, bulk tasks, multi-library registry, library views |
//! | [`viewer`] | Document open/render, zoom/scroll/find/outline, viewer update and canvas composition |
//! | [`components`] | Presentational widgets shared by library and viewer chrome |
//!
//! Message ownership is intentional: library and viewer updaters get first
//! crack at each [`Message`]; shell `update` handles the remainder (sync,
//! chrome, theme, file dialogs, session). Prefer extending an existing
//! message cluster over inventing a parallel channel.
//!
//! [`iced`]: https://docs.rs/iced

pub use pdf_folio_style as style;
pub use pdf_folio_style::theme;

/// Presentational widgets shared by library and viewer chrome.
mod components;
/// Library filtering, drag/selection, bulk tasks, registry, and views.
mod library;
/// Root app state, messages, update, session, shortcuts, and subscriptions.
mod shell;
/// Document open/render, zoom/scroll/find/outline, and viewer canvas.
mod viewer;

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
use pdf_folio_cloud::raindrop::{
    RaindropImportDestination, RaindropImportPhase, RaindropImportPreview, RaindropImportProgress,
    RaindropPdfCandidate,
};
#[cfg(test)]
use pdf_folio_core::NewLibraryEntry;
use pdf_folio_core::{
    Db, EntryId, Folder, FolderId, ImportedEntry, LibraryEntry, LibraryLayoutMode,
    LibraryOrganizationSnapshot, LibraryPreferences, LibrarySortMode, LibraryWatchEvent,
};
use pdf_folio_core::{OutlineNode, PageTextLayer, PdfDoc, TileCache, TileKey};

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
    bulk_restore_trash_items_task, clear_pending_raindrop_rollback, create_folder_task,
    delete_folder_task, delete_tag_task, edit_metadata_task, export_entries_for_source,
    export_library_entries_task, import_folder_with_index, import_pdf_with_index,
    import_review_from_summary, load_pending_raindrop_rollback, move_entries_to_folder_task,
    move_folder_task, paste_library_clipboard_task, pending_raindrop_rollback_check_task,
    permanently_delete_folder_from_trash_task, persist_manual_entry_order_task,
    persist_manual_folder_entry_order_task, persist_manual_folder_order_task,
    raindrop_import_destination, raindrop_import_preserves_structure, raindrop_import_root_folder,
    raindrop_import_task, raindrop_thumbnail_task, relink_entry_task, rename_folder_task,
    rename_tag_task, reset_metadata_task, restore_library_history_snapshot_task,
    rollback_pending_raindrop_import_task, save_pending_raindrop_rollback, search_library_task,
    PendingRaindropRollback,
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
pub(crate) use components::shared::icons::*;
pub(crate) use shell::constants::*;
pub use shell::messages;
#[cfg(test)]
pub(crate) use viewer::layout::*;

use crate::shell::messages::{
    ConfirmationAction, ContextMenuAction, ContextMenuTarget, LibrarySidebarTab, Message,
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
use crate::viewer::rendering::ZoomRenderPolicy;
use crate::viewer::rendering::{
    width_from_percent_input, zoom_percent_label, ZoomPreset, MAX_ZOOM_WIDTH, MIN_ZOOM_WIDTH,
    ZOOM_INPUT_ID,
};
use crate::viewer::state::{
    RenderedPageView, ViewerFindState, ViewerScrollMode, ViewerSpreadMode, ViewerTextSelection,
};
use crate::viewer::tasks::{
    mark_entry_opened_task, open_document_task, open_library_document_task, render_page,
    schedule_zoom_render,
};
#[cfg(test)]
use notify::EventKind;

use crate::components::library::view::with_alpha;
use components::shared::root_surface::view;
use library::registry::{
    load_library_registry, LibraryNameDialog, LibraryProfile, LibraryRegistryRuntime,
};
use shell::session::{load_app_session, save_app_session, AppSession};
use shell::session::{SyncAuthRuntime, SyncAuthState};
use shell::update::update;

pub use shell::app::*;
pub use viewer::document::ViewerRuntime;

/// Launches the PDF-Folio iced application.
///
/// Loads the previous [`AppSession`] when `initial_file` is `None`, builds
/// [`PDFolioApp`] via session/file restore, then runs the iced event loop with
/// the shared subscription tree and window settings. When a CLI file path is
/// provided, session restore for mode/document is skipped so that path opens
/// immediately.
///
/// # Errors
///
/// Returns an error when startup state cannot be created or iced fails to run.
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

/// Default logical window size from the style book layout, or bundled fallback.
fn initial_window_size() -> [f32; 2] {
    StyleBook::load()
        .unwrap_or_else(|_| StyleBook::bundled())
        .layout()
        .window_size()
}

/// Persists a snapshot of the current app session on a background thread.
///
/// Emits [`Message::SessionSaved`] on success or [`Message::LibraryError`] if
/// serialization or disk write fails. Call after navigation, zoom, or library
/// layout changes that should survive relaunch.
pub(crate) fn save_app_session_task(app: &PDFolioApp) -> Task<Message> {
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

/// Batches an arbitrary task with a session snapshot save.
///
/// Convenience for update handlers that both produce work and mutate
/// restorable UI state (mode, document position, filters, etc.).
pub(crate) fn with_session_save(task: Task<Message>, app: &PDFolioApp) -> Task<Message> {
    Task::batch([task, save_app_session_task(app)])
}

/// Opens the OS file manager at `path`, optionally revealing the file itself.
///
/// Tries platform-specific candidates from [`file_manager_commands`] until one
/// succeeds; emits status or error messages on completion.
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

/// Native file picker for opening a PDF outside the library (viewer open).
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

/// Folder picker for bulk-importing every PDF under a directory tree.
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

/// Native file picker for importing one or more PDFs into the active library.
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

/// Folder picker for the destination when exporting selected library PDFs.
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

/// File picker that rebinds a missing library entry to a new on-disk PDF path.
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

/// Writes the active library's view preferences to the open database.
///
/// Captures sort mode, grid/list layout, selected folder, sidebar width,
/// metadata density, and tree expand/collapse state, then emits
/// [`Message::LibraryPreferencesSaved`] or [`Message::LibraryError`].
pub(crate) fn save_library_preferences_task(app: &PDFolioApp) -> Task<Message> {
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

/// Short label explaining why an entry matched the current library search.
///
/// Prefers full-text hit page numbers when available; otherwise falls back to
/// title/author/path field match labels. Returns `None` when the query is empty.
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

/// Single-line title that ellipsizes to `width` and shows a tooltip when cut.
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

/// Ellipsizes `label` to fit `width` minus `reserved_width` at small UI font size.
fn truncate_for_width(label: &str, width: f32, reserved_width: f32) -> String {
    truncate_for_width_with_font(label, width, reserved_width, FontSize::SM)
}

/// Folder/file tree row label truncated to the available sidebar width.
fn file_tree_label(label: &str, width: f32, font_size: u32) -> String {
    truncate_for_width_with_font(label, width, 0.0, font_size)
}

/// UI font used for library file-tree labels at the given weight.
fn file_tree_font(weight: iced::font::Weight) -> Font {
    Font {
        family: font::Family::Name(UI_FONT_FAMILY),
        weight,
        ..Font::DEFAULT
    }
}

/// Approximates glyph width and ellipsizes `label` so it fits the available pixels.
///
/// Uses a fixed average character width derived from `font_size` rather than
/// measuring glyphs, which is good enough for sidebar and card labels.
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

/// Debounces library search input by 200 ms before re-querying the index.
///
/// Emits [`Message::SearchDebounced`] with the original query string so the
/// library updater can ignore stale generations that finished after a newer
/// keystroke.
pub(crate) fn schedule_search(query: String) -> Task<Message> {
    Task::perform(
        async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            query
        },
        Message::SearchDebounced,
    )
}

/// Crate-root unit tests for launch helpers and shared UI utilities.
#[cfg(test)]
mod tests;
