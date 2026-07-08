use super::*;

impl PDFolioApp {
    pub(crate) fn all_tags(&self) -> Vec<String> {
        let mut tags: Vec<String> = self
            .library
            .library_entries
            .iter()
            .flat_map(|entry| entry.tags.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();
        tags
    }

    pub(crate) fn request_visible_thumbnails(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let entries = self.visible_library_entries();
        let folder_section_height = folder_cards_section_height(self, self.child_folders().len());
        let entry_scroll_offset =
            (self.library.library_scroll_offset - folder_section_height).max(0.0);
        let visible_entries = if self.library.compact_view_mode {
            let window = self.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
            entries[window].to_vec()
        } else {
            let layout = self.library_masonry_layout(&entries);
            self.visible_library_masonry_layout_items_at(&layout, entry_scroll_offset)
                .into_iter()
                .filter_map(|item| entries.get(item.index).cloned())
                .collect()
        };
        let thumbnail_size = if self.library.compact_view_mode {
            ThumbnailSize::Default
        } else {
            self.thumbnail_size_for_grid_zoom()
        };
        for entry in visible_entries {
            let key = ThumbnailCacheKey {
                entry_id: entry.id.clone(),
                size: thumbnail_size,
            };
            if self.library.thumbnails.contains_key(&key)
                || self.library.pending_thumbnails.contains(&key)
            {
                continue;
            }
            self.library.pending_thumbnails.insert(key);
            tasks.push(Task::perform(
                load_or_render_thumbnail(entry, thumbnail_size),
                |result| match result {
                    Ok((entry_id, size, page)) => Message::ThumbnailReady {
                        entry_id,
                        size,
                        data: page.rgba,
                        width: page.width,
                        height: page.height,
                    },
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ));
        }

        Task::batch(tasks)
    }

    pub(crate) fn load_cached_visible_thumbnails(&mut self) {
        let entries = self.visible_thumbnail_entries();
        let preferred_size = if self.library.compact_view_mode {
            ThumbnailSize::Default
        } else {
            self.thumbnail_size_for_grid_zoom()
        };
        for entry in entries {
            for size in [
                preferred_size,
                ThumbnailSize::Default,
                ThumbnailSize::Large,
                ThumbnailSize::Small,
            ] {
                let key = ThumbnailCacheKey {
                    entry_id: entry.id.clone(),
                    size,
                };
                if self.library.thumbnails.contains_key(&key) {
                    continue;
                }
                if let Ok(Some(thumbnail)) = load_cached_thumbnail(&entry.id, size) {
                    self.library.thumbnails.insert(key, thumbnail);
                }
            }
        }
    }

    fn visible_thumbnail_entries(&self) -> Vec<LibraryEntry> {
        let entries = self.visible_library_entries();
        let folder_section_height = folder_cards_section_height(self, self.child_folders().len());
        let entry_scroll_offset =
            (self.library.library_scroll_offset - folder_section_height).max(0.0);
        if self.library.compact_view_mode {
            let window = self.visible_library_entry_window_at(entries.len(), entry_scroll_offset);
            entries[window].to_vec()
        } else {
            let layout = self.library_masonry_layout(&entries);
            self.visible_library_masonry_layout_items_at(&layout, entry_scroll_offset)
                .into_iter()
                .filter_map(|item| entries.get(item.index).cloned())
                .collect()
        }
    }

    pub(crate) fn refresh_library(&mut self) -> Task<Message> {
        let db = Arc::clone(&self.db);
        let sort_mode = self.library.library_sort_mode;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    db.purge_expired_trash(30)?;
                    Ok::<_, anyhow::Error>((
                        db.get_entries_sorted(sort_mode)?,
                        db.get_trashed_entries()?,
                    ))
                })
                .await?
            },
            |result| match result {
                Ok((entries, trash_entries)) => Message::LibraryLoaded {
                    entries,
                    trash_entries,
                },
                Err(error) => Message::LibraryError(error.to_string()),
            },
        )
    }

    pub(crate) fn refresh_folders(&self) -> Task<Message> {
        let db = Arc::clone(&self.db);
        let trash_db = Arc::clone(&self.db);
        Task::batch([
            Task::perform(
                async move { tokio::task::spawn_blocking(move || db.get_folders()).await? },
                |result| match result {
                    Ok(folders) => Message::LibraryFoldersLoaded(folders),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ),
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || trash_db.get_trashed_folders()).await?
                },
                |result| match result {
                    Ok(folders) => Message::LibraryTrashFoldersLoaded(folders),
                    Err(error) => Message::LibraryError(error.to_string()),
                },
            ),
        ])
    }
}
