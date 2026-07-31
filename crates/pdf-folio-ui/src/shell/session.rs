//! Last-session persistence and Google sync auth runtime for PDF-Folio.
//!
//! Serializes enough of [`PDFolioApp`] into `session.json` under the XDG data
//! directory so relaunch restores window size, theme, library filters,
//! selection, and the last open document (page, zoom, find, outline expand).
//! CLI file open skips session document restore so the provided path wins.
//!
//! # Key types
//!
//! - [`AppSession`] — on-disk snapshot schema (versioned).
//! - [`SessionViewer`] — document path, page, scroll, zoom, find, outline.
//! - [`SyncAuthRuntime`] / [`SyncAuthState`] — Google sign-in gate for cloud
//!   features and library access when sync is configured.
//!
//! # Related modules
//!
//! - [`crate::save_app_session_task`] — async wrapper used by updaters.
//! - [`super::update`] — applies sign-in results and session-related messages.
//! - [`crate::library::registry`] — active library id is part of the snapshot.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use iced::Task;
#[cfg(not(test))]
use pdf_folio_cloud::sync::cached_session;
use pdf_folio_cloud::sync::{sign_in_with_google, GoogleAuthConfig, Session};
use pdf_folio_core::{EntryId, FolderId, LibrarySortMode};

use crate::*;

/// On-disk `session.json` schema version; mismatched files are ignored on load.
const SESSION_SCHEMA_VERSION: u16 = 1;

/// Versioned on-disk snapshot of app mode, window, viewer, and library UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AppSession {
    /// Schema version; mismatched values cause the session file to be ignored.
    version: u16,
    /// Id of the vault that was active when the session was written.
    #[serde(default = "default_session_library_id")]
    pub(crate) active_library_id: String,
    /// Full-screen surface to restore (`Library` or `Viewer`).
    mode: SessionMode,
    /// Logical window size captured at last snapshot.
    window: SessionWindow,
    /// Theme id string for appearance restore.
    appearance: SessionAppearance,
    /// Open document path/page/zoom/find state.
    pub(crate) viewer: SessionViewer,
    /// Library layout, filters, selection, and sidebar state.
    library: SessionLibrary,
}

/// Full-screen surface restored from session (`Library` or `Viewer`).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionMode {
    /// Restore into the library manager surface.
    Library,
    /// Restore into the PDF viewer for the saved document.
    Viewer,
}

/// Logical window dimensions captured for the next launch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionWindow {
    /// Logical window width at snapshot time.
    width: f32,
    /// Logical window height at snapshot time.
    height: f32,
}

/// Appearance fields restored from session (currently theme only).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionAppearance {
    /// Theme id (`"light"` / `"dark"`) applied on restore.
    theme: String,
}

/// Viewer portion of an [`AppSession`]: document identity, page, zoom, find.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SessionViewer {
    /// Absolute path of the open PDF, if any.
    pub(crate) document_path: Option<PathBuf>,
    /// Library entry id string for the open document, when known.
    pub(crate) entry_id: Option<String>,
    /// Zero-based page index to restore.
    page: u16,
    /// Vertical scroll offset within the document.
    scroll_offset: f32,
    /// Horizontal scroll/pan offset within the document.
    horizontal_offset: f32,
    /// Viewer scroll mode id (`page`, `vertical`, `horizontal`, `wrapped`).
    scroll_mode: String,
    /// Viewer spread mode id (`none`, `odd`, `even`).
    spread_mode: String,
    /// Rendered page width in logical pixels.
    zoom_width: u16,
    /// Whether the outline / TOC sidebar was open.
    toc_open: bool,
    /// Viewer sidebar tab id (`contents` / `thumbnails`).
    sidebar_tab: String,
    /// Expanded outline node paths for the TOC tree.
    expanded_outline_paths: Vec<Vec<usize>>,
    /// Whether the find-in-document bar was open.
    find_open: bool,
    /// Find-in-document query string.
    find_query: String,
    /// Whether all find matches were highlighted.
    find_highlight_all: bool,
    /// Whether find matching was case-sensitive.
    find_match_case: bool,
    /// Whether find matching respected diacritics.
    find_match_diacritics: bool,
}

/// Library layout, filters, selection, and sidebar state for session restore.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionLibrary {
    /// Compact list layout vs masonry grid.
    compact_view_mode: bool,
    /// Masonry grid card scale factor.
    grid_zoom: f32,
    /// Metadata density id (`minimal` / `standard` / `detailed`).
    metadata_density: String,
    /// Library sort mode as its wire/string form.
    sort_mode: String,
    /// Selected folder id string, or root when `None`.
    selected_folder: Option<String>,
    /// Details/inspector folder id string, if any.
    details_folder_id: Option<String>,
    /// Search box contents.
    search_query: String,
    /// Vertical scroll offset of the library content pane.
    scroll_offset: f32,
    /// Left sidebar width in logical pixels.
    tag_sidebar_width: f32,
    /// Whether the left sidebar was open.
    tag_sidebar_open: bool,
    /// Library sidebar tab id (`files` / `tags`).
    sidebar_tab: String,
    /// Whether the library root tree node was expanded.
    tree_root_expanded: bool,
    /// Folder ids whose tree children were collapsed.
    collapsed_folder_ids: Vec<String>,
    /// Whether the folder details section was open.
    folder_details_sidebar_open: bool,
    /// Active tag filter string, if any.
    active_tag_filter: Option<String>,
    /// Reading-progress filter id (`unread` / `reading` / `finished`), if any.
    active_reading_filter: Option<String>,
    /// Whether only missing-file entries were shown.
    missing_filter_active: bool,
    /// Multi-selected entry id strings.
    selected_entry_ids: Vec<String>,
    /// Shift-selection anchor entry id string, if any.
    selection_anchor: Option<String>,
    /// Details editor entry id string, if any.
    details_entry_id: Option<String>,
}

impl AppSession {
    /// Logical window size restored on the next launch.
    pub(crate) fn window_size(&self) -> [f32; 2] {
        [self.window.width.max(1.0), self.window.height.max(1.0)]
    }

    /// Saved document path only when the session should reopen in viewer mode.
    ///
    /// Library-mode sessions retain the last viewer path for "Back to Viewer",
    /// but opening that PDF during process startup would delay an otherwise
    /// immediately interactive library surface.
    pub(crate) fn viewer_document_to_restore(&self) -> Option<PathBuf> {
        matches!(self.mode, SessionMode::Viewer)
            .then(|| self.viewer.document_path.clone())
            .flatten()
    }
}

impl PDFolioApp {
    /// Captures restorable UI state into an [`AppSession`] for disk write.
    pub(crate) fn snapshot_session(&self) -> AppSession {
        AppSession {
            version: SESSION_SCHEMA_VERSION,
            active_library_id: self.libraries.active_library_id.clone(),
            mode: match self.mode {
                AppMode::SignedOut => SessionMode::Library,
                AppMode::Library => SessionMode::Library,
                AppMode::Viewer => SessionMode::Viewer,
                AppMode::LibrarySwitcher => SessionMode::Library,
            },
            window: SessionWindow {
                width: self.viewer.viewport_width.max(1.0),
                height: self.viewer.viewport_height.max(1.0),
            },
            appearance: SessionAppearance {
                theme: theme_id(self.appearance.theme).to_owned(),
            },
            viewer: SessionViewer {
                document_path: self.viewer.current_document_path.clone(),
                entry_id: self
                    .viewer
                    .current_entry_id
                    .as_ref()
                    .map(|entry_id| entry_id.as_str().to_owned()),
                page: self.current_page(),
                scroll_offset: self.viewer.scroll_offset.max(0.0),
                horizontal_offset: self.viewer.horizontal_offset.max(0.0),
                scroll_mode: viewer_scroll_mode_id(self.viewer.viewer_scroll_mode).to_owned(),
                spread_mode: viewer_spread_mode_id(self.viewer.viewer_spread_mode).to_owned(),
                zoom_width: self.viewer.zoom_width,
                toc_open: self.viewer.toc_open,
                sidebar_tab: viewer_sidebar_tab_id(self.viewer.viewer_sidebar_tab).to_owned(),
                expanded_outline_paths: self
                    .viewer
                    .expanded_outline_paths
                    .iter()
                    .cloned()
                    .collect(),
                find_open: self.viewer.viewer_find.open,
                find_query: self.viewer.viewer_find.query.clone(),
                find_highlight_all: self.viewer.viewer_find.highlight_all,
                find_match_case: self.viewer.viewer_find.match_case,
                find_match_diacritics: self.viewer.viewer_find.match_diacritics,
            },
            library: SessionLibrary {
                compact_view_mode: self.library.compact_view_mode,
                grid_zoom: self.library.library_grid_zoom,
                metadata_density: metadata_density_id(self.library.library_metadata_density)
                    .to_owned(),
                sort_mode: self.library.library_sort_mode.as_str().to_owned(),
                selected_folder: folder_id_to_string(&self.library.selected_folder),
                details_folder_id: folder_id_to_string(&self.library.details_folder_id),
                search_query: self.library.search_query.clone(),
                scroll_offset: self.library.library_scroll_offset.max(0.0),
                tag_sidebar_width: self.library.library_tag_sidebar_width,
                tag_sidebar_open: self.library.library_tag_sidebar_open,
                sidebar_tab: library_sidebar_tab_id(self.library.library_sidebar_tab).to_owned(),
                tree_root_expanded: self.library.library_tree_root_expanded,
                collapsed_folder_ids: self
                    .library
                    .collapsed_library_tree_folders
                    .iter()
                    .map(|folder_id| folder_id.as_str().to_owned())
                    .collect(),
                folder_details_sidebar_open: self.library.folder_details_sidebar_open,
                active_tag_filter: self.library.active_tag_filter.clone(),
                active_reading_filter: self
                    .library
                    .active_reading_filter
                    .map(reading_filter_id)
                    .map(ToOwned::to_owned),
                missing_filter_active: self.library.missing_filter_active,
                selected_entry_ids: self
                    .library
                    .selected_library_entries
                    .iter()
                    .map(|entry_id| entry_id.as_str().to_owned())
                    .collect(),
                selection_anchor: self
                    .library
                    .library_selection_anchor
                    .as_ref()
                    .map(|entry_id| entry_id.as_str().to_owned()),
                details_entry_id: self
                    .library
                    .details_entry_id
                    .as_ref()
                    .map(|entry_id| entry_id.as_str().to_owned()),
            },
        }
    }

    /// Applies pending session library filters/selection after entries load.
    ///
    /// May open a restored library entry in the viewer and always requests
    /// visible thumbnails plus a scroll restore task.
    pub(crate) fn apply_pending_session_to_loaded_library(&mut self) -> Task<Message> {
        let Some(session) = self.pending_session_restore.clone() else {
            return Task::none();
        };

        self.apply_library_session(&session);

        if matches!(session.mode, SessionMode::Viewer) && session.viewer.document_path.is_none() {
            if let Some(entry_id) = session.viewer.entry_id.as_deref().map(EntryId::new) {
                if self
                    .library
                    .library_entries
                    .iter()
                    .any(|entry| entry.id == entry_id)
                {
                    // Keep pending until the document opens so page/zoom restore
                    // can still run via apply_pending_session_to_open_document.
                    return Task::done(Message::OpenLibraryEntry(entry_id));
                }
            }
        }

        // Path-based Viewer restore is handled by the startup open task. If that
        // document is already open, finish pending page/zoom restore now so later
        // LibraryLoaded events (e.g. after Back to Library) cannot re-apply Viewer.
        if matches!(session.mode, SessionMode::Viewer)
            && session.viewer.document_path.is_some()
            && self.viewer.doc.is_some()
            && self.document_matches_session(&session)
        {
            return self.apply_pending_session_to_open_document();
        }

        if !matches!(session.mode, SessionMode::Viewer) || session.viewer.document_path.is_none() {
            self.pending_session_restore = None;
        }

        Task::batch([
            self.request_visible_thumbnails(),
            scroll_library_to_offset_task(self.library.library_scroll_offset),
            save_app_session_task(self),
        ])
    }

    /// Applies pending session page/zoom/find state after a document opens.
    ///
    /// Also re-binds the library [`EntryId`] from the session snapshot (path-based
    /// startup opens leave `current_entry_id` unset) and kicks a background
    /// annotation load so notes appear without blocking the interactive viewer.
    pub(crate) fn apply_pending_session_to_open_document(&mut self) -> Task<Message> {
        let Some(session) = self.pending_session_restore.clone() else {
            return Task::none();
        };

        if !self.document_matches_session(&session) {
            return Task::none();
        }

        self.pending_session_restore = None;
        self.mode = match session.mode {
            SessionMode::Library => AppMode::Library,
            SessionMode::Viewer => AppMode::Viewer,
        };
        // Path-based restore opens via DocumentOpened and clears entry_id; put it
        // back so annotations / reading progress can attach to the library row.
        if self.viewer.current_entry_id.is_none() {
            if let Some(entry_id) = session.viewer.entry_id.as_deref().map(EntryId::new) {
                self.viewer.current_entry_id = Some(entry_id);
            }
        }
        // Prefer library title over a content-hash path once entry_id is restored
        // (library rows may still be empty — ensure_* re-seeds when they load).
        self.seed_provisional_document_title();
        self.viewer.page_scroll_page = session.viewer.page;
        self.viewer.scroll_offset = session.viewer.scroll_offset.max(0.0);
        self.viewer.last_scroll_offset = self.viewer.scroll_offset;
        self.viewer.horizontal_offset = session.viewer.horizontal_offset.max(0.0);
        self.viewer.viewer_scroll_mode = parse_viewer_scroll_mode(&session.viewer.scroll_mode);
        self.viewer.viewer_spread_mode = parse_viewer_spread_mode(&session.viewer.spread_mode);
        self.viewer.zoom_width = session
            .viewer
            .zoom_width
            .clamp(MIN_ZOOM_WIDTH, MAX_ZOOM_WIDTH);
        self.viewer.active_zoom_preset = None;
        self.viewer.zoom_input = zoom_percent_label(self.viewer.zoom_width);
        self.viewer.toc_open = session.viewer.toc_open;
        self.viewer.viewer_sidebar_tab = parse_viewer_sidebar_tab(&session.viewer.sidebar_tab);
        self.viewer.expanded_outline_paths =
            session.viewer.expanded_outline_paths.into_iter().collect();
        self.viewer.viewer_find.open = session.viewer.find_open;
        self.viewer.viewer_find.query = session.viewer.find_query;
        self.viewer.viewer_find.highlight_all = session.viewer.find_highlight_all;
        self.viewer.viewer_find.match_case = session.viewer.find_match_case;
        self.viewer.viewer_find.match_diacritics = session.viewer.find_match_diacritics;
        self.clamp_horizontal_offset();
        self.clamp_scroll_offset();

        Task::batch([
            self.request_visible_pages(),
            self.request_viewer_thumbnail_pages(),
            self.scroll_viewer_to_offsets_task(),
            // Non-blocking spawn_blocking list; UI stays interactive meanwhile.
            self.load_annotations_task(),
            save_app_session_task(self),
        ])
    }

    /// Applies theme, mode, and library UI fields from a loaded session snapshot.
    pub(crate) fn apply_library_session(&mut self, session: &AppSession) {
        self.appearance.theme = parse_theme(&session.appearance.theme);
        self.mode = match session.mode {
            SessionMode::Library => AppMode::Library,
            SessionMode::Viewer => AppMode::Viewer,
        };
        self.library.compact_view_mode = session.library.compact_view_mode;
        self.library.library_grid_zoom = session
            .library
            .grid_zoom
            .clamp(self.library_grid_zoom_min(), self.library_grid_zoom_limit());
        self.library.library_metadata_density =
            parse_metadata_density(&session.library.metadata_density);
        self.library.library_sort_mode = session
            .library
            .sort_mode
            .parse()
            .unwrap_or(LibrarySortMode::RecentlyAdded);
        self.library.selected_folder = valid_folder_id(
            session.library.selected_folder.as_deref(),
            &self.library.library_folders,
        );
        self.library.details_folder_id = valid_folder_id(
            session.library.details_folder_id.as_deref(),
            &self.library.library_folders,
        );
        self.library.search_query = session.library.search_query.clone();
        self.library.search_results = None;
        self.library.search_hit_pages.clear();
        self.library.library_scroll_offset = session.library.scroll_offset.max(0.0);
        self.library.library_tag_sidebar_width = session.library.tag_sidebar_width.clamp(
            self.layout().library_sidebar_min_width,
            self.layout().library_sidebar_max_width,
        );
        self.library.library_tag_sidebar_open = session.library.tag_sidebar_open;
        self.library.library_sidebar_tab = parse_library_sidebar_tab(&session.library.sidebar_tab);
        self.library.library_tree_root_expanded = session.library.tree_root_expanded;
        self.library.collapsed_library_tree_folders = session
            .library
            .collapsed_folder_ids
            .iter()
            .filter_map(|id| valid_folder_id(Some(id), &self.library.library_folders))
            .collect();
        self.library.folder_details_sidebar_open = session.library.folder_details_sidebar_open;
        self.library.active_tag_filter = session.library.active_tag_filter.clone();
        self.library.active_reading_filter = session
            .library
            .active_reading_filter
            .as_deref()
            .map(parse_reading_filter);
        self.library.missing_filter_active = session.library.missing_filter_active;
        self.library.selected_library_entries = session
            .library
            .selected_entry_ids
            .iter()
            .filter_map(|id| valid_entry_id(id, &self.library.library_entries))
            .collect();
        self.library.library_selection_anchor = session
            .library
            .selection_anchor
            .as_deref()
            .and_then(|id| valid_entry_id(id, &self.library.library_entries));
        self.library.details_entry_id = session
            .library
            .details_entry_id
            .as_deref()
            .and_then(|id| valid_entry_id(id, &self.library.library_entries));
        self.sync_folder_rename_input();
        self.sync_details_editor_to_selection();
        if !self.library.search_query.trim().is_empty() {
            self.library.search_generation = self.library.search_generation.wrapping_add(1);
        }
    }

    /// True when the open document matches the session entry id or file path.
    ///
    /// Matches on either identity: path-only startup opens leave `current_entry_id`
    /// unset even when the session snapshot still carries an entry id, so requiring
    /// entry id alone would leave `pending_session_restore` stuck and later library
    /// reloads would re-enter Viewer mode.
    fn document_matches_session(&self, session: &AppSession) -> bool {
        let entry_matches = session.viewer.entry_id.as_deref().is_some_and(|session_entry_id| {
            self.viewer
                .current_entry_id
                .as_ref()
                .is_some_and(|entry_id| entry_id.as_str() == session_entry_id)
        });
        let path_matches = session.viewer.document_path.as_ref().is_some_and(|path| {
            self.viewer
                .current_document_path
                .as_ref()
                .is_some_and(|current| current == path)
        });
        entry_matches || path_matches
    }
}

/// Reads `session.json` if present and schema-compatible; otherwise `Ok(None)`.
pub(crate) fn load_app_session() -> Result<Option<AppSession>> {
    let path = session_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let json = std::fs::read_to_string(&path)
        .with_context(|| format!("Could not read {}.", path.display()))?;
    let session = serde_json::from_str::<AppSession>(&json)
        .with_context(|| format!("Could not parse {}.", path.display()))?;

    if session.version == SESSION_SCHEMA_VERSION {
        Ok(Some(session))
    } else {
        tracing::warn!(
            version = session.version,
            expected = SESSION_SCHEMA_VERSION,
            "Ignoring incompatible PDF-Folio session"
        );
        Ok(None)
    }
}

/// Writes `session` as pretty JSON under the PDF-Folio data directory.
pub(crate) fn save_app_session(session: &AppSession) -> Result<()> {
    let path = session_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}.", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(session)?)
        .with_context(|| format!("Could not write {}.", path.display()))?;
    Ok(())
}

/// Absolute path to `session.json` under the PDF-Folio XDG data directory.
fn session_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().join("session.json"))
}

/// Serializes an optional folder id as its string form for session JSON.
fn folder_id_to_string(folder_id: &Option<FolderId>) -> Option<String> {
    folder_id
        .as_ref()
        .map(|folder_id| folder_id.as_str().to_owned())
}

/// Parses `id` only when it still exists in the loaded folder list.
fn valid_folder_id(id: Option<&str>, folders: &[Folder]) -> Option<FolderId> {
    let id = id?;
    folders
        .iter()
        .any(|folder| folder.id.as_str() == id)
        .then(|| FolderId::new(id))
}

/// Parses `id` only when it still exists among loaded library entries.
fn valid_entry_id(id: &str, entries: &[LibraryEntry]) -> Option<EntryId> {
    entries
        .iter()
        .any(|entry| entry.id.as_str() == id)
        .then(|| EntryId::new(id))
}

/// Wire form of [`AppTheme`] written into session appearance.
fn theme_id(theme: AppTheme) -> &'static str {
    match theme {
        AppTheme::Light => "light",
        AppTheme::Dark => "dark",
    }
}

/// Restores [`AppTheme`] from session JSON; unknown values become dark.
fn parse_theme(value: &str) -> AppTheme {
    match value {
        "light" => AppTheme::Light,
        _ => AppTheme::Dark,
    }
}

/// Wire form of [`ViewerScrollMode`] for session viewer state.
fn viewer_scroll_mode_id(mode: ViewerScrollMode) -> &'static str {
    match mode {
        ViewerScrollMode::Page => "page",
        ViewerScrollMode::Vertical => "vertical",
        ViewerScrollMode::Horizontal => "horizontal",
        ViewerScrollMode::Wrapped => "wrapped",
    }
}

/// Restores scroll mode from session JSON; defaults to continuous vertical.
fn parse_viewer_scroll_mode(value: &str) -> ViewerScrollMode {
    match value {
        "page" => ViewerScrollMode::Page,
        "horizontal" => ViewerScrollMode::Horizontal,
        "wrapped" => ViewerScrollMode::Wrapped,
        _ => ViewerScrollMode::Vertical,
    }
}

/// Wire form of [`ViewerSpreadMode`] for session viewer state.
fn viewer_spread_mode_id(mode: ViewerSpreadMode) -> &'static str {
    match mode {
        ViewerSpreadMode::None => "none",
        ViewerSpreadMode::Odd => "odd",
        ViewerSpreadMode::Even => "even",
    }
}

/// Restores spread mode from session JSON; unknown values mean no spreads.
fn parse_viewer_spread_mode(value: &str) -> ViewerSpreadMode {
    match value {
        "odd" => ViewerSpreadMode::Odd,
        "even" => ViewerSpreadMode::Even,
        _ => ViewerSpreadMode::None,
    }
}

/// Wire form of [`LibrarySidebarTab`] for session library state.
fn library_sidebar_tab_id(tab: LibrarySidebarTab) -> &'static str {
    match tab {
        LibrarySidebarTab::Files => "files",
        LibrarySidebarTab::Tags => "tags",
    }
}

/// Restores the library sidebar tab; unknown values become the files tree.
fn parse_library_sidebar_tab(value: &str) -> LibrarySidebarTab {
    match value {
        "tags" => LibrarySidebarTab::Tags,
        _ => LibrarySidebarTab::Files,
    }
}

/// Wire form of [`ViewerSidebarTab`] for session viewer state.
fn viewer_sidebar_tab_id(tab: ViewerSidebarTab) -> &'static str {
    match tab {
        ViewerSidebarTab::Contents => "contents",
        ViewerSidebarTab::Thumbnails => "thumbnails",
        ViewerSidebarTab::Annotations => "annotations",
    }
}

/// Restores the viewer sidebar tab; unknown values become contents/outline.
fn parse_viewer_sidebar_tab(value: &str) -> ViewerSidebarTab {
    match value {
        "thumbnails" => ViewerSidebarTab::Thumbnails,
        "annotations" => ViewerSidebarTab::Annotations,
        _ => ViewerSidebarTab::Contents,
    }
}

/// Wire form of [`LibraryMetadataDensity`] for session library state.
fn metadata_density_id(density: LibraryMetadataDensity) -> &'static str {
    match density {
        LibraryMetadataDensity::Minimal => "minimal",
        LibraryMetadataDensity::Standard => "standard",
        LibraryMetadataDensity::Detailed => "detailed",
    }
}

/// Restores metadata density; unknown values become standard.
fn parse_metadata_density(value: &str) -> LibraryMetadataDensity {
    match value {
        "minimal" => LibraryMetadataDensity::Minimal,
        "detailed" => LibraryMetadataDensity::Detailed,
        _ => LibraryMetadataDensity::Standard,
    }
}

/// Wire form of [`LibraryReadingFilter`] for session library state.
fn reading_filter_id(filter: LibraryReadingFilter) -> &'static str {
    match filter {
        LibraryReadingFilter::Unread => "unread",
        LibraryReadingFilter::Reading => "reading",
        LibraryReadingFilter::Finished => "finished",
    }
}

/// Restores the reading-progress filter; unknown values become in-progress.
fn parse_reading_filter(value: &str) -> LibraryReadingFilter {
    match value {
        "unread" => LibraryReadingFilter::Unread,
        "finished" => LibraryReadingFilter::Finished,
        _ => LibraryReadingFilter::Reading,
    }
}

/// Serde default for [`AppSession::active_library_id`] when older sessions omit it.
fn default_session_library_id() -> String {
    String::from("default")
}

/// Fallback allow-listed Google email when `PDF_FOLIO_ALLOWED_GOOGLE_EMAIL` is unset.
const DEFAULT_ALLOWED_GOOGLE_EMAIL: &str = "aidanjwagner03@gmail.com";
/// Fallback CRDT sync server base URL when `PDF_FOLIO_SYNC_SERVER` is unset.
const DEFAULT_SYNC_SERVER_BASE_URL: &str = "http://mind-palace:53148";

/// Sync sign-in gate: expected allow-list email, server URL, and auth phase.
///
/// When not signed in, the shell shows a sign-in surface instead of the
/// library. Loaded from cached cloud session on startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAuthRuntime {
    /// Current Google auth phase for the sync gate UI.
    pub state: SyncAuthState,
    /// Allow-listed email address this library is locked to.
    pub expected_email: String,
    /// Base URL of the CRDT sync server.
    pub server_base_url: String,
    /// Last sign-in or allow-list error message for the sign-in surface.
    pub error: Option<String>,
}

/// Auth phase for the Google sync gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAuthState {
    /// No valid session; show the sign-in surface.
    SignedOut,
    /// Browser OAuth is in progress.
    SigningIn,
    /// Active session for the allow-listed email.
    SignedIn {
        /// Email of the signed-in Google account.
        email: String,
        /// RFC3339 expiry timestamp of the cloud session.
        expires_at: String,
    },
    /// A session exists but its email is not on the allow list.
    WrongAccount {
        /// Email from the rejected session, when known.
        email: Option<String>,
    },
}

impl SyncAuthRuntime {
    /// Restores auth from the cached cloud session, or starts signed out.
    pub fn load() -> Self {
        let expected_email = expected_google_email();
        let server_base_url = sync_server_base_url();
        #[cfg(test)]
        return Self::signed_in_for_tests(expected_email, server_base_url);

        #[cfg(not(test))]
        match cached_session() {
            Ok(session) if session_matches_expected_email(&session, &expected_email) => Self {
                state: SyncAuthState::SignedIn {
                    email: expected_email.clone(),
                    expires_at: session.expires_at.to_rfc3339(),
                },
                expected_email,
                server_base_url,
                error: None,
            },
            Ok(session) if session.is_valid() => Self {
                state: SyncAuthState::WrongAccount {
                    email: session.email.clone(),
                },
                expected_email,
                server_base_url,
                error: Some(String::from(
                    "That cached Google session is not allowed for this library.",
                )),
            },
            _ => Self {
                state: SyncAuthState::SignedOut,
                expected_email,
                server_base_url,
                error: None,
            },
        }
    }

    /// Test-only auth runtime that starts already signed in for the expected email.
    #[cfg(test)]
    fn signed_in_for_tests(expected_email: String, server_base_url: String) -> Self {
        Self {
            state: SyncAuthState::SignedIn {
                email: expected_email.clone(),
                expires_at: String::from("test"),
            },
            expected_email,
            server_base_url,
            error: None,
        }
    }

    /// Returns true when sync auth is in the signed-in state.
    pub fn is_signed_in(&self) -> bool {
        matches!(self.state, SyncAuthState::SignedIn { .. })
    }

    /// Applies a successful sync session to auth runtime state.
    pub fn apply_signed_in_session(&mut self, session: Session) -> Result<()> {
        if !session_matches_expected_email(&session, &self.expected_email) {
            self.state = SyncAuthState::WrongAccount {
                email: session.email.clone(),
            };
            anyhow::bail!(
                "Signed in as {}, but PDF-Folio is locked to {}.",
                session
                    .email
                    .as_deref()
                    .unwrap_or(session.google_sub.as_str()),
                self.expected_email
            );
        }

        self.state = SyncAuthState::SignedIn {
            email: session
                .email
                .clone()
                .unwrap_or_else(|| self.expected_email.clone()),
            expires_at: session.expires_at.to_rfc3339(),
        };
        self.error = None;
        Ok(())
    }
}

/// Runs browser Google OAuth and emits [`Message::SyncSignInFinished`].
///
/// Rejects sessions whose email is not the configured allow-list address.
pub(crate) fn sync_sign_in_task(expected_email: String, server_base_url: String) -> Task<Message> {
    Task::perform(
        async move {
            let client_id = load_google_client_id_from_secrets()
                .context("Provide PDF_FOLIO_GOOGLE_CLIENT_ID or a Google client_secret JSON.")?;
            let session = sign_in_with_google(&GoogleAuthConfig {
                client_id,
                sync_server_base_url: server_base_url,
            })
            .await?;
            if !session_matches_expected_email(&session, &expected_email) {
                anyhow::bail!(
                    "Signed in as {}, but PDF-Folio is locked to {}.",
                    session
                        .email
                        .as_deref()
                        .unwrap_or(session.google_sub.as_str()),
                    expected_email
                );
            }
            Ok::<_, anyhow::Error>(session)
        },
        |result| match result {
            Ok(session) => Message::SyncSignInFinished(Ok(session)),
            Err(error) => Message::SyncSignInFinished(Err(error.to_string())),
        },
    )
}

/// Allow-listed Google email from env, or [`DEFAULT_ALLOWED_GOOGLE_EMAIL`].
fn expected_google_email() -> String {
    std::env::var("PDF_FOLIO_ALLOWED_GOOGLE_EMAIL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ALLOWED_GOOGLE_EMAIL.to_owned())
}

/// Sync server base URL from env, or [`DEFAULT_SYNC_SERVER_BASE_URL`].
fn sync_server_base_url() -> String {
    std::env::var("PDF_FOLIO_SYNC_SERVER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYNC_SERVER_BASE_URL.to_owned())
}

/// OAuth client id from `PDF_FOLIO_GOOGLE_CLIENT_ID` or a `secrets/client_secret_*.json`.
fn load_google_client_id_from_secrets() -> Option<String> {
    std::env::var("PDF_FOLIO_GOOGLE_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let secrets_dir = Path::new("secrets");
            let path = std::fs::read_dir(secrets_dir)
                .ok()?
                .filter_map(std::result::Result::ok)
                .map(|entry| entry.path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("client_secret_") && name.ends_with(".json")
                        })
                })?;
            let json = std::fs::read_to_string(path).ok()?;
            let value = serde_json::from_str::<serde_json::Value>(&json).ok()?;
            value
                .get("installed")
                .and_then(|installed| installed.get("client_id"))
                .and_then(|client_id| client_id.as_str())
                .map(str::to_owned)
        })
}

/// True when `session` is still valid and its email matches the allow list.
fn session_matches_expected_email(session: &Session, expected_email: &str) -> bool {
    session.is_valid()
        && session
            .email
            .as_deref()
            .is_some_and(|email| email.eq_ignore_ascii_case(expected_email))
}
