use super::*;
use crate::viewer::layout::*;

impl PDFolioApp {
    pub(crate) fn layout(&self) -> &crate::style::AppLayoutTokens {
        self.appearance.style_book.layout()
    }

    pub(crate) fn estimated_viewer_viewport_width(&self) -> f32 {
        let sidebar_width = if self.viewer.toc_open {
            self.layout().viewer_sidebar_width
        } else {
            0.0
        };
        (self.viewer.viewport_width - sidebar_width).max(1.0)
    }

    pub(crate) fn estimated_viewer_viewport_height(&self) -> f32 {
        (self.viewer.viewport_height - self.layout().toolbar_height).max(1.0)
    }

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
                annotations: Vec::new(),
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
    pub(crate) fn open_document(&mut self, doc: Arc<PdfDoc>) -> Task<Message> {
        self.open_document_with_path(doc, None)
    }

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

    pub(crate) fn return_to_viewer(&mut self) -> Task<Message> {
        if self.viewer.doc.is_none() {
            return Task::none();
        }

        self.mode = AppMode::Viewer;
        self.clear_library_transient_interactions();
        self.request_visible_pages()
    }

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

    pub(crate) fn request_visible_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let pages = self.visible_page_range();

        self.request_text_layers(pages, doc)
    }

    pub(crate) fn request_all_text_layers(&mut self) -> Task<Message> {
        let Some(doc) = &self.viewer.doc else {
            return Task::none();
        };
        let doc = Arc::clone(doc);
        let page_count = doc.page_count();

        self.request_text_layers(0..page_count, doc)
    }

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

    pub(crate) fn refresh_viewer_find_matches(&mut self) {
        self.viewer.viewer_find.refresh_matches(
            self.viewer
                .viewer_text_layers
                .iter()
                .map(|(page, layer)| (page, layer.as_ref())),
        );
    }

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

    pub(crate) fn set_viewer_find_query(&mut self, query: String) -> Task<Message> {
        self.viewer.viewer_find.query = query;
        self.refresh_viewer_find_matches();
        Task::batch([
            self.request_all_text_layers(),
            self.scroll_to_selected_viewer_find_match(),
        ])
    }

    pub(crate) fn scroll_to_selected_viewer_find_match(&mut self) -> Task<Message> {
        let Some(selected) = self.viewer.viewer_find.selected_match() else {
            return Task::none();
        };

        self.scroll_to_viewer_find_match(selected)
    }

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

    pub(crate) fn start_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        self.viewer.viewer_text_selection = Some(ViewerTextSelection::new(ViewerTextAnchor::new(
            page, char_index,
        )));
        self.viewer.viewer_copy_pending = false;
    }

    pub(crate) fn update_viewer_text_selection(&mut self, page: u16, char_index: usize) {
        let Some(selection) = &mut self.viewer.viewer_text_selection else {
            return;
        };

        selection.focus = ViewerTextAnchor::new(page, char_index);
        self.viewer.viewer_copy_pending = false;
    }

    pub(crate) fn finish_viewer_text_selection(&mut self) {
        if let Some(selection) = &mut self.viewer.viewer_text_selection {
            selection.dragging = false;
        }
    }

    pub(crate) fn clear_viewer_text_selection(&mut self) {
        self.viewer.viewer_text_selection = None;
        self.viewer.viewer_copy_pending = false;
    }

    pub(crate) fn selected_text_layers_ready(&self) -> bool {
        let Some(selection) = self.viewer.viewer_text_selection else {
            return false;
        };

        let (start, end) = selection.ordered();
        (start.page..=end.page).all(|page| self.viewer.viewer_text_layers.contains_key(&page))
    }

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

    pub(crate) fn render_width_px(&self) -> u16 {
        (f32::from(self.viewer.zoom_width) * self.viewer.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    pub(crate) fn render_height_px(&self, page: u16) -> u16 {
        (self.page_height(page) * self.viewer.scale_factor.max(1.0))
            .round()
            .clamp(1.0, f32::from(u16::MAX)) as u16
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.viewer_content_size(self.viewer.viewer_viewport_width)
            .height
    }

    pub(crate) fn content_width(&self) -> f32 {
        self.viewer_content_size(self.viewer.viewer_viewport_width)
            .width
    }

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

    pub(crate) fn viewer_page_rect_for_page(&self, target_page: u16) -> Option<Rectangle> {
        self.viewer_page_rects_content(self.viewer.viewer_viewport_width)
            .into_iter()
            .find_map(|(page, rect)| (page == target_page).then_some(rect))
    }

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
