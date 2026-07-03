use super::*;
use crate::library::drag::folder_drop_target_ready;

fn test_db(label: &str) -> Db {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "pdf-folio-ui-{label}-{}-{nanos}.db",
        std::process::id()
    ));
    Db::open(path).expect("test database should open")
}

fn test_new_entry(id: &str) -> NewLibraryEntry {
    NewLibraryEntry {
        id: EntryId::new(id),
        path: PathBuf::from(format!("/tmp/{id}.pdf")),
        title: Some(id.to_owned()),
        author: None,
        author_attributed: false,
        page_count_attributed: false,
        page_count: None,
        file_size: None,
        cover_hash: None,
    }
}

#[test]
fn range_selection_ids_preserves_visible_order_forward() {
    let entries = ["a", "b", "c", "d"].map(EntryId::new);
    let ids = range_selection_ids(1, 3, &entries);

    assert_eq!(
        ids.iter().map(EntryId::as_str).collect::<Vec<_>>(),
        vec!["b", "c", "d"]
    );
}

#[test]
fn range_selection_ids_preserves_visible_order_backward() {
    let entries = ["a", "b", "c", "d"].map(EntryId::new);
    let ids = range_selection_ids(3, 1, &entries);

    assert_eq!(
        ids.iter().map(EntryId::as_str).collect::<Vec<_>>(),
        vec!["b", "c", "d"]
    );
}

#[test]
fn checkbox_toggle_adds_and_removes_one_entry_without_clearing_others() {
    let mut selected = HashSet::from([EntryId::new("a"), EntryId::new("b")]);

    toggle_selection_entry_id(&mut selected, EntryId::new("c"));
    assert!(selected.contains(&EntryId::new("a")));
    assert!(selected.contains(&EntryId::new("b")));
    assert!(selected.contains(&EntryId::new("c")));

    toggle_selection_entry_id(&mut selected, EntryId::new("b"));
    assert!(selected.contains(&EntryId::new("a")));
    assert!(!selected.contains(&EntryId::new("b")));
    assert!(selected.contains(&EntryId::new("c")));
}

#[test]
fn master_checkbox_state_reflects_none_partial_and_all_visible_selection() {
    assert_eq!(
        master_checkbox_state_for_counts(0, 4),
        MasterCheckboxState::None
    );
    assert_eq!(
        master_checkbox_state_for_counts(2, 4),
        MasterCheckboxState::Partial
    );
    assert_eq!(
        master_checkbox_state_for_counts(4, 4),
        MasterCheckboxState::All
    );
    assert_eq!(
        master_checkbox_state_for_counts(0, 0),
        MasterCheckboxState::None
    );
}

#[test]
fn metadata_density_round_trips_visible_field_preferences() {
    for density in [
        LibraryMetadataDensity::Minimal,
        LibraryMetadataDensity::Standard,
        LibraryMetadataDensity::Detailed,
    ] {
        assert_eq!(
            LibraryMetadataDensity::from_visible_fields(&density.visible_fields()),
            density
        );
    }
}

#[test]
fn reading_state_uses_progress_and_known_page_count() {
    assert_eq!(
        library_reading_state(0, Some(12)),
        LibraryReadingFilter::Unread
    );
    assert_eq!(
        library_reading_state(3, Some(12)),
        LibraryReadingFilter::Reading
    );
    assert_eq!(
        library_reading_state(11, Some(12)),
        LibraryReadingFilter::Finished
    );
    assert_eq!(
        library_reading_state(4, None),
        LibraryReadingFilter::Reading
    );
}

#[test]
fn viewer_text_selection_orders_character_ranges_from_drag_endpoints() {
    let mut selection = ViewerTextSelection::new(ViewerTextAnchor::new(2, 9));
    selection.focus = ViewerTextAnchor::new(1, 3);

    assert_eq!(
        selection.ordered(),
        (ViewerTextAnchor::new(1, 3), ViewerTextAnchor::new(2, 9))
    );
    assert_eq!(selection.char_range_for_page(1, 12), Some(3..=11));
    assert_eq!(selection.char_range_for_page(2, 12), Some(0..=9));
    assert!(!selection.contains_page(3));
}

#[test]
fn viewer_spread_groups_pair_odd_pages_on_left() {
    assert_eq!(
        viewer_spread_groups(5, ViewerSpreadMode::Odd),
        vec![vec![0, 1], vec![2, 3], vec![4]]
    );
}

#[test]
fn viewer_spread_groups_leave_cover_alone_for_even_spreads() {
    assert_eq!(
        viewer_spread_groups(5, ViewerSpreadMode::Even),
        vec![vec![0], vec![1, 2], vec![3, 4]]
    );
}

#[test]
fn selected_render_key_prefers_exact_preview_then_nearest() {
    let target = TileKey {
        page: 2,
        width_px: 1000,
    };
    let keys = vec![
        TileKey {
            page: 2,
            width_px: 760,
        },
        TileKey {
            page: 2,
            width_px: 900,
        },
        TileKey {
            page: 1,
            width_px: 1000,
        },
    ];

    assert_eq!(
        selected_render_key(keys.iter(), target, Some(760), true),
        Some(TileKey {
            page: 2,
            width_px: 760
        })
    );

    assert_eq!(
        selected_render_key(keys.iter(), target, None, true),
        Some(TileKey {
            page: 2,
            width_px: 900
        })
    );

    let keys_with_exact = keys
        .iter()
        .copied()
        .chain(std::iter::once(target))
        .collect::<Vec<_>>();
    assert_eq!(
        selected_render_key(keys_with_exact.iter(), target, Some(760), true),
        Some(target)
    );
}

#[test]
fn selected_render_key_returns_none_without_same_page_image() {
    let target = TileKey {
        page: 2,
        width_px: 1000,
    };
    let keys = [TileKey {
        page: 1,
        width_px: 1000,
    }];

    assert_eq!(selected_render_key(keys.iter(), target, None, true), None);
}

#[test]
fn prefetch_page_order_prioritizes_visible_then_directional_margin() {
    assert_eq!(
        prefetch_page_order_for_range(4..6, 10, true),
        vec![4, 5, 3, 6, 7, 8]
    );
    assert_eq!(
        prefetch_page_order_for_range(4..6, 10, false),
        vec![4, 5, 3, 6, 2, 1]
    );
    assert_eq!(prefetch_page_order_for_range(0..1, 2, false), vec![0, 1]);
}

#[test]
fn stale_page_render_completion_is_discarded() {
    let mut app = PDFolioApp::new().expect("app should initialize");
    let key = TileKey {
        page: 0,
        width_px: 800,
    };
    app.zoom_generation = 2;
    app.pending_renders.insert(key, Some(1));

    let _ = update(
        &mut app,
        Message::PageRendered {
            key,
            data: Vec::new(),
            width: 1,
            height: 1,
            generation: Some(1),
        },
    );

    assert!(!app.rendered_pages.contains_key(&key));
    assert!(app.cache.is_empty());
    assert!(!app.pending_renders.contains_key(&key));
}

#[test]
fn root_library_scope_shows_only_unfiled_entries() {
    let db = test_db("root-folder-scope");
    db.insert_entry(&test_new_entry("unfiled")).unwrap();
    db.insert_entry(&test_new_entry("filed")).unwrap();
    let folder = db.create_folder("Reading", None).unwrap();
    db.add_entry_to_folder(&EntryId::new("filed"), &folder)
        .unwrap();
    let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
    let unfiled = entries
        .iter()
        .find(|entry| entry.id == EntryId::new("unfiled"))
        .unwrap();
    let filed = entries
        .iter()
        .find(|entry| entry.id == EntryId::new("filed"))
        .unwrap();

    assert!(entry_visible_in_folder_scope(unfiled, None));
    assert!(!entry_visible_in_folder_scope(filed, None));
    assert!(entry_visible_in_folder_scope(filed, Some(&folder)));
    assert!(!entry_visible_in_folder_scope(unfiled, Some(&folder)));
}

#[test]
fn multi_drag_reorder_preserves_selected_relative_order() {
    let entries = ["a", "b", "c", "d", "e"].map(EntryId::new);
    let dragged = ["b", "d"].map(EntryId::new);
    let result = reorder_entry_ids_for_drag(&entries, &dragged, 2);

    assert_eq!(
        result.iter().map(EntryId::as_str).collect::<Vec<_>>(),
        vec!["a", "c", "b", "d", "e"]
    );
}

#[test]
fn multi_drag_reorder_can_append_selected_group() {
    let entries = ["a", "b", "c", "d"].map(EntryId::new);
    let dragged = ["a", "c"].map(EntryId::new);
    let result = reorder_entry_ids_for_drag(&entries, &dragged, usize::MAX);

    assert_eq!(
        result.iter().map(EntryId::as_str).collect::<Vec<_>>(),
        vec!["b", "d", "a", "c"]
    );
}

#[test]
fn multi_drag_placeholder_count_matches_visible_selection_size() {
    let entries = ["a", "b", "c", "d"].map(EntryId::new);
    let dragged = ["b", "d", "missing"].map(EntryId::new);

    assert_eq!(dragged_placeholder_count(&entries, &dragged), 2);
}

#[test]
fn folder_drag_reorder_moves_folder_before_hovered_sibling() {
    let folders = ["a", "b", "c", "d"].map(FolderId::new);

    let result =
        reorder_folder_ids_before_target(&folders, &FolderId::new("d"), &FolderId::new("b"))
            .unwrap();

    assert_eq!(
        result.iter().map(FolderId::as_str).collect::<Vec<_>>(),
        vec!["a", "d", "b", "c"]
    );
}

#[test]
fn folder_drop_target_activates_after_dwell_delay() {
    let started_at = Instant::now();

    assert!(!folder_drop_target_ready(
        started_at,
        started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS - 1)
    ));
    assert!(folder_drop_target_ready(
        started_at,
        started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS)
    ));
}

#[test]
fn folder_drop_target_hit_testing_resolves_cursor_bounds() {
    let reading = FolderId::new("reading");
    let archive = FolderId::new("archive");
    let targets = vec![
        (
            reading.clone(),
            Rectangle {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 40.0,
            },
        ),
        (
            archive.clone(),
            Rectangle {
                x: 10.0,
                y: 72.0,
                width: 100.0,
                height: 40.0,
            },
        ),
    ];

    assert_eq!(
        folder_drop_target_at_cursor(Point::new(24.0, 36.0), &targets),
        Some(reading)
    );
    assert_eq!(
        folder_drop_target_at_cursor(Point::new(24.0, 84.0), &targets),
        Some(archive)
    );
    assert_eq!(
        folder_drop_target_at_cursor(Point::new(4.0, 84.0), &targets),
        None
    );
}

#[test]
fn folder_move_target_rejects_self_and_descendants() {
    let db = test_db("folder-move-target");
    let root = db.create_folder("Root", None).unwrap();
    let child = db.create_folder("Child", Some(&root)).unwrap();
    let sibling = db.create_folder("Sibling", None).unwrap();
    let folders = db.get_folders().unwrap();

    assert!(!folder_can_move_into(&folders, &root, &root));
    assert!(!folder_can_move_into(&folders, &root, &child));
    assert!(folder_can_move_into(&folders, &child, &sibling));
}

#[test]
fn folder_card_target_hit_test_uses_grid_cells() {
    let db = test_db("folder-card-hit-test");
    let first = db.create_folder("First", None).unwrap();
    let second = db.create_folder("Second", None).unwrap();
    let third = db.create_folder("Third", None).unwrap();
    let folders = db.get_folders().unwrap();

    assert_eq!(
        folder_card_target_at_cursor(
            Point::new(132.0, 18.0),
            &folders,
            &first,
            0.0,
            0.0,
            0.0,
            100.0,
            40.0,
            12.0,
            8.0,
            2,
        ),
        Some(second)
    );
    assert_eq!(
        folder_card_target_at_cursor(
            Point::new(18.0, 18.0),
            &folders,
            &first,
            0.0,
            0.0,
            0.0,
            100.0,
            40.0,
            12.0,
            8.0,
            2,
        ),
        None
    );
    assert_eq!(
        folder_card_target_at_cursor(
            Point::new(108.0, 18.0),
            &folders,
            &third,
            0.0,
            0.0,
            0.0,
            100.0,
            40.0,
            12.0,
            8.0,
            2,
        ),
        None
    );
    assert_eq!(
        folder_card_target_at_cursor(
            Point::new(18.0, 58.0),
            &folders,
            &first,
            0.0,
            0.0,
            0.0,
            100.0,
            40.0,
            12.0,
            8.0,
            2,
        ),
        Some(third)
    );
}

#[test]
fn entry_search_fields_match_folder_names() {
    assert!(entry_search_fields_match(
        "Quarterly Report",
        "Analyst",
        "/tmp/report.pdf",
        ["finance"].into_iter(),
        ["Research Archive"].into_iter(),
        "archive",
    ));
    assert!(!entry_search_fields_match(
        "Quarterly Report",
        "Analyst",
        "/tmp/report.pdf",
        ["finance"].into_iter(),
        ["Research Archive"].into_iter(),
        "cookbook",
    ));
}

#[test]
fn search_match_source_label_reports_metadata_source() {
    assert_eq!(
        search_match_source_label_for_fields(
            "Quarterly Report",
            "Analyst",
            "/tmp/reports/quarterly.pdf",
            ["finance"],
            ["Research Archive"],
            "quarterly"
        ),
        Some(String::from("Match in title"))
    );
    assert_eq!(
        search_match_source_label_for_fields(
            "Quarterly Report",
            "Analyst",
            "/tmp/reports/quarterly.pdf",
            ["finance"],
            ["Research Archive"],
            "analyst"
        ),
        Some(String::from("Match in author"))
    );
    assert_eq!(
        search_match_source_label_for_fields(
            "Quarterly Report",
            "Analyst",
            "/tmp/reports/quarterly.pdf",
            ["finance"],
            ["Research Archive"],
            "finance"
        ),
        Some(String::from("Match in tag"))
    );
    assert_eq!(
        search_match_source_label_for_fields(
            "Quarterly Report",
            "Analyst",
            "/tmp/reports/quarterly.pdf",
            ["finance"],
            ["Research Archive"],
            "archive"
        ),
        Some(String::from("Match in folder"))
    );
    assert_eq!(
        search_match_source_label_for_fields(
            "Quarterly Report",
            "Analyst",
            "/tmp/reports/quarterly.pdf",
            ["finance"],
            ["Research Archive"],
            "reports"
        ),
        Some(String::from("Match in path"))
    );
}

#[test]
fn import_title_cleanup_rejects_poor_titles_and_cleans_spacing() {
    assert_eq!(
        clean_import_title("  Quarterly   Report  "),
        Some(String::from("Quarterly Report"))
    );
    assert_eq!(clean_import_title("Untitled"), None);
    assert_eq!(
        title_from_path(Path::new("/tmp/Research Notes.pdf")),
        Some(String::from("Research Notes"))
    );
}

#[test]
fn file_manager_command_targets_parent_for_containing_folder() {
    let path = PathBuf::from("/tmp/pdf-folio/report.pdf");

    let (_, args) = file_manager_command(&path, false).unwrap();
    assert!(args.iter().any(|arg| arg.contains("pdf-folio")));

    let (program, reveal_args) = file_manager_command(&path, true).unwrap();
    assert!(!program.is_empty());
    assert!(!reveal_args.is_empty());
}

#[test]
fn file_uri_escapes_spaces_for_file_manager_reveal() {
    assert_eq!(
        file_uri(Path::new("/tmp/pdf folio/report draft.pdf")),
        "file:///tmp/pdf%20folio/report%20draft.pdf"
    );
}

#[test]
fn duplicate_status_label_reports_unique_and_matching_count() {
    assert_eq!(duplicate_status_label_for_count(0), "Unique content hash");
    assert_eq!(duplicate_status_label_for_count(2), "2 matching duplicates");
}

#[test]
fn folder_smart_count_labels_include_progress_and_missing_state() {
    let counts = FolderSmartCounts {
        total: 12,
        in_progress: 3,
        missing: 1,
    };

    assert_eq!(
        folder_meta_label(counts, 2),
        "12 PDFs . 2 Folders . 3 reading . 1 missing"
    );
    assert_eq!(folder_sidebar_count_label(counts), "12 PDFs");
    assert_eq!(folder_meta_label(FolderSmartCounts::default(), 0), "Empty");
}

#[test]
fn indeterminate_progress_value_stays_inside_progress_bar_range() {
    for elapsed in [0.0, 0.25, 0.75, 1.5, 4.25, 12.0] {
        let value = indeterminate_progress_value(elapsed);
        assert!((0.0..=1.0).contains(&value));
    }
}

#[test]
fn folder_drop_flash_expires_after_success_window() {
    let folder_id = FolderId::new("reading");
    let started_at = Instant::now();

    assert!(folder_drop_flash_active_at(
        &folder_id,
        Some((&folder_id, started_at)),
        started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS - 1)
    ));
    assert!(!folder_drop_flash_active_at(
        &folder_id,
        Some((&folder_id, started_at)),
        started_at + Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
    ));
}

#[test]
fn drag_auto_scroll_is_idle_outside_edge_bands() {
    assert_eq!(drag_auto_scroll_velocity(240.0, 100.0, 400.0), 0.0);
}

#[test]
fn drag_auto_scroll_velocity_tracks_nearest_edge_direction() {
    let up = drag_auto_scroll_velocity(110.0, 100.0, 400.0);
    let down = drag_auto_scroll_velocity(490.0, 100.0, 400.0);

    assert!(up < 0.0);
    assert!(down > 0.0);
    assert!((up.abs() - down).abs() < 0.01);
}

#[test]
fn drag_auto_scroll_velocity_clamps_outside_viewport() {
    let above = drag_auto_scroll_velocity(0.0, 100.0, 400.0);
    let below = drag_auto_scroll_velocity(600.0, 100.0, 400.0);

    assert_eq!(above, -LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED);
    assert_eq!(below, LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED);
}

#[test]
fn drag_auto_scroll_edge_band_shrinks_for_short_viewports() {
    let center = drag_auto_scroll_velocity(125.0, 100.0, 50.0);
    let top = drag_auto_scroll_velocity(101.0, 100.0, 50.0);

    assert_eq!(center, 0.0);
    assert!(top < 0.0);
}

#[test]
fn masonry_target_index_uses_card_midpoints_as_insertion_slots() {
    let layout = LibraryMasonryLayout {
        columns: vec![vec![
            LibraryMasonryItem {
                index: 0,
                top: 0.0,
                height: 100.0,
            },
            LibraryMasonryItem {
                index: 2,
                top: 120.0,
                height: 100.0,
            },
        ]],
        content_height: 220.0,
    };

    assert_eq!(masonry_target_index(&layout, 0, 49.0), Some(0));
    assert_eq!(masonry_target_index(&layout, 0, 50.0), Some(2));
    assert_eq!(masonry_target_index(&layout, 0, 220.0), Some(3));
}

#[test]
fn masonry_target_index_empty_column_appends_to_compact_items() {
    let layout = LibraryMasonryLayout {
        columns: vec![
            vec![LibraryMasonryItem {
                index: 0,
                top: 0.0,
                height: 100.0,
            }],
            Vec::new(),
        ],
        content_height: 100.0,
    };

    assert_eq!(masonry_target_index(&layout, 1, 20.0), Some(1));
}

#[test]
fn style_watch_event_reloads_for_kdl_changes() {
    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![PathBuf::from("styles/components/library/sidebar.kdl")],
        attrs: notify::event::EventAttributes::new(),
    };

    assert!(style_watch_event_should_reload(&event));
}

#[test]
fn style_watch_event_reloads_for_directory_changes() {
    let root =
        std::env::temp_dir().join(format!("pdf-folio-style-watch-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("test style dir should be created");
    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Name(
            notify::event::RenameMode::Both,
        )),
        paths: vec![root.clone()],
        attrs: notify::event::EventAttributes::new(),
    };

    assert!(style_watch_event_should_reload(&event));

    std::fs::remove_dir_all(root).expect("test style dir should be removed");
}

#[test]
fn style_watch_event_ignores_unrelated_paths() {
    let event = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![PathBuf::from("README.md")],
        attrs: notify::event::EventAttributes::new(),
    };

    assert!(!style_watch_event_should_reload(&event));
}
