use super::client::{
    zip_download_progress_basis_points, Raindrop, RaindropCollection, RaindropFile, RaindropRef,
};
use super::import::{
    mirror_collections, zip_extract_progress_basis_points, zip_import_progress_basis_points,
    PROGRESS_BASIS_POINTS_MAX,
};
use super::matching::{choose_import_strategy, RaindropImportStrategy, ZipMatchIndex};
use super::*;
use pdf_folio_core::Db;
use std::collections::{HashMap, HashSet};

#[test]
fn import_strategy_uses_zip_only_for_large_uploaded_file_batches() {
    let small_uploaded = (0..ZIP_IMPORT_THRESHOLD - 1)
        .map(uploaded_pdf)
        .collect::<Vec<_>>();
    assert_eq!(
        choose_import_strategy(&small_uploaded),
        RaindropImportStrategy::IndividualFiles
    );

    let large_uploaded = (0..ZIP_IMPORT_THRESHOLD)
        .map(uploaded_pdf)
        .collect::<Vec<_>>();
    assert_eq!(
        choose_import_strategy(&large_uploaded),
        RaindropImportStrategy::ZipExport
    );

    let mut mixed = (0..ZIP_IMPORT_THRESHOLD + 1)
        .map(uploaded_pdf)
        .collect::<Vec<_>>();
    mixed[0] = linked_pdf(0);
    assert_eq!(
        choose_import_strategy(&mixed),
        RaindropImportStrategy::ZipExport
    );
}

#[test]
fn zip_import_progress_fills_remaining_bar_after_download() {
    assert_eq!(
        zip_import_progress_basis_points(0, 8),
        ZIP_EXTRACTED_PROGRESS_BASIS_POINTS
    );
    assert_eq!(zip_import_progress_basis_points(4, 8), 7_500);
    assert_eq!(
        zip_import_progress_basis_points(8, 8),
        PROGRESS_BASIS_POINTS_MAX
    );
}

#[test]
fn zip_extract_progress_fills_gap_before_importing() {
    assert_eq!(
        zip_extract_progress_basis_points(0, 100),
        ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS
    );
    assert_eq!(zip_extract_progress_basis_points(50, 100), 4_375);
    assert_eq!(
        zip_extract_progress_basis_points(100, 100),
        ZIP_EXTRACTED_PROGRESS_BASIS_POINTS
    );
}

#[test]
fn zip_download_progress_fills_middle_quarter() {
    assert_eq!(
        zip_download_progress_basis_points(0, 100),
        ZIP_PREPARING_PROGRESS_BASIS_POINTS
    );
    assert_eq!(zip_download_progress_basis_points(50, 100), 2_500);
    assert_eq!(
        zip_download_progress_basis_points(100, 100),
        ZIP_DOWNLOADED_PROGRESS_BASIS_POINTS
    );
}

#[test]
fn zip_match_index_keeps_suffix_filename_matching() {
    let raindrops = vec![uploaded_pdf(42)];
    let index = ZipMatchIndex::new(&raindrops);
    let remaining = HashSet::from([0]);

    assert_eq!(
        index.match_entry(&remaining, "export-Uploaded 42.pdf", 1024),
        Some(0)
    );
}

#[test]
fn preview_candidate_preserves_import_metadata() {
    let raindrop = uploaded_pdf(42);
    let candidate = raindrop.to_candidate(&HashMap::new());
    let restored = candidate.to_raindrop();

    assert_eq!(restored.id, raindrop.id);
    assert_eq!(restored.download_link(), raindrop.download_link());
    assert!(restored.has_uploaded_file());
    assert_eq!(restored.tags, raindrop.tags);
    assert_eq!(restored.collection_id(), raindrop.collection_id());
}

#[test]
fn mirror_collections_reanchors_unresolved_parents_under_selected_root() {
    let db = test_db();
    let source_id = "raindrop:test";
    db.upsert_import_source(source_id, "raindrop", Some("test"), Some("Test"))
        .unwrap();
    let old_parent = RaindropCollection {
        id: 10,
        title: Some(String::from("Old Parent")),
        sort: 0,
        parent: None,
    };
    let child = RaindropCollection {
        id: 20,
        title: Some(String::from("Child")),
        sort: 0,
        parent: Some(RaindropRef { id: old_parent.id }),
    };
    mirror_collections(&db, source_id, &[old_parent.clone(), child.clone()], None).unwrap();

    let root = db.create_folder("Chosen Root", None).unwrap();
    mirror_collections(&db, source_id, &[child], Some(&root)).unwrap();

    let folders = db.get_folders().unwrap();
    let child_folder_id = db
        .raindrop_collection_folder(source_id, 20)
        .unwrap()
        .unwrap();
    let child_folder = folders
        .iter()
        .find(|folder| folder.id == child_folder_id)
        .unwrap();
    assert_eq!(child_folder.parent_id.as_ref(), Some(&root));
}

fn test_db() -> Db {
    let path = std::env::temp_dir().join(format!(
        "pdf-folio-raindrop-{}-{:?}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
    ));
    Db::open(path).expect("test database should open")
}

fn uploaded_pdf(id: usize) -> Raindrop {
    Raindrop {
        id: id as i64,
        title: Some(format!("Uploaded {id}.pdf")),
        link: format!("https://api.raindrop.io/v2/raindrop/{id}/file?type=application/pdf"),
        cover: None,
        media: Vec::new(),
        item_type: Some(String::from("document")),
        collection: None,
        tags: Vec::new(),
        file: Some(RaindropFile {
            name: Some(format!("Uploaded {id}.pdf")),
            link: None,
            size: Some(1024),
            mime_type: Some(String::from("application/pdf")),
        }),
        last_update: None,
        uploaded_file: false,
    }
}

fn linked_pdf(id: usize) -> Raindrop {
    Raindrop {
        id: id as i64,
        title: Some(format!("Linked {id}.pdf")),
        link: format!("https://example.com/linked-{id}.pdf"),
        cover: None,
        media: Vec::new(),
        item_type: Some(String::from("document")),
        collection: None,
        tags: Vec::new(),
        file: None,
        last_update: None,
        uploaded_file: false,
    }
}
