use super::*;

fn test_db() -> Db {
    let path = std::env::temp_dir().join(format!(
        "pdf-folio-db-{}-{}.db",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    Db::open(path).expect("test database should open")
}

fn entry(id: &str, title: &str) -> NewLibraryEntry {
    NewLibraryEntry {
        id: EntryId::new(id),
        path: PathBuf::from(format!("/tmp/{id}.pdf")),
        title: Some(title.to_owned()),
        author: None,
        author_attributed: false,
        page_count_attributed: false,
        page_count: Some(10),
        file_size: Some(1024),
        cover_hash: None,
    }
}

#[test]
fn inserts_entries_with_gapped_manual_order_and_reorders_them() {
    let db = test_db();
    db.insert_entry(&entry("a", "Alpha")).unwrap();
    db.insert_entry(&entry("b", "Beta")).unwrap();
    db.insert_entry(&entry("c", "Gamma")).unwrap();

    let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert!(entries[1].manual_order - entries[0].manual_order >= MANUAL_ORDER_GAP);

    db.set_manual_entry_order(&[EntryId::new("c"), EntryId::new("a"), EntryId::new("b")])
        .unwrap();

    let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["c", "a", "b"]
    );
}

#[test]
fn updates_and_resets_display_metadata() {
    let db = test_db();
    let id = EntryId::new("book");
    db.insert_entry(&entry("book", "The Book")).unwrap();

    db.update_display_metadata(&id, Some("  A Better Book  "), Some(" Author Name "))
        .unwrap();
    db.apply_title_sort_cleanup(&id).unwrap();

    let entry = db
        .entry_by_path(Path::new("/tmp/book.pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(entry.display_title.as_deref(), Some("A Better Book"));
    assert_eq!(entry.display_author.as_deref(), Some("Author Name"));
    assert_eq!(entry.sort_title.as_deref(), Some("better book"));
    assert!(entry.metadata_locked);

    db.reset_display_metadata(&id).unwrap();
    let entry = db
        .entry_by_path(Path::new("/tmp/book.pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(entry.display_title, None);
    assert_eq!(entry.display_author, None);
    assert_eq!(entry.sort_title.as_deref(), Some("the book"));
    assert!(!entry.metadata_locked);
}

#[test]
fn relinks_missing_entry_to_new_path() {
    let db = test_db();
    let id = EntryId::new("book");
    db.insert_entry(&entry("book", "The Book")).unwrap();
    db.set_missing(&id, true).unwrap();

    let next_path = Path::new("/tmp/relinked-book.pdf");
    db.relink_entry_path(&id, next_path).unwrap();

    let entry = db.entry_by_path(next_path).unwrap().unwrap();
    assert_eq!(entry.id, id);
    assert!(!entry.missing);
}

#[test]
fn folders_support_membership_nesting_and_cascade() {
    let db = test_db();
    db.insert_entry(&entry("a", "Alpha")).unwrap();
    db.insert_entry(&entry("b", "Beta")).unwrap();

    let parent = db.create_folder("Work", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();
    db.add_entry_to_folder(&EntryId::new("a"), &parent).unwrap();
    db.add_entry_to_folder(&EntryId::new("b"), &parent).unwrap();
    db.add_entry_to_folder(&EntryId::new("a"), &child).unwrap();

    let folders = db.get_folders().unwrap();
    assert_eq!(folders.len(), 2);
    assert_eq!(
        folders
            .iter()
            .find(|folder| folder.id == child)
            .and_then(|folder| folder.parent_id.as_ref()),
        Some(&parent)
    );

    let entries = db.entries_in_folder(&parent).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(entries[0].folders.len(), 2);

    assert!(db.move_folder(&parent, Some(&child)).is_err());

    db.delete_entry(&EntryId::new("a")).unwrap();
    assert_eq!(db.entries_in_folder(&parent).unwrap().len(), 1);

    db.delete_folder(&parent).unwrap();
    assert!(db.get_folders().unwrap().is_empty());
    assert_eq!(db.get_all_entries().unwrap().len(), 1);
}

#[test]
fn folders_support_manual_sibling_reordering() {
    let db = test_db();
    let root_a = db.create_folder("Alpha", None).unwrap();
    let root_b = db.create_folder("Beta", None).unwrap();
    let root_c = db.create_folder("Gamma", None).unwrap();
    let child_a = db.create_folder("Child A", Some(&root_a)).unwrap();
    let child_b = db.create_folder("Child B", Some(&root_a)).unwrap();

    db.set_manual_folder_order(None, &[root_c.clone(), root_a.clone(), root_b.clone()])
        .unwrap();
    db.set_manual_folder_order(Some(&root_a), &[child_b.clone(), child_a.clone()])
        .unwrap();

    let folders = db.get_folders().unwrap();
    let root_order = folders
        .iter()
        .filter(|folder| folder.parent_id.is_none())
        .map(|folder| folder.id.clone())
        .collect::<Vec<_>>();
    let child_order = folders
        .iter()
        .filter(|folder| folder.parent_id.as_ref() == Some(&root_a))
        .map(|folder| folder.id.clone())
        .collect::<Vec<_>>();

    assert_eq!(root_order, vec![root_c, root_a.clone(), root_b]);
    assert_eq!(child_order, vec![child_b, child_a]);
    assert!(db
        .set_manual_folder_order(None, &[root_a, FolderId::new("missing")])
        .is_err());
}

#[test]
fn folder_assignment_is_additive_across_folders() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();

    let first = db.create_folder("Reading", None).unwrap();
    let second = db.create_folder("Research", None).unwrap();
    db.add_entry_to_folder(&entry_id, &first).unwrap();
    db.add_entry_to_folder(&entry_id, &second).unwrap();

    let entry = db
        .entry_by_path(Path::new("/tmp/paper.pdf"))
        .unwrap()
        .unwrap();
    let folder_names = entry
        .folders
        .iter()
        .map(|folder| folder.name.as_str())
        .collect::<Vec<_>>();
    assert!(folder_names.contains(&"Reading"));
    assert!(folder_names.contains(&"Research"));
}

#[test]
fn entries_move_restore_and_expire_from_trash() {
    let db = test_db();
    let id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let before_trash = db.library_organization_snapshot().unwrap();

    db.trash_entries([&id]).unwrap();
    assert!(db.get_all_entries().unwrap().is_empty());
    assert_eq!(db.get_trashed_entries().unwrap()[0].id, id);

    db.restore_library_organization_snapshot(&before_trash)
        .unwrap();
    assert_eq!(db.get_all_entries().unwrap()[0].id, id);
    assert!(db.get_trashed_entries().unwrap().is_empty());

    db.trash_entries([&id]).unwrap();
    db.restore_entries([&id]).unwrap();
    assert_eq!(db.get_all_entries().unwrap()[0].id, id);
    assert!(db.get_trashed_entries().unwrap().is_empty());

    db.trash_entries([&id]).unwrap();
    let expired = Utc::now().timestamp() - 31 * 24 * 60 * 60;
    let connection = rusqlite::Connection::open(db.path()).unwrap();
    connection
        .execute(
            "UPDATE entries SET trashed_at = ?1 WHERE id = ?2",
            rusqlite::params![expired, id.as_str()],
        )
        .unwrap();

    assert_eq!(db.purge_expired_trash(30).unwrap(), 1);
    assert!(db.get_trashed_entries().unwrap().is_empty());
    assert!(db
        .entry_by_path(Path::new("/tmp/paper.pdf"))
        .unwrap()
        .is_none());
}

#[test]
fn folder_tree_moves_to_trash_with_entries() {
    let db = test_db();
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let entry_id = EntryId::new("paper");
    let parent = db.create_folder("Projects", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();
    db.add_entry_to_folder(&entry_id, &child).unwrap();

    db.trash_folder_tree(&parent).unwrap();

    assert!(db.get_folders().unwrap().is_empty());
    assert_eq!(db.get_trashed_folders().unwrap().len(), 2);
    assert!(db.get_all_entries().unwrap().is_empty());
    assert_eq!(db.get_trashed_entries().unwrap()[0].id, entry_id);
}

#[test]
fn folder_tree_restores_from_trash_with_entries() {
    let db = test_db();
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let entry_id = EntryId::new("paper");
    let parent = db.create_folder("Projects", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();
    db.add_entry_to_folder(&entry_id, &child).unwrap();

    db.trash_folder_tree(&parent).unwrap();
    let restored = db.restore_folder_tree(&parent).unwrap();

    assert_eq!(restored, 3);
    assert_eq!(db.get_folders().unwrap().len(), 2);
    assert!(db.get_trashed_folders().unwrap().is_empty());
    let entry = db.get_all_entries().unwrap().pop().unwrap();
    assert_eq!(entry.id, entry_id);
    assert_eq!(entry.folders[0].id, child);
}

#[test]
fn trashed_child_folder_restores_to_root_when_parent_stays_trashed() {
    let db = test_db();
    let parent = db.create_folder("Projects", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();

    db.trash_folder_tree(&parent).unwrap();
    db.restore_folder_tree(&child).unwrap();

    let folders = db.get_folders().unwrap();
    let restored_child = folders.iter().find(|folder| folder.id == child).unwrap();
    assert!(restored_child.parent_id.is_none());
    assert_eq!(db.get_trashed_folders().unwrap()[0].id, parent);
}

#[test]
fn trashed_folder_tree_permanently_deletes_folder_and_entries() {
    let db = test_db();
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let entry_id = EntryId::new("paper");
    let parent = db.create_folder("Projects", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();
    db.add_entry_to_folder(&entry_id, &child).unwrap();

    db.trash_folder_tree(&parent).unwrap();
    let (deleted, entry_ids) = db.permanently_delete_trashed_folder_tree(&parent).unwrap();

    assert_eq!(deleted, 3);
    assert_eq!(entry_ids, vec![entry_id]);
    assert!(db.get_trashed_folders().unwrap().is_empty());
    assert!(db.get_trashed_entries().unwrap().is_empty());
    assert!(db.get_all_entries().unwrap().is_empty());
}

#[test]
fn organization_snapshot_restores_copied_folder_subtree() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();

    let parent = db.create_folder("Projects", None).unwrap();
    let child = db.create_folder("Drafts", Some(&parent)).unwrap();
    db.add_entry_to_folder(&entry_id, &parent).unwrap();
    db.add_entry_to_folder(&entry_id, &child).unwrap();
    let before = db.library_organization_snapshot().unwrap();

    let pasted_parent = db.create_folder("Archive", None).unwrap();
    let copied = db
        .copy_folder_subtree(&parent, Some(&pasted_parent))
        .unwrap();
    let after_copy = db.library_organization_snapshot().unwrap();
    assert!(after_copy.folders.iter().any(|folder| folder.id == copied));
    assert!(after_copy.folders.len() > before.folders.len());
    assert!(after_copy.entry_folders.len() > before.entry_folders.len());

    db.restore_library_organization_snapshot(&before).unwrap();
    assert_eq!(db.library_organization_snapshot().unwrap(), before);
}

#[test]
fn organization_snapshot_restores_entry_metadata_tags_and_root_order() {
    let db = test_db();
    let alpha = EntryId::new("alpha");
    let beta = EntryId::new("beta");
    db.insert_entry(&entry("alpha", "Alpha")).unwrap();
    db.insert_entry(&entry("beta", "Beta")).unwrap();
    let folder = db.create_folder("Reading", None).unwrap();
    db.add_entry_to_folder(&alpha, &folder).unwrap();
    db.add_tag(&alpha, "important").unwrap();
    db.update_display_metadata(&alpha, Some("Original Alpha"), Some("Original Author"))
        .unwrap();
    let before = db.library_organization_snapshot().unwrap();

    db.rename_folder(&folder, "Archive").unwrap();
    db.remove_tag(&alpha, "important").unwrap();
    db.add_tag(&alpha, "later").unwrap();
    db.update_display_metadata(&alpha, Some("Changed Alpha"), Some("Changed Author"))
        .unwrap();
    db.set_manual_entry_order(&[beta.clone(), alpha.clone()])
        .unwrap();

    db.restore_library_organization_snapshot(&before).unwrap();

    let folders = db.get_folders().unwrap();
    assert_eq!(folders[0].name, "Reading");
    let entries = db.get_entries_sorted(LibrarySortMode::Manual).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    let restored = db
        .entry_by_path(Path::new("/tmp/alpha.pdf"))
        .unwrap()
        .unwrap();
    assert_eq!(restored.display_title.as_deref(), Some("Original Alpha"));
    assert_eq!(restored.display_author.as_deref(), Some("Original Author"));
    assert_eq!(restored.tags, vec![String::from("important")]);
}

#[test]
fn organization_snapshot_search_changes_ignore_non_indexed_library_edits() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let folder = db.create_folder("Reading", None).unwrap();
    db.add_entry_to_folder(&entry_id, &folder).unwrap();
    let before = db.library_organization_snapshot().unwrap();

    db.rename_folder(&folder, "Archive").unwrap();
    db.add_tag(&entry_id, "later").unwrap();
    let folder_and_tag = db.library_organization_snapshot().unwrap();
    assert!(before.search_changed_entry_ids(&folder_and_tag).is_empty());

    db.update_display_metadata(&entry_id, Some("Changed Paper"), None)
        .unwrap();
    let metadata = db.library_organization_snapshot().unwrap();
    assert_eq!(
        folder_and_tag.search_changed_entry_ids(&metadata),
        vec![entry_id.clone()]
    );

    db.trash_entries([&entry_id]).unwrap();
    let trashed = db.library_organization_snapshot().unwrap();
    assert_eq!(metadata.search_changed_entry_ids(&trashed), vec![entry_id]);
}

#[test]
fn raindrop_mappings_preserve_remote_identity_and_local_folders() {
    let db = test_db();
    let source = db
        .upsert_import_source(
            "raindrop:test-user",
            "raindrop",
            Some("test-user"),
            Some("Test User"),
        )
        .unwrap();
    assert_eq!(source.kind, "raindrop");

    let root = db
        .upsert_raindrop_collection_mapping(&source.id, 10, None, "Papers", None)
        .unwrap()
        .0;
    let child = db
        .upsert_raindrop_collection_mapping(&source.id, 20, Some(10), "AI", None)
        .unwrap()
        .0;
    assert_eq!(
        db.raindrop_collection_folder(&source.id, 20).unwrap(),
        Some(child.clone())
    );

    let folders = db.get_folders().unwrap();
    assert_eq!(
        folders
            .iter()
            .find(|folder| folder.id == child)
            .and_then(|folder| folder.parent_id.as_ref()),
        Some(&root)
    );

    db.insert_entry(&entry("paper", "Remote Paper")).unwrap();
    db.upsert_raindrop_entry_mapping(&RaindropEntryMapping {
        source_id: source.id.clone(),
        raindrop_id: 99,
        entry_id: EntryId::new("paper"),
        collection_id: Some(20),
        remote_link: Some(String::from("https://raindrop.io/file.pdf")),
        remote_title: Some(String::from("Remote Paper")),
        remote_updated_at: Some(String::from("2026-07-03T12:00:00.000Z")),
        file_name: Some(String::from("paper.pdf")),
        file_size: Some(2048),
    })
    .unwrap();

    assert_eq!(
        db.raindrop_entry_id(&source.id, 99).unwrap(),
        Some(EntryId::new("paper"))
    );
}

#[test]
fn moving_entry_to_folder_replaces_existing_folder_memberships() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();

    let first = db.create_folder("Reading", None).unwrap();
    let second = db.create_folder("Research", None).unwrap();
    db.add_entry_to_folder(&entry_id, &first).unwrap();
    db.move_entry_to_folder(&entry_id, &second).unwrap();

    let entry = db
        .entry_by_path(Path::new("/tmp/paper.pdf"))
        .unwrap()
        .unwrap();
    let folder_names = entry
        .folders
        .iter()
        .map(|folder| folder.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(folder_names, vec!["Research"]);
}

#[test]
fn moving_entry_to_root_removes_folder_memberships() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();

    let first = db.create_folder("Reading", None).unwrap();
    let second = db.create_folder("Research", None).unwrap();
    db.add_entry_to_folder(&entry_id, &first).unwrap();
    db.add_entry_to_folder(&entry_id, &second).unwrap();
    db.move_entry_to_root(&entry_id).unwrap();

    let entry = db
        .entry_by_path(Path::new("/tmp/paper.pdf"))
        .unwrap()
        .unwrap();
    assert!(entry.folders.is_empty());
    assert!(db.entries_in_folder(&first).unwrap().is_empty());
    assert!(db.entries_in_folder(&second).unwrap().is_empty());
}

#[test]
fn library_preferences_round_trip() {
    let db = test_db();
    let folder = db.create_folder("Reading", None).unwrap();
    let collapsed_folder = db.create_folder("Collapsed", None).unwrap();
    let preferences = LibraryPreferences {
        sort_mode: LibrarySortMode::TitleAsc,
        layout_mode: LibraryLayoutMode::List,
        selected_folder: Some(folder.clone()),
        sidebar_width: 220.0,
        grid_zoom: 1.24,
        visible_metadata_fields: vec![String::from("author"), String::from("progress")],
        library_tree_root_expanded: false,
        collapsed_folder_ids: vec![collapsed_folder.clone()],
    };

    db.save_library_preferences(&preferences).unwrap();
    let loaded = db.library_preferences().unwrap();

    assert_eq!(loaded.sort_mode, LibrarySortMode::TitleAsc);
    assert_eq!(loaded.layout_mode, LibraryLayoutMode::List);
    assert_eq!(loaded.selected_folder, Some(folder));
    assert_eq!(loaded.sidebar_width, 220.0);
    assert_eq!(loaded.grid_zoom, 1.24);
    assert_eq!(
        loaded.visible_metadata_fields,
        vec![String::from("author"), String::from("progress")]
    );
    assert!(!loaded.library_tree_root_expanded);
    assert_eq!(loaded.collapsed_folder_ids, vec![collapsed_folder]);
}

#[test]
fn sync_metadata_tracks_updated_rows_and_checkpoints() {
    let db = test_db();
    db.upsert_sync_entry(&SyncEntryRow {
        id: EntryId::new("entry-old"),
        library_id: String::from("main"),
        title: Some(String::from("Old")),
        author: None,
        updated_at: 10,
        deleted_at: None,
    })
    .unwrap();
    db.upsert_sync_entry(&SyncEntryRow {
        id: EntryId::new("entry-new"),
        library_id: String::from("main"),
        title: Some(String::from("New")),
        author: Some(String::from("Ada")),
        updated_at: 20,
        deleted_at: None,
    })
    .unwrap();
    db.upsert_sync_folder(&SyncFolderRow {
        id: FolderId::new("folder-new"),
        library_id: String::from("main"),
        name: String::from("Reading"),
        parent_id: None,
        updated_at: 30,
        deleted_at: None,
    })
    .unwrap();
    db.upsert_sync_entry_folder(&SyncEntryFolderRow {
        entry_id: EntryId::new("entry-new"),
        folder_id: FolderId::new("folder-new"),
        updated_at: 40,
        deleted_at: None,
    })
    .unwrap();

    let entries = db.sync_entries_updated_since("main", 10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id.as_str(), "entry-new");
    let folders = db.sync_folders_updated_since("main", 20).unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "Reading");
    let memberships = db.sync_entry_folders_updated_since("main", 20).unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].folder_id.as_str(), "folder-new");

    assert_eq!(db.sync_checkpoint("main", "laptop").unwrap(), None);
    db.set_sync_checkpoint("main", "laptop", 40).unwrap();
    assert_eq!(db.sync_checkpoint("main", "laptop").unwrap(), Some(40));
}

#[test]
fn seed_sync_metadata_captures_current_library_state() {
    let db = test_db();
    let entry_id = EntryId::new("paper");
    db.insert_entry(&entry("paper", "Paper")).unwrap();
    let folder_id = db.create_folder("Research", None).unwrap();
    db.add_entry_to_folder(&entry_id, &folder_id).unwrap();

    let summary = db.seed_sync_metadata("default").unwrap();
    assert_eq!(summary.entries, 1);
    assert_eq!(summary.folders, 1);
    assert_eq!(summary.entry_folders, 1);

    let entries = db.sync_entries_updated_since("default", 0).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, entry_id);
    assert_eq!(entries[0].title.as_deref(), Some("Paper"));
    let folders = db.sync_folders_updated_since("default", 0).unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].id, folder_id);
    let memberships = db.sync_entry_folders_updated_since("default", 0).unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].entry_id.as_str(), "paper");
}
