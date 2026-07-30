//! Viewer helper types and `PDFolioApp` methods for document open, find, and layout.
//!
//! Defines scroll/spread mode enums, rendered page wrappers, text selection and
//! find-in-document state, plus a large `impl PDFolioApp` block that opens
//! documents, manages text layers, computes page rectangles, and requests
//! visible tiles. Navigation-focused methods live in [`super::navigation`];
//! this module is the broader “viewer state machine” companion.
//!
//! # Key types
//!
//! - [`ViewerScrollMode`] / [`ViewerSpreadMode`] — paging arrangement.
//! - [`RenderedPageView`] — iced image handle for a raster tile.
//! - [`ViewerTextSelection`] / [`ViewerFindState`] — selection and search UI.
//!
//! # Related modules
//!
//! - [`super::document`] — field bag on `PDFolioApp::viewer`.
//! - [`super::tasks`] — async open/render constructors.
//! - [`super::update`] — message handlers that call these helpers.
//! - [`super::layout`] — pure geometry used by page-rect builders.

use iced::widget::image;
use pdf_folio_core::{PageTextLayer, RenderedPage};

/// Direction and paging model used to arrange PDF pages in the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerScrollMode {
    /// Advance one page/spread at a time.
    Page,
    /// Stack pages or spreads top-to-bottom.
    Vertical,
    /// Place pages or spreads left-to-right.
    Horizontal,
    /// Wrap pages or spreads into rows that fit the viewport width.
    Wrapped,
}

impl ViewerScrollMode {
    /// All user-facing scroll modes in menu order.
    pub const ALL: [Self; 4] = [Self::Page, Self::Vertical, Self::Horizontal, Self::Wrapped];

    /// User-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Page => "Page Scrolling",
            Self::Vertical => "Vertical Scrolling",
            Self::Horizontal => "Horizontal Scrolling",
            Self::Wrapped => "Wrapped Scrolling",
        }
    }

    /// Short help text for menus.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Page => "one page at a time",
            Self::Vertical => "continuous vertical",
            Self::Horizontal => "continuous horizontal",
            Self::Wrapped => "rows wrap to viewport",
        }
    }
}

/// Two-page spread pairing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerSpreadMode {
    /// Show one page per slot.
    None,
    /// Pair pages with odd-numbered pages on the left.
    Odd,
    /// Pair pages with even-numbered pages on the left, leaving the cover alone.
    Even,
}

impl ViewerSpreadMode {
    /// All user-facing spread modes in menu order.
    pub const ALL: [Self; 3] = [Self::None, Self::Odd, Self::Even];

    /// User-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No Spreads",
            Self::Odd => "Odd Spreads",
            Self::Even => "Even Spreads",
        }
    }
}

/// A rendered page prepared for display by iced.
#[derive(Debug, Clone)]
pub struct RenderedPageView {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

impl From<RenderedPage> for RenderedPageView {
    fn from(page: RenderedPage) -> Self {
        Self {
            width: page.width,
            height: page.height,
            handle: image::Handle::from_rgba(
                u32::from(page.width),
                u32::from(page.height),
                page.rgba,
            ),
        }
    }
}

/// A concrete character position in the viewer text layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewerTextAnchor {
    /// Zero-based page index.
    pub page: u16,
    /// Zero-based character index inside the page text layer.
    pub char_index: usize,
}

impl ViewerTextAnchor {
    /// Builds an anchor at zero-based `page` and `char_index`.
    pub fn new(page: u16, char_index: usize) -> Self {
        Self { page, char_index }
    }
}

/// Per-character text selection state for the raster viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerTextSelection {
    /// Character where the drag selection started.
    pub anchor: ViewerTextAnchor,
    /// Character currently under the selection drag.
    pub focus: ViewerTextAnchor,
    /// Whether the pointer is still dragging the selection.
    pub dragging: bool,
}

impl ViewerTextSelection {
    /// Starts a new selection anchored to one character.
    pub fn new(anchor: ViewerTextAnchor) -> Self {
        Self {
            anchor,
            focus: anchor,
            dragging: true,
        }
    }

    /// Returns the ordered selection endpoints.
    pub fn ordered(self) -> (ViewerTextAnchor, ViewerTextAnchor) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    /// Returns whether a page is inside the selection.
    pub fn contains_page(self, page: u16) -> bool {
        let (start, end) = self.ordered();
        (start.page..=end.page).contains(&page)
    }

    /// Returns the selected character range for a single page.
    pub fn char_range_for_page(
        self,
        page: u16,
        page_char_count: usize,
    ) -> Option<std::ops::RangeInclusive<usize>> {
        if page_char_count == 0 || !self.contains_page(page) {
            return None;
        }

        let (start, end) = self.ordered();
        let last = page_char_count - 1;
        let start_index = if page == start.page {
            start.char_index.min(last)
        } else {
            0
        };
        let end_index = if page == end.page {
            end.char_index.min(last)
        } else {
            last
        };

        (start_index <= end_index).then_some(start_index..=end_index)
    }
}

/// One text match in the open PDF viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewerFindMatch {
    /// Zero-based page index.
    pub page: u16,
    /// Inclusive zero-based start character index inside the page text layer.
    pub start: usize,
    /// Exclusive zero-based end character index inside the page text layer.
    pub end: usize,
}

impl ViewerFindMatch {
    /// Returns the inclusive range used by highlight drawing helpers.
    pub fn char_range(self) -> Option<std::ops::RangeInclusive<usize>> {
        (self.start < self.end).then_some(self.start..=self.end - 1)
    }
}

/// User-visible find-in-document state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerFindState {
    /// Whether the find bar is visible.
    pub open: bool,
    /// Search input contents.
    pub query: String,
    /// Whether all matches should be highlighted.
    pub highlight_all: bool,
    /// Whether matching should preserve case.
    pub match_case: bool,
    /// Whether matching should distinguish diacritic marks.
    pub match_diacritics: bool,
    /// All known matches in currently loaded text layers.
    pub matches: Vec<ViewerFindMatch>,
    /// Selected match index within `matches`.
    pub selected: Option<usize>,
}

impl Default for ViewerFindState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            highlight_all: true,
            match_case: false,
            match_diacritics: false,
            matches: Vec::new(),
            selected: None,
        }
    }
}

impl ViewerFindState {
    /// Recomputes matches from loaded text layers.
    pub fn refresh_matches<'a>(
        &mut self,
        layers: impl Iterator<Item = (&'a u16, &'a PageTextLayer)>,
    ) {
        let previous = self.selected_match();
        self.matches =
            viewer_find_matches(layers, &self.query, self.match_case, self.match_diacritics);
        self.selected = if self.matches.is_empty() {
            None
        } else if let Some(previous) = previous {
            self.matches
                .iter()
                .position(|candidate| *candidate >= previous)
                .or(Some(0))
        } else {
            Some(0)
        };
    }

    /// Returns the currently selected match.
    pub fn selected_match(&self) -> Option<ViewerFindMatch> {
        self.selected
            .and_then(|index| self.matches.get(index).copied())
    }

    /// Selects the next match, wrapping at the end.
    pub fn select_next(&mut self) {
        self.select_relative(1);
    }

    /// Selects the previous match, wrapping at the beginning.
    pub fn select_previous(&mut self) {
        self.select_relative(-1);
    }

    fn select_relative(&mut self, delta: isize) {
        if self.matches.is_empty() {
            self.selected = None;
            return;
        }

        let current = self.selected.unwrap_or(0);
        let len = self.matches.len();
        self.selected = Some(if delta < 0 {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        });
    }
}

/// Finds all non-overlapping query matches in loaded page text layers.
pub fn viewer_find_matches<'a>(
    layers: impl Iterator<Item = (&'a u16, &'a PageTextLayer)>,
    query: &str,
    match_case: bool,
    match_diacritics: bool,
) -> Vec<ViewerFindMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let needle = normalize_find_text(query, match_case, match_diacritics);
    if needle.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut layers = layers.collect::<Vec<_>>();
    layers.sort_by_key(|(page, _)| **page);

    for (page, layer) in layers {
        let mut haystack = String::new();
        let mut char_map = Vec::new();
        for (char_index, character) in layer.chars.iter().enumerate() {
            for normalized in character
                .text
                .chars()
                .flat_map(|character| normalize_find_char(character, match_case, match_diacritics))
            {
                haystack.push(normalized);
                char_map.push(char_index);
            }
        }

        let mut offset = 0;
        while let Some(relative) = haystack[offset..].find(&needle) {
            let start_byte = offset + relative;
            let end_byte = start_byte + needle.len();
            let start_normalized = haystack[..start_byte].chars().count();
            let end_normalized = haystack[..end_byte].chars().count();

            if let (Some(start), Some(end)) = (
                char_map.get(start_normalized).copied(),
                char_map.get(end_normalized.saturating_sub(1)).copied(),
            ) {
                matches.push(ViewerFindMatch {
                    page: *page,
                    start,
                    end: end.saturating_add(1),
                });
            }

            offset = end_byte;
        }
    }

    matches
}

fn normalize_find_text(text: &str, match_case: bool, match_diacritics: bool) -> String {
    text.chars()
        .flat_map(|character| normalize_find_char(character, match_case, match_diacritics))
        .collect()
}

fn normalize_find_char(character: char, match_case: bool, match_diacritics: bool) -> Vec<char> {
    let mut chars = if match_case {
        vec![character]
    } else {
        character.to_lowercase().collect()
    };

    if !match_diacritics {
        for character in &mut chars {
            *character = fold_latin_diacritic(*character);
        }
    }

    chars
}

fn fold_latin_diacritic(character: char) -> char {
    match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å'
        | 'ā' | 'ă' | 'ą' => 'a',
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ð' | 'Ď' | 'Đ' | 'ð' | 'ď' | 'đ' => 'd',
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ'
        | 'ė' | 'ę' | 'ě' => 'e',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => 'h',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī'
        | 'ĭ' | 'į' | 'ı' => 'i',
        'Ĵ' | 'ĵ' => 'j',
        'Ķ' | 'ķ' | 'ĸ' => 'k',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø'
        | 'ō' | 'ŏ' | 'ő' => 'o',
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => 'r',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ś' | 'ŝ' | 'ş' | 'š' | 'ſ' => 's',
        'Ţ' | 'Ť' | 'Ŧ' | 'ţ' | 'ť' | 'ŧ' => 't',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ'
        | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ŵ' | 'ŵ' => 'w',
        'Ý' | 'Ŷ' | 'Ÿ' | 'ý' | 'ÿ' | 'ŷ' => 'y',
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => 'z',
        _ => character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_folio_core::{PageTextChar, TextRect};

    #[test]
    fn viewer_find_matches_ignore_case_by_default() {
        let layer = text_layer(0, "Find find FIND");

        let matches = viewer_find_matches([(&0, &layer)].into_iter(), "find", false, false);

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[1].start, 5);
        assert_eq!(matches[2].start, 10);
    }

    #[test]
    fn viewer_find_matches_can_match_case() {
        let layer = text_layer(0, "Find find");

        let matches = viewer_find_matches([(&0, &layer)].into_iter(), "find", true, false);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 5);
    }

    #[test]
    fn viewer_find_matches_can_match_diacritics() {
        let layer = text_layer(0, "cafe café");

        let folded = viewer_find_matches([(&0, &layer)].into_iter(), "cafe", false, false);
        let strict = viewer_find_matches([(&0, &layer)].into_iter(), "cafe", false, true);

        assert_eq!(folded.len(), 2);
        assert_eq!(strict.len(), 1);
    }

    fn text_layer(page: u16, text: &str) -> PageTextLayer {
        PageTextLayer {
            page,
            width_points: 100.0,
            height_points: 100.0,
            chars: text
                .chars()
                .enumerate()
                .map(|(index, character)| PageTextChar {
                    index,
                    text: character.to_string(),
                    bounds: TextRect {
                        x: index as f32 * 0.01,
                        y: 0.1,
                        width: 0.01,
                        height: 0.05,
                    },
                })
                .collect(),
        }
    }
}

use crate::viewer::layout::*;
use crate::*;

impl PDFolioApp {
    /// Layout tokens from the loaded style book (toolbar heights, gutters, etc.).
    pub(crate) fn layout(&self) -> &crate::style::AppLayoutTokens {
        self.appearance.style_book.layout()
    }

    /// Estimates canvas width from window width minus open viewer sidebar.
    pub(crate) fn estimated_viewer_viewport_width(&self) -> f32 {
        let sidebar_width = if self.viewer.toc_open {
            self.layout().viewer_sidebar_width
        } else {
            0.0
        };
        (self.viewer.viewport_width - sidebar_width).max(1.0)
    }

    /// Estimates canvas height from window height minus toolbar.
    pub(crate) fn estimated_viewer_viewport_height(&self) -> f32 {
        (self.viewer.viewport_height - self.layout().toolbar_height).max(1.0)
    }

    /// Recomputes zoom width when the active preset depends on viewport size.
    pub(crate) fn apply_active_dimension_zoom(&mut self) -> Task<Message> {
        let Some(preset) = self.viewer.active_zoom_preset else {
            return Task::none();
        };
        if !preset.is_dimension_dependent() {
            return Task::none();
        }

        let width = preset.width_for(self);
        self.viewer.zoom_input = zoom_percent_label(width);
        let task = self.zoom_to_width(width, None, ZoomRenderPolicy::Immediate);
        if matches!(preset, ZoomPreset::PageWidth) {
            self.viewer.horizontal_offset = 0.0;
        }
        self.clamp_horizontal_offset();
        self.clamp_scroll_offset();
        task
    }

    /// Creates application state using the default database location.
    ///
    /// # Errors
    ///
    /// Returns an error when the library database cannot be opened.
    pub fn new() -> Result<Self> {
        Self::with_active_library_id(None)
    }

    fn with_active_library_id(active_library_id: Option<&str>) -> Result<Self> {
        let settings = Settings::default();
        let libraries = load_library_registry(active_library_id)?;
        let Some(active_profile) = libraries.active_profile() else {
            anyhow::bail!("No active library is available.");
        };
        let db = Arc::new(Db::open(active_profile.db_path.clone())?);
        let preferences = db.library_preferences().unwrap_or_default();
        let (style_book, style_load_error) = match StyleBook::load() {
            Ok(style_book) => (style_book, None),
            Err(error) => {
                tracing::warn!(%error, "Failed to load external styles; using bundled defaults");
                (StyleBook::bundled(), Some(error))
            }
        };
        let layout = style_book.layout();
        let sync_auth = SyncAuthRuntime::load();
        let auth_ready = sync_auth.is_signed_in();
        let library_entries = db.get_entries_sorted(preferences.sort_mode)?;
        let library_trash_entries = db.get_trashed_entries()?;
        let library_folders = db.get_folders()?;
        let library_trash_folders = db.get_trashed_folders()?;
        let library_status = Some(format!("{} PDFs in library", library_entries.len()));
        let mut app = Self {
            mode: if auth_ready {
                AppMode::Library
            } else {
                AppMode::SignedOut
            },
            viewer: ViewerRuntime {
                doc: None,
                current_entry_id: None,
                current_document_path: None,
                rendered_pages: HashMap::new(),
                page_aspect_ratios: Vec::new(),
                viewport_height: 900.0,
                viewport_width: 960.0,
                viewer_viewport_height: 900.0,
                viewer_viewport_width: 732.0,
                document_error: None,
                pending_document_open: false,
                document_open_started_at: None,
                dismissed_document_errors: HashSet::new(),
                cache: TileCache::with_default_capacity(),
                page_scroll_page: 0,
                scroll_offset: 0.0,
                horizontal_offset: 0.0,
                viewer_scroll_mode: ViewerScrollMode::Vertical,
                viewer_spread_mode: ViewerSpreadMode::None,
                zoom_width: settings.default_zoom_width,
                active_zoom_preset: None,
                zoom_editing: false,
                zoom_input: zoom_percent_label(settings.default_zoom_width),
                zoom_menu_open: false,
                zoom_preview_width_px: None,
                zoom_generation: 0,
                last_scroll_offset: 0.0,
                scale_factor: 1.0,
                modifiers: keyboard::Modifiers::default(),
                viewer_text_selection: None,
                viewer_text_layers: HashMap::new(),
                pending_text_layers: HashSet::new(),
                viewer_copy_pending: false,
                viewer_find: ViewerFindState::default(),
                pending_renders: HashMap::new(),
                page_fade_started: HashMap::new(),
                toc_open: true,
                viewer_sidebar_tab: ViewerSidebarTab::Contents,
                outline: Vec::new(),
                expanded_outline_paths: HashSet::new(),
                jump_dialog_open: false,
                page_input_editing: false,
                jump_input: String::new(),
            },
            library: LibraryRuntime {
                compact_view_mode: matches!(preferences.layout_mode, LibraryLayoutMode::List),
                library_grid_zoom: LibraryPreferences::default().grid_zoom.clamp(
                    layout.metric("LibraryInteraction", "grid_zoom_min", 0.25),
                    layout.metric("LibraryInteraction", "grid_zoom_max", 12.0),
                ),
                library_metadata_density: LibraryMetadataDensity::from_visible_fields(
                    &preferences.visible_metadata_fields,
                ),
                library_entries,
                library_trash_entries,
                library_folders,
                library_trash_folders,
                folder_smart_count_cache: HashMap::new(),
                trash_view_active: false,
                library_sort_mode: preferences.sort_mode,
                selected_folder: preferences.selected_folder,
                details_folder_id: None,
                new_folder_name: String::new(),
                create_folder_dialog_open: false,
                folder_rename_input: String::new(),
                search_query: String::new(),
                search_results: None,
                search_hit_pages: HashMap::new(),
                search_generation: 0,
                library_scroll_offset: 0.0,
                library_viewport_height: 720.0,
                library_viewport_x: 0.0,
                library_viewport_y: 0.0,
                library_viewport_width: 960.0,
                library_tag_sidebar_width: preferences.sidebar_width.clamp(
                    layout.library_sidebar_min_width,
                    layout.library_sidebar_max_width,
                ),
                library_tag_sidebar_open: true,
                resizing_library_tag_sidebar: false,
                library_inspector_width: layout.metric("LibraryInspector", "width", 320.0).clamp(
                    layout.metric("LibraryInspector", "min_width", 260.0),
                    layout.metric("LibraryInspector", "max_width", 480.0),
                ),
                library_inspector_open: true,
                resizing_library_inspector: false,
                library_sidebar_tab: LibrarySidebarTab::Files,
                library_tree_root_expanded: preferences.library_tree_root_expanded,
                library_tags_expanded: true,
                collapsed_library_tree_folders: preferences
                    .collapsed_folder_ids
                    .into_iter()
                    .collect::<HashSet<_>>(),
                folder_details_sidebar_open: false,
                thumbnails: HashMap::new(),
                pending_thumbnails: HashSet::new(),
                active_tag_filter: None,
                active_reading_filter: None,
                active_recently_opened_filter: false,
                missing_filter_active: false,
                previous_tag_pill_view: None,
                tag_entry_id: None,
                tag_input: String::new(),
                renaming_tag: None,
                tag_rename_input: String::new(),
                selected_library_entries: HashSet::new(),
                library_selection_anchor: None,
                bulk_tag_input: String::new(),
                inspector_tag_input: String::new(),
                inspector_tag_suggestions_open: false,
                inspector_tag_highlighted_index: 0,
                details_entry_id: None,
                details_title_input: String::new(),
                details_author_input: String::new(),
                library_status,
                library_error: None,
                library_startup_loading: false,
                library_history_restore_started_at: None,
                raindrop_connect_dialog_open: false,
                raindrop_callback_copied: false,
                raindrop_client_id_input: String::new(),
                raindrop_client_secret_input: String::new(),
                raindrop_import_dialog_open: false,
                raindrop_import_preview: None,
                raindrop_pdf_thumbnails: HashMap::new(),
                selected_raindrop_pdf_ids: HashSet::new(),
                raindrop_import_destination:
                    pdf_folio_cloud::raindrop::RaindropImportDestination::PreserveRaindropFolders,
                raindrop_import_location_menu_open: false,
                expanded_raindrop_import_location_folders: HashSet::new(),
                raindrop_import_new_folder_active: false,
                raindrop_import_new_folder_name: String::new(),
                raindrop_import_progress: None,
                import_menu_open: false,
                import_review: None,
                tag_manager_open: false,
                tag_manager_filter: String::new(),
                tag_manager_merge_destination: String::new(),
                export_dialog: None,
                export_progress: None,
                last_export_summary: None,
                raindrop_rollback_recovery_active: false,
                raindrop_rollback_recovery_status: None,
                dismissed_library_errors: HashSet::new(),
                bulk_operation_progress: None,
                folder_drop_flash: None,
                last_library_click: None,
                last_folder_click: None,
                last_tag_click: None,
                folder_drag_started_in_tree: false,
                parent_directory_drop_scroll_adjusted: false,
                library_card_hover_animations: HashMap::new(),
                animation_now: Instant::now(),
                library_drag: None,
                folder_drag: None,
                move_picker: None,
                clipboard: None,
                history: LibraryHistory::default(),
            },
            libraries,
            chrome: ChromeRuntime {
                pending_confirmation: None,
                folder_delete_warning_suppressed: false,
                folder_delete_skip_warning_checked: false,
                open_context_menu: None,
                command_palette_open: false,
                command_palette_query: String::new(),
                command_palette_selected_index: 0,
                cursor_position: Point::ORIGIN,
            },
            appearance: AppearanceRuntime {
                theme: AppTheme::Dark,
                style_book,
                style_load_error,
            },
            settings,
            sync_auth,
            db,
            sync_in_progress: None,
            sync_queued_libraries: HashSet::new(),
            last_sync_started_at: None,
            last_sync_completed_at: None,
            startup_background_ready: false,
            pending_session_restore: None,
        };
        app.rebuild_folder_smart_count_cache();
        app.set_active_library_preview_from_entries();
        Ok(app)
    }

    /// Creates application state and records the startup PDF path when available.
    pub fn with_initial_file(initial_file: Option<PathBuf>) -> Result<Self> {
        Self::with_initial_file_and_session(initial_file, None)
    }

    /// Builds app state, optionally restoring a session and queuing a CLI file open.
    ///
    /// When signed in with an `initial_file`, switches to viewer mode and marks
    /// a pending document open for the launch task chain.
    pub(crate) fn with_initial_file_and_session(
        initial_file: Option<PathBuf>,
        session: Option<AppSession>,
    ) -> Result<Self> {
        let mut app = Self::with_active_library_id(
            session
                .as_ref()
                .map(|session| session.active_library_id.as_str()),
        )?;
        app.pending_session_restore = session;
        if let Some(session) = app.pending_session_restore.as_ref() {
            let [width, height] = session.window_size();
            app.viewer.viewport_width = width;
            app.viewer.viewport_height = height;
            app.viewer.viewer_viewport_width = app.estimated_viewer_viewport_width();
            app.viewer.viewer_viewport_height = app.estimated_viewer_viewport_height();
        }
        if let Some(session) = app.pending_session_restore.clone() {
            app.apply_library_session(&session);
            app.library.library_entries =
                app.db.get_entries_sorted(app.library.library_sort_mode)?;
            app.library.library_trash_entries = app.db.get_trashed_entries()?;
            app.rebuild_folder_smart_count_cache();
            app.library.thumbnails.clear();
            app.library.pending_thumbnails.clear();
            app.set_active_library_preview_from_entries();
        }
        let Some(path) = initial_file else {
            return Ok(app);
        };

        if app.sync_auth.is_signed_in() {
            app.mode = AppMode::Viewer;
            app.pending_session_restore = None;
            app.viewer.document_error = Some(format!("Opening {}...", path.display()));
            app.viewer.pending_document_open = true;
            app.viewer.document_open_started_at = Some(Instant::now());
        }

        Ok(app)
    }

    #[cfg(test)]
    /// Test helper: opens `doc` without an associated filesystem path.
    pub(crate) fn open_document(&mut self, doc: Arc<PdfDoc>) -> Task<Message> {
        self.open_document_with_path(doc, None)
    }

    /// Installs `doc` as the open document, resets viewer runtime, and requests tiles.
    ///
    /// Switches to viewer mode, rebuilds aspect ratios and outline, clears text
    /// layers/find/selection, applies automatic zoom, and returns tasks to
    /// render the first pages plus any pending session restore.
    pub(crate) fn open_document_with_path(
        &mut self,
        doc: Arc<PdfDoc>,
        path: Option<PathBuf>,
    ) -> Task<Message> {
        self.mode = AppMode::Viewer;
        self.clear_library_transient_interactions();
        self.viewer.doc = Some(Arc::clone(&doc));
        self.viewer.current_document_path = path;
        self.viewer.current_entry_id = None;
        self.viewer.cache.clear();
        self.viewer.rendered_pages.clear();
        self.viewer.page_aspect_ratios = (0..doc.page_count())
            .map(|page| doc.page_aspect_ratio(page).unwrap_or(11.0 / 8.5))
            .collect();
        self.viewer.outline = doc.outline().unwrap_or_default();
        self.viewer.viewer_sidebar_tab = ViewerSidebarTab::Contents;
        self.viewer.expanded_outline_paths.clear();
        self.viewer.pending_renders.clear();
        self.viewer.page_fade_started.clear();
        self.viewer.page_scroll_page = 0;
        self.viewer.scroll_offset = 0.0;
        self.viewer.last_scroll_offset = 0.0;
        self.viewer.horizontal_offset = 0.0;
        self.viewer.viewer_viewport_width = self.estimated_viewer_viewport_width();
        self.viewer.viewer_viewport_height = self.estimated_viewer_viewport_height();
        self.viewer.active_zoom_preset = Some(ZoomPreset::Automatic);
        self.viewer.zoom_width = ZoomPreset::Automatic.width_for(self);
        self.viewer.zoom_editing = false;
        self.viewer.zoom_input = zoom_percent_label(self.viewer.zoom_width);
        self.viewer.zoom_menu_open = false;
        self.viewer.zoom_preview_width_px = None;
        self.viewer.zoom_generation = self.viewer.zoom_generation.wrapping_add(1);
        self.viewer.viewer_text_selection = None;
        self.viewer.viewer_text_layers.clear();
        self.viewer.pending_text_layers.clear();
        self.viewer.viewer_copy_pending = false;
        self.viewer.viewer_find = ViewerFindState::default();
        self.viewer.pending_document_open = false;
        self.viewer.document_open_started_at = None;
        self.viewer.document_error = None;
        self.viewer.jump_dialog_open = false;
        self.viewer.page_input_editing = false;
        self.viewer.jump_input.clear();

        Task::batch([
            self.request_visible_pages(),
            self.apply_pending_session_to_open_document(),
        ])
    }

    /// Leaves viewer mode for the library, refreshing entries/folders/thumbnails.
    pub(crate) fn return_to_library(&mut self) -> Task<Message> {
        self.mode = AppMode::Library;
        self.viewer.document_error = None;
        self.viewer.jump_dialog_open = false;
        self.viewer.page_input_editing = false;
        self.viewer.jump_input.clear();
        Task::batch([
            self.refresh_library(),
            self.refresh_folders(),
            self.request_visible_thumbnails(),
        ])
    }

    /// Returns to an already-open document without reloading it.
    pub(crate) fn return_to_viewer(&mut self) -> Task<Message> {
        if self.viewer.doc.is_none() {
            return Task::none();
        }

        self.mode = AppMode::Viewer;
        self.clear_library_transient_interactions();
        self.request_visible_pages()
    }

    /// Opens a library entry's PDF and restores its last reading page.
    ///
    /// Sets `current_entry_id` for progress tracking, then batches open setup,
    /// session restore, tile requests, and scroll sync.
    pub(crate) fn open_library_document(
        &mut self,
        entry_id: EntryId,
        doc: Arc<PdfDoc>,
    ) -> Task<Message> {
        self.viewer.current_entry_id = Some(entry_id.clone());
        self.viewer.current_document_path = self
            .library
            .library_entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .map(|entry| entry.path.clone());
        let last_page = self
            .library
            .library_entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .map_or(0, |entry| entry.last_page);
        let task = self.open_document_with_path(doc, self.viewer.current_document_path.clone());
        self.viewer.current_entry_id = Some(entry_id);
        self.viewer.last_scroll_offset = self.viewer.scroll_offset;
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            self.viewer.page_scroll_page = last_page;
            self.viewer.scroll_offset = 0.0;
            self.viewer.horizontal_offset = 0.0;
        } else {
            self.viewer.scroll_offset = self.page_top(last_page);
        }
        self.clamp_scroll_offset();
        Task::batch([
            task,
            self.apply_pending_session_to_open_document(),
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Schedules renders (or cache hits) for prefetched visible pages at current zoom.
    ///
    /// Emits [`Message::PageRendered`] per new tile and also requests text layers
    /// for selection/find.
    pub(crate) fn request_visible_pages(&mut self) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        let generation = self.viewer.zoom_generation;
        for page in self.prefetch_page_order() {
            let key = TileKey {
                page,
                width_px: self.render_width_px(),
            };

            if self.viewer.rendered_pages.contains_key(&key)
                || self.viewer.pending_renders.get(&key) == Some(&Some(generation))
            {
                continue;
            }

            if let Some(data) = self.viewer.cache.get(&key) {
                let width = key.width_px;
                let height = self.render_height_px(page);
                let expected_len = usize::from(width) * usize::from(height) * 4;

                if data.len() == expected_len {
                    let handle = image::Handle::from_rgba(
                        u32::from(width),
                        u32::from(height),
                        data.as_ref().clone(),
                    );
                    self.viewer.rendered_pages.insert(
                        key,
                        RenderedPageView {
                            width,
                            height,
                            handle,
                        },
                    );
                    continue;
                }
            }

            self.viewer.pending_renders.insert(key, Some(generation));
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                render_page(doc, key),
                move |result| match result {
                    Ok((key, page)) => Message::PageRendered {
                        key,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                        generation: Some(generation),
                    },
                    Err(error) => Message::DocumentError(error.to_string()),
                },
            ));
        }

        Task::batch([Task::batch(tasks), self.request_visible_text_layers()])
    }

    /// Renders sidebar thumbnail-width tiles when the thumbnails tab is active.
    pub(crate) fn request_viewer_thumbnail_pages(&mut self) -> Task<Message> {
        if self.viewer.viewer_sidebar_tab != ViewerSidebarTab::Thumbnails {
            return Task::none();
        }

        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };

        let mut tasks = Vec::new();
        for page in 0..doc.page_count() {
            let key = TileKey {
                page,
                width_px: self.layout().viewer_thumbnail_width_px,
            };

            if self.viewer.rendered_pages.contains_key(&key)
                || self.viewer.pending_renders.contains_key(&key)
            {
                continue;
            }

            if let Some(data) = self.viewer.cache.get(&key) {
                let height = (f32::from(key.width_px)
                    * self.viewer.page_aspect_ratios[usize::from(page)])
                .round()
                .clamp(1.0, f32::from(u16::MAX)) as u16;
                let expected_len = usize::from(key.width_px) * usize::from(height) * 4;

                if data.len() == expected_len {
                    let handle = image::Handle::from_rgba(
                        u32::from(key.width_px),
                        u32::from(height),
                        data.as_ref().clone(),
                    );
                    self.viewer.rendered_pages.insert(
                        key,
                        RenderedPageView {
                            width: key.width_px,
                            height,
                            handle,
                        },
                    );
                    continue;
                }
            }

            self.viewer.pending_renders.insert(key, None);
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                render_page(doc, key),
                |result| match result {
                    Ok((key, page)) => Message::PageRendered {
                        key,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                        generation: None,
                    },
                    Err(error) => Message::DocumentError(error.to_string()),
                },
            ));
        }

        Task::batch(tasks)
    }

    /// Extracts text layers for pages currently in the viewport.
    pub(crate) fn request_visible_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let pages = self.visible_page_range();

        self.request_text_layers(pages, doc)
    }

    /// Extracts text layers for every page (used when find-in-document opens).
    pub(crate) fn request_all_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let page_count = doc.page_count();

        self.request_text_layers(0..page_count, doc)
    }

    /// Spawns text-layer extraction tasks for pages not already loaded or pending.
    ///
    /// Completions become [`Message::ViewerTextLayerLoaded`] or
    /// [`Message::ViewerTextLayerError`].
    pub(crate) fn request_text_layers(
        &mut self,
        pages: std::ops::Range<u16>,
        doc: Arc<PdfDoc>,
    ) -> Task<Message> {
        let mut tasks = Vec::new();
        for page in pages {
            if self.viewer.viewer_text_layers.contains_key(&page)
                || self.viewer.pending_text_layers.contains(&page)
            {
                continue;
            }

            self.viewer.pending_text_layers.insert(page);
            let doc = Arc::clone(&doc);
            tasks.push(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || doc.text_layer(page))
                        .await
                        .map_err(anyhow::Error::from)?
                },
                move |result| match result {
                    Ok(layer) => Message::ViewerTextLayerLoaded {
                        page,
                        layer: Arc::new(layer),
                    },
                    Err(error) => Message::ViewerTextLayerError {
                        page,
                        error: error.to_string(),
                    },
                },
            ));
        }

        Task::batch(tasks)
    }

    /// Recomputes find matches from currently loaded text layers and the query.
    pub(crate) fn refresh_viewer_find_matches(&mut self) {
        self.viewer.viewer_find.refresh_matches(
            self.viewer
                .viewer_text_layers
                .iter()
                .map(|(page, layer)| (page, layer.as_ref())),
        );
    }

    /// Shows the find bar, loads all text layers, and focuses the query field.
    pub(crate) fn open_viewer_find(&mut self) -> Task<Message> {
        if self.mode != AppMode::Viewer || self.viewer.doc.is_none() {
            return Task::none();
        }

        self.viewer.viewer_find.open = true;
        self.viewer.zoom_menu_open = false;
        self.refresh_viewer_find_matches();

        Task::batch([
            self.request_all_text_layers(),
            operation::focus(Id::new(VIEWER_FIND_INPUT_ID)),
        ])
    }

    /// Sets viewer find query.
    pub(crate) fn set_viewer_find_query(&mut self, query: String) -> Task<Message> {
        self.viewer.viewer_find.query = query;
        self.refresh_viewer_find_matches();
        Task::batch([
            self.request_all_text_layers(),
            self.scroll_to_selected_viewer_find_match(),
        ])
    }

    /// Scrolls to the currently selected find match, if any.
    pub(crate) fn scroll_to_selected_viewer_find_match(&mut self) -> Task<Message> {
        let Some(selected) = self.viewer.viewer_find.selected_match() else {
            return Task::none();
        };

        self.scroll_to_viewer_find_match(selected)
    }

    /// Scrolls so `selected` is visible and refreshes tiles at the new offset.
    pub(crate) fn scroll_to_viewer_find_match(
        &mut self,
        selected: ViewerFindMatch,
    ) -> Task<Message> {
        let Some(layer) = self.viewer.viewer_text_layers.get(&selected.page) else {
            return Task::none();
        };
        let Some(character) = layer.chars.get(selected.start) else {
            return Task::none();
        };

        self.scroll_to_page_rect(selected.page, character.bounds.x, character.bounds.y);
        self.clamp_scroll_offset();
        self.clamp_horizontal_offset();
        Task::batch([
            self.request_visible_pages(),
            self.scroll_viewer_to_offsets_task(),
        ])
    }

    /// Begins a drag text selection at the given character anchor.
    pub(crate) fn start_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        self.viewer.viewer_text_selection = Some(ViewerTextSelection::new(ViewerTextAnchor::new(
            page, char_index,
        )));
        self.viewer.viewer_copy_pending = false;
    }

    /// Extends the active selection focus to another character while dragging.
    pub(crate) fn update_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        let Some(selection) = &mut self.viewer.viewer_text_selection else {
            return;
        };

        selection.focus = ViewerTextAnchor::new(page, char_index);
        self.viewer.viewer_copy_pending = false;
    }

    /// Ends the pointer drag without clearing the selection range.
    pub(crate) fn finish_viewer_text_selection(&mut self) {
        if let Some(selection) = &mut self.viewer.viewer_text_selection {
            selection.dragging = false;
        }
    }

    /// Clears selection state and any pending copy wait.
    pub(crate) fn clear_viewer_text_selection(&mut self) {
        self.viewer.viewer_text_selection = None;
        self.viewer.viewer_copy_pending = false;
    }

    /// Whether every page spanned by the selection has a loaded text layer.
    pub(crate) fn selected_text_layers_ready(&self) -> bool {
        let Some(selection) = self.viewer.viewer_text_selection else {
            return false;
        };

        let (start, end) = selection.ordered();
        (start.page..=end.page).all(|page| self.viewer.viewer_text_layers.contains_key(&page))
    }

    /// Concatenates selected characters across pages, or `None` if empty/unavailable.
    pub(crate) fn selected_viewer_text(&self) -> Option<String> {
        let selection = self.viewer.viewer_text_selection?;
        let (start, end) = selection.ordered();
        let mut text = String::new();

        for page in start.page..=end.page {
            let layer = self.viewer.viewer_text_layers.get(&page)?;
            let Some(range) = selection.char_range_for_page(page, layer.chars.len()) else {
                continue;
            };

            if !text.is_empty() {
                text.push('\n');
            }
            for index in range {
                if let Some(character) = layer.chars.get(index) {
                    text.push_str(&character.text);
                }
            }
        }

        (!text.is_empty()).then_some(text)
    }

    /// Copies selected PDF text to the OS clipboard, waiting for layers if needed.
    pub(crate) fn copy_selected_viewer_text(&mut self) -> Task<Message> {
        if self.viewer.viewer_text_selection.is_none() {
            return Task::none();
        }

        if self.selected_text_layers_ready() {
            self.viewer.viewer_copy_pending = false;
            self.selected_viewer_text()
                .map_or_else(Task::none, clipboard::write)
        } else {
            self.viewer.viewer_copy_pending = true;
            self.request_visible_text_layers()
        }
    }

    /// Zero-based half-open range of pages intersecting the current viewport.
    pub(crate) fn visible_page_range(&self) -> std::ops::Range<u16> {
        let Some(doc) = &self.viewer.doc else {
            return 0..0;
        };

        let page_count = doc.page_count();
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            let page = self
                .viewer
                .page_scroll_page
                .min(page_count.saturating_sub(1));
            return page..page.saturating_add(1).min(page_count);
        }

        let viewport = Rectangle {
            x: self.viewer.horizontal_offset.max(0.0),
            y: self.viewer.scroll_offset.max(0.0),
            width: self.viewer.viewer_viewport_width.max(1.0),
            height: self.viewer.viewer_viewport_height.max(1.0),
        };
        let mut first = None;
        let mut end = 0;

        for (page, rect) in self.viewer_page_rects_content(self.viewer.viewer_viewport_width) {
            if rects_intersect(rect, viewport) {
                first.get_or_insert(page);
                end = page.saturating_add(1);
            }
        }

        first.unwrap_or(0)..end.max(first.unwrap_or(0).saturating_add(1).min(page_count))
    }

    /// Priority-ordered pages to render (visible first, then scroll-direction neighbors).
    pub(crate) fn prefetch_page_order(&self) -> Vec<u16> {
        let Some(doc) = &self.viewer.doc else {
            return Vec::new();
        };
        let page_count = doc.page_count();
        if page_count == 0 {
            return Vec::new();
        }

        prefetch_page_order_for_range(
            self.visible_page_range(),
            page_count,
            self.viewer.scroll_offset >= self.viewer.last_scroll_offset,
        )
    }

    /// Layout height of `page` at the current zoom width and aspect ratio.
    pub(crate) fn page_height(&self, page: u16) -> f32 {
        let ratio = self
            .viewer
            .page_aspect_ratios
            .get(usize::from(page))
            .copied()
            .unwrap_or(11.0 / 8.5)
            .max(0.01);
        f32::from(self.viewer.zoom_width) / ratio
    }

    /// Device-pixel render width for tiles at the current zoom and scale factor.
    pub(crate) fn render_width_px(&self) -> u16 {
        (f32::from(self.viewer.zoom_width) * self.viewer.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    /// Device-pixel render height for `page` at the current zoom and scale factor.
    pub(crate) fn render_height_px(&self, page: u16) -> u16 {
        (self.page_height(page) * self.viewer.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    /// Full document content height in logical pixels (scroll extent).
    pub(crate) fn content_height(&self) -> f32 {
        self.viewer_content_size(self.viewer.viewer_viewport_width)
            .height
    }

    /// Full document content width in logical pixels (scroll extent).
    pub(crate) fn content_width(&self) -> f32 {
        self.viewer_content_size(self.viewer.viewer_viewport_width)
            .width
    }

    /// Page rectangles that currently intersect the viewport.
    pub(crate) fn viewer_page_rects_visible_content(&self) -> Vec<(u16, Rectangle)> {
        let viewport = Rectangle {
            x: self.viewer.horizontal_offset.max(0.0),
            y: self.viewer.scroll_offset.max(0.0),
            width: self.viewer.viewer_viewport_width.max(1.0),
            height: self.viewer.viewer_viewport_height.max(1.0),
        };

        self.viewer_page_rects_content(self.viewer.viewer_viewport_width)
            .into_iter()
            .filter(|(_, rect)| rects_intersect(*rect, viewport))
            .collect()
    }

    /// Content-space rectangle for a single page, if present in the layout.
    pub(crate) fn viewer_page_rect_for_page(&self, target_page: u16) -> Option<Rectangle> {
        self.viewer_page_rects_content(self.viewer.viewer_viewport_width)
            .into_iter()
            .find_map(|(page, rect)| (page == target_page).then_some(rect))
    }

    /// All page rectangles in document content coordinates for the scroll mode.
    pub(crate) fn viewer_page_rects_content(&self, viewport_width: f32) -> Vec<(u16, Rectangle)> {
        let Some(doc) = &self.viewer.doc else {
            return Vec::new();
        };

        match self.viewer.viewer_scroll_mode {
            ViewerScrollMode::Page => self.page_mode_rects(doc.page_count()),
            ViewerScrollMode::Horizontal => {
                let groups = viewer_spread_groups(doc.page_count(), self.viewer.viewer_spread_mode);
                self.horizontal_page_rects(&groups)
            }
            ViewerScrollMode::Wrapped => {
                let groups = viewer_spread_groups(doc.page_count(), self.viewer.viewer_spread_mode);
                self.wrapped_page_rects(&groups, viewport_width)
            }
            ViewerScrollMode::Vertical => {
                let groups = viewer_spread_groups(doc.page_count(), self.viewer.viewer_spread_mode);
                self.vertical_page_rects(&groups)
            }
        }
    }

    /// Single-page layout: only the current page-scroll page is positioned.
    pub(crate) fn page_mode_rects(&self, page_count: u16) -> Vec<(u16, Rectangle)> {
        if page_count == 0 {
            return Vec::new();
        }

        let page = self
            .viewer
            .page_scroll_page
            .min(page_count.saturating_sub(1));
        let height = self.page_height(page);
        let content_width = (f32::from(self.viewer.zoom_width) + Spacing::PAGE_GUTTER * 2.0)
            .max(self.viewer.viewer_viewport_width)
            .max(1.0);
        let x =
            ((content_width - f32::from(self.viewer.zoom_width)) / 2.0).max(Spacing::PAGE_GUTTER);

        vec![(
            page,
            Rectangle::new(
                Point::new(x, Spacing::PAGE_GUTTER),
                Size::new(f32::from(self.viewer.zoom_width), height),
            ),
        )]
    }

    /// Stacks spread groups top-to-bottom for vertical continuous scrolling.
    pub(crate) fn vertical_page_rects(&self, groups: &[Vec<u16>]) -> Vec<(u16, Rectangle)> {
        let content_width = viewer_groups_max_width(self, groups)
            .max(self.viewer.viewer_viewport_width)
            .max(1.0);
        let mut rects = Vec::new();
        let mut y = Spacing::PAGE_GUTTER;

        for group in groups {
            let group_width = viewer_group_width(self, group);
            let group_height = viewer_group_height(self, group);
            let mut x = ((content_width - group_width) / 2.0).max(Spacing::PAGE_GUTTER);

            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(x, y + (group_height - height) / 2.0),
                        Size::new(f32::from(self.viewer.zoom_width), height),
                    ),
                ));
                x += f32::from(self.viewer.zoom_width) + Spacing::PAGE_GAP;
            }

            y += group_height + Spacing::PAGE_GAP;
        }

        rects
    }

    /// Places spread groups left-to-right for horizontal continuous scrolling.
    pub(crate) fn horizontal_page_rects(&self, groups: &[Vec<u16>]) -> Vec<(u16, Rectangle)> {
        let content_size =
            self.viewer_content_size_for_groups(groups, self.viewer.viewer_viewport_width);
        let total_width = viewer_groups_inline_width(self, groups);
        let mut rects = Vec::new();
        let mut x = ((content_size.width - total_width) / 2.0).max(Spacing::PAGE_GUTTER);

        for group in groups {
            let group_height = viewer_group_height(self, group);
            let mut page_x = x;
            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(page_x, (content_size.height - height) / 2.0),
                        Size::new(f32::from(self.viewer.zoom_width), height),
                    ),
                ));
                page_x += f32::from(self.viewer.zoom_width) + Spacing::PAGE_GAP;
            }
            x += viewer_group_width(self, group).max(group_height * 0.0) + Spacing::PAGE_GAP;
        }

        rects
    }

    /// Wraps spread groups into rows that fit `viewport_width`.
    pub(crate) fn wrapped_page_rects(
        &self,
        groups: &[Vec<u16>],
        viewport_width: f32,
    ) -> Vec<(u16, Rectangle)> {
        let max_row_width = (viewport_width - Spacing::PAGE_GUTTER * 2.0)
            .max(viewer_groups_max_width(self, groups))
            .max(f32::from(self.viewer.zoom_width));
        let content_width = (max_row_width + Spacing::PAGE_GUTTER * 2.0)
            .max(self.viewer.viewer_viewport_width)
            .max(1.0);
        let mut rects = Vec::new();
        let mut x = Spacing::PAGE_GUTTER;
        let mut y = Spacing::PAGE_GUTTER;
        let mut row_height: f32 = 0.0;

        for group in groups {
            let group_width = viewer_group_width(self, group);
            let group_height = viewer_group_height(self, group);
            if x > Spacing::PAGE_GUTTER && x + group_width > Spacing::PAGE_GUTTER + max_row_width {
                y += row_height + Spacing::PAGE_GAP;
                x = Spacing::PAGE_GUTTER;
                row_height = 0.0;
            }

            let mut page_x = x;
            for &page in group {
                let height = self.page_height(page);
                rects.push((
                    page,
                    Rectangle::new(
                        Point::new(page_x, y + (group_height - height) / 2.0),
                        Size::new(f32::from(self.viewer.zoom_width), height),
                    ),
                ));
                page_x += f32::from(self.viewer.zoom_width) + Spacing::PAGE_GAP;
            }

            x += group_width + Spacing::PAGE_GAP;
            row_height = row_height.max(group_height);
        }

        let horizontal_padding = if content_width > max_row_width + Spacing::PAGE_GUTTER * 2.0 {
            (content_width - (max_row_width + Spacing::PAGE_GUTTER * 2.0)) / 2.0
        } else {
            0.0
        };

        if horizontal_padding > 0.0 {
            for (_, rect) in &mut rects {
                rect.x += horizontal_padding;
            }
        }

        rects
    }

    /// Total scrollable content size for the open document at `viewport_width`.
    pub(crate) fn viewer_content_size(&self, viewport_width: f32) -> Size {
        let Some(doc) = &self.viewer.doc else {
            return Size::new(
                viewport_width.max(1.0),
                self.viewer.viewer_viewport_height.max(1.0),
            );
        };
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            let page = self
                .viewer
                .page_scroll_page
                .min(doc.page_count().saturating_sub(1));
            return Size::new(
                (f32::from(self.viewer.zoom_width) + Spacing::PAGE_GUTTER * 2.0)
                    .max(viewport_width)
                    .max(1.0),
                (self.page_height(page) + Spacing::PAGE_GUTTER * 2.0)
                    .max(self.viewer.viewer_viewport_height)
                    .max(1.0),
            );
        }

        let groups = viewer_spread_groups(doc.page_count(), self.viewer.viewer_spread_mode);
        self.viewer_content_size_for_groups(&groups, viewport_width)
    }

    /// Content size for precomputed spread `groups` under the active scroll mode.
    pub(crate) fn viewer_content_size_for_groups(
        &self,
        groups: &[Vec<u16>],
        viewport_width: f32,
    ) -> Size {
        match self.viewer.viewer_scroll_mode {
            ViewerScrollMode::Horizontal => Size::new(
                viewer_groups_inline_width(self, groups)
                    .max(viewport_width)
                    .max(1.0),
                (viewer_groups_max_height(self, groups) + Spacing::PAGE_GUTTER * 2.0)
                    .max(self.viewer.viewer_viewport_height)
                    .max(1.0),
            ),
            ViewerScrollMode::Wrapped => {
                let rects = self.wrapped_page_rects(groups, viewport_width);
                let height = rects
                    .iter()
                    .map(|(_, rect)| rect.y + rect.height)
                    .fold(0.0, f32::max)
                    + Spacing::PAGE_GUTTER;
                Size::new(
                    viewport_width
                        .max(viewer_groups_max_width(self, groups))
                        .max(1.0),
                    height.max(self.viewer.viewer_viewport_height).max(1.0),
                )
            }
            ViewerScrollMode::Page | ViewerScrollMode::Vertical => {
                let height: f32 = groups
                    .iter()
                    .map(|group| viewer_group_height(self, group) + Spacing::PAGE_GAP)
                    .sum();
                Size::new(
                    viewer_groups_max_width(self, groups)
                        .max(viewport_width)
                        .max(1.0),
                    (height + Spacing::PAGE_GUTTER * 2.0)
                        .max(self.viewer.viewer_viewport_height)
                        .max(1.0),
                )
            }
        }
    }

    /// Best-effort current page index for toolbar display and progress writes.
    pub(crate) fn current_page(&self) -> u16 {
        if self.viewer.viewer_scroll_mode == ViewerScrollMode::Page {
            return self.viewer.doc.as_ref().map_or(0, |doc| {
                self.viewer
                    .page_scroll_page
                    .min(doc.page_count().saturating_sub(1))
            });
        }

        self.visible_page_range().start
    }
}
