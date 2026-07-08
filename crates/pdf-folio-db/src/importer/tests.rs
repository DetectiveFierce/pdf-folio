use super::*;

#[test]
fn title_from_path_uses_clean_filename_stem() {
    assert_eq!(
        title_from_path(Path::new("/tmp/  Quarterly   Report .pdf")),
        Some(String::from("Quarterly Report"))
    );
    assert_eq!(title_from_path(Path::new("/tmp/Untitled.pdf")), None);
}
