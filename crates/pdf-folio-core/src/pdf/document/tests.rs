use super::*;

fn fixture_pdf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase1-single-page.pdf")
}

fn multipage_fixture_pdf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase1-multipage.pdf")
}

#[test]
fn opens_fixture_pdf() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;

    assert_eq!(doc.page_count(), 1);
    assert!(doc.path().ends_with("phase1-single-page.pdf"));

    Ok(())
}

#[test]
fn opens_multipage_fixture_pdf() -> Result<()> {
    let doc = PdfDoc::open(&multipage_fixture_pdf())?;

    assert_eq!(doc.page_count(), 84);
    assert!(doc.path().ends_with("phase1-multipage.pdf"));
    let outline = doc.outline()?;
    assert!(!outline.is_empty());
    assert!(outline_has_page_target(&outline));

    Ok(())
}

#[test]
fn renders_page_zero_as_rgba() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;
    let rendered = doc.render_page(0, 320)?;

    assert_eq!(rendered.width, 320);
    assert!(rendered.height > 0);
    assert_eq!(
        rendered.rgba.len(),
        rendered.width as usize * rendered.height as usize * 4
    );

    Ok(())
}

#[test]
fn reports_plausible_page_aspect_ratio() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;
    let ratio = doc.page_aspect_ratio(0)?;

    assert!(ratio > 0.5 && ratio < 3.0, "unexpected ratio: {ratio}");

    Ok(())
}

#[test]
fn extracts_fixture_text() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;
    let text = doc.text_on_page(0)?;

    assert!(text.contains("PDF-Folio Phase 1 Fixture"));

    Ok(())
}

#[test]
fn extracts_fixture_text_layer_with_character_bounds() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;
    let layer = doc.text_layer(0)?;

    assert!(!layer.chars.is_empty());
    assert!(layer.width_points > 0.0);
    assert!(layer.height_points > 0.0);
    assert!(layer.chars.iter().any(|character| character.text == "P"));
    assert!(layer.chars.iter().any(|character| {
        character.bounds.width > 0.0
            && character.bounds.height > 0.0
            && character.bounds.x >= 0.0
            && character.bounds.y >= 0.0
    }));

    Ok(())
}

#[test]
fn returns_empty_outline_for_fixture_without_bookmarks() -> Result<()> {
    let doc = PdfDoc::open(&fixture_pdf())?;

    assert!(doc.outline()?.is_empty());

    Ok(())
}

fn outline_has_page_target(nodes: &[OutlineNode]) -> bool {
    nodes
        .iter()
        .any(|node| node.page.is_some() || outline_has_page_target(&node.children))
}
