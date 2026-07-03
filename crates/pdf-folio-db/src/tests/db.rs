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
