use super::*;

#[test]
fn indexes_and_searches_page_documents() -> Result<()> {
    let root = std::env::temp_dir().join(format!("pdf-folio-search-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let index = SearchIndex::open(&root)?;

    index.replace_entry_pages([
        IndexDocument {
            id: String::from("entry-a"),
            title: String::from("Algebra Notes"),
            author: String::from("Ada"),
            body: String::from("rings fields and groups"),
            page: 0,
        },
        IndexDocument {
            id: String::from("entry-a"),
            title: String::from("Algebra Notes"),
            author: String::from("Ada"),
            body: String::from("linear transformations"),
            page: 1,
        },
    ])?;

    let hits = index.search("linear", 10)?;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "entry-a");
    assert_eq!(hits[0].page, 1);

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
