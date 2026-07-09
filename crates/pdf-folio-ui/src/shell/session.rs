//! Last-session persistence for reopening PDF-Folio where it was left.

use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use iced::Task;
use pdf_folio_db::{EntryId, FolderId, LibrarySortMode};

use crate::*;

const SESSION_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct AppSession {
    version: u16,
    #[serde(default = "default_session_library_id")]
    pub(crate) active_library_id: String,
    mode: SessionMode,
    window: SessionWindow,
    appearance: SessionAppearance,
    pub(crate) viewer: SessionViewer,
    library: SessionLibrary,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionMode {
    Library,
    Viewer,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionWindow {
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionAppearance {
    theme: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SessionViewer {
    pub(crate) document_path: Option<PathBuf>,
    entry_id: Option<String>,
    page: u16,
    scroll_offset: f32,
    horizontal_offset: f32,
    scroll_mode: String,
    spread_mode: String,
    zoom_width: u16,
    toc_open: bool,
    sidebar_tab: String,
    expanded_outline_paths: Vec<Vec<usize>>,
    find_open: bool,
    find_query: String,
    find_highlight_all: bool,
    find_match_case: bool,
    find_match_diacritics: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionLibrary {
    compact_view_mode: bool,
    grid_zoom: f32,
    metadata_density: String,
    sort_mode: String,
    selected_folder: Option<String>,
    details_folder_id: Option<String>,
    search_query: String,
    scroll_offset: f32,
    tag_sidebar_width: f32,
    tag_sidebar_open: bool,
    sidebar_tab: String,
    tree_root_expanded: bool,
    collapsed_folder_ids: Vec<String>,
    folder_details_sidebar_open: bool,
    active_tag_filter: Option<String>,
    active_reading_filter: Option<String>,
    missing_filter_active: bool,
    selected_entry_ids: Vec<String>,
    selection_anchor: Option<String>,
    details_entry_id: Option<String>,
}

impl AppSession {
    pub(crate) fn window_size(&self) -> [f32; 2] {
        [self.window.width.max(1.0), self.window.height.max(1.0)]
    }
}

impl PDFolioApp {
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
                    return Task::done(Message::OpenLibraryEntry(entry_id));
                }
            }
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
            save_app_session_task(self),
        ])
    }

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

    fn document_matches_session(&self, session: &AppSession) -> bool {
        if let Some(session_entry_id) = session.viewer.entry_id.as_deref() {
            return self
                .viewer
                .current_entry_id
                .as_ref()
                .is_some_and(|entry_id| entry_id.as_str() == session_entry_id);
        }

        session.viewer.document_path.as_ref().is_some_and(|path| {
            self.viewer
                .current_document_path
                .as_ref()
                .is_some_and(|current| current == path)
        })
    }
}

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

fn session_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("dev", "pdf-folio", "PDF-Folio")
        .context("Could not find a data directory for PDF-Folio.")?;
    Ok(project_dirs.data_dir().join("session.json"))
}

fn folder_id_to_string(folder_id: &Option<FolderId>) -> Option<String> {
    folder_id
        .as_ref()
        .map(|folder_id| folder_id.as_str().to_owned())
}

fn valid_folder_id(id: Option<&str>, folders: &[Folder]) -> Option<FolderId> {
    let id = id?;
    folders
        .iter()
        .any(|folder| folder.id.as_str() == id)
        .then(|| FolderId::new(id))
}

fn valid_entry_id(id: &str, entries: &[LibraryEntry]) -> Option<EntryId> {
    entries
        .iter()
        .any(|entry| entry.id.as_str() == id)
        .then(|| EntryId::new(id))
}

fn theme_id(theme: AppTheme) -> &'static str {
    match theme {
        AppTheme::Light => "light",
        AppTheme::Dark => "dark",
    }
}

fn parse_theme(value: &str) -> AppTheme {
    match value {
        "light" => AppTheme::Light,
        _ => AppTheme::Dark,
    }
}

fn viewer_scroll_mode_id(mode: ViewerScrollMode) -> &'static str {
    match mode {
        ViewerScrollMode::Page => "page",
        ViewerScrollMode::Vertical => "vertical",
        ViewerScrollMode::Horizontal => "horizontal",
        ViewerScrollMode::Wrapped => "wrapped",
    }
}

fn parse_viewer_scroll_mode(value: &str) -> ViewerScrollMode {
    match value {
        "page" => ViewerScrollMode::Page,
        "horizontal" => ViewerScrollMode::Horizontal,
        "wrapped" => ViewerScrollMode::Wrapped,
        _ => ViewerScrollMode::Vertical,
    }
}

fn viewer_spread_mode_id(mode: ViewerSpreadMode) -> &'static str {
    match mode {
        ViewerSpreadMode::None => "none",
        ViewerSpreadMode::Odd => "odd",
        ViewerSpreadMode::Even => "even",
    }
}

fn parse_viewer_spread_mode(value: &str) -> ViewerSpreadMode {
    match value {
        "odd" => ViewerSpreadMode::Odd,
        "even" => ViewerSpreadMode::Even,
        _ => ViewerSpreadMode::None,
    }
}

fn library_sidebar_tab_id(tab: LibrarySidebarTab) -> &'static str {
    match tab {
        LibrarySidebarTab::Files => "files",
        LibrarySidebarTab::Tags => "tags",
    }
}

fn parse_library_sidebar_tab(value: &str) -> LibrarySidebarTab {
    match value {
        "tags" => LibrarySidebarTab::Tags,
        _ => LibrarySidebarTab::Files,
    }
}

fn viewer_sidebar_tab_id(tab: ViewerSidebarTab) -> &'static str {
    match tab {
        ViewerSidebarTab::Contents => "contents",
        ViewerSidebarTab::Thumbnails => "thumbnails",
    }
}

fn parse_viewer_sidebar_tab(value: &str) -> ViewerSidebarTab {
    match value {
        "thumbnails" => ViewerSidebarTab::Thumbnails,
        _ => ViewerSidebarTab::Contents,
    }
}

fn metadata_density_id(density: LibraryMetadataDensity) -> &'static str {
    match density {
        LibraryMetadataDensity::Minimal => "minimal",
        LibraryMetadataDensity::Standard => "standard",
        LibraryMetadataDensity::Detailed => "detailed",
    }
}

fn parse_metadata_density(value: &str) -> LibraryMetadataDensity {
    match value {
        "minimal" => LibraryMetadataDensity::Minimal,
        "detailed" => LibraryMetadataDensity::Detailed,
        _ => LibraryMetadataDensity::Standard,
    }
}

fn reading_filter_id(filter: LibraryReadingFilter) -> &'static str {
    match filter {
        LibraryReadingFilter::Unread => "unread",
        LibraryReadingFilter::Reading => "reading",
        LibraryReadingFilter::Finished => "finished",
    }
}

fn parse_reading_filter(value: &str) -> LibraryReadingFilter {
    match value {
        "unread" => LibraryReadingFilter::Unread,
        "finished" => LibraryReadingFilter::Finished,
        _ => LibraryReadingFilter::Reading,
    }
}

fn default_session_library_id() -> String {
    String::from("default")
}
