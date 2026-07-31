use super::*;

#[test]
fn bundled_styles_compile() {
    let style_book = StyleBook::bundled();
    let tokens = style_book.tokens("espresso");
    // espresso.kdl: color "accent" "#E0B45A"
    assert_eq!(tokens.accent, Color::from_rgb8(0xE0, 0xB4, 0x5A));
    // Guard against re-embedding the full class table (stack-overflows iced views).
    assert!(
        std::mem::size_of::<ThemeTokens>() < 1024,
        "ThemeTokens grew too large: {} bytes",
        std::mem::size_of::<ThemeTokens>()
    );
}

#[test]
fn bundled_sources_include_every_bundled_kdl_file() {
    let bundled_dir = bundled_style_dir();
    let sources = bundled_style_sources().expect("bundled sources should load");
    let source_names = sources
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();

    for path in style_files_in_dir(&bundled_dir) {
        let source_name = path.display().to_string();
        assert!(
            source_names.contains(&source_name.as_str()),
            "{} should be included in bundled style sources",
            path.display()
        );
    }
}

#[test]
fn bundled_file_tree_active_border_uses_side_widths() {
    let style_book = StyleBook::bundled();

    // sidebar.kdl FileTree active: left width=3 accent rail (espresso + light).
    let espresso = style_book.tokens("espresso").class_styles[Class::FileTree.index()]
        .resolve(ComponentState::Active)
        .border
        .expect("active espresso file tree border should be set");
    assert_eq!(espresso.left.width, Some(3.0));
    assert!(espresso.uniform_style().is_none());

    let light = style_book.tokens("light").class_styles[Class::FileTree.index()]
        .resolve(ComponentState::Active)
        .border
        .expect("active light file tree border should be set");
    assert_eq!(light.left.width, Some(3.0));
    assert!(light.uniform_style().is_none());
}

#[test]
fn bundled_viewer_styles_are_independent_from_global_toolbar() {
    let style_book = StyleBook::bundled();
    let tokens = style_book.tokens("espresso");
    let viewer_toolbar = tokens.class_styles[Class::ViewerToolbar.index()]
        .resolve(ComponentState::Normal)
        .background;
    let global_toolbar = tokens.class_styles[Class::Toolbar.index()]
        .resolve(ComponentState::Normal)
        .background;
    let viewer_find = tokens.class_styles[Class::ViewerFindBar.index()]
        .resolve(ComponentState::Normal)
        .background;

    assert_eq!(viewer_toolbar, global_toolbar);
    assert!(viewer_find.is_some());
    assert_eq!(style_book.layout().viewer_find_bar_width, 420.0);
}

#[test]
fn user_style_files_include_nested_kdl_files() {
    let root = std::env::temp_dir().join(format!("pdf-folio-style-test-{}", std::process::id()));
    let nested = root.join("components").join("library");
    std::fs::create_dir_all(&nested).expect("nested test style dir should be created");
    let top_level = root.join("theme.kdl");
    let nested_file = nested.join("sidebar.kdl");
    std::fs::write(&top_level, "").expect("top-level test style should be written");
    std::fs::write(&nested_file, "").expect("nested test style should be written");

    let files = user_style_files(&root);

    assert!(files.contains(&top_level));
    assert!(files.contains(&nested_file));
    assert!(
        files.iter().position(|path| path == &top_level)
            < files.iter().position(|path| path == &nested_file),
        "theme overrides should be loaded before component overrides"
    );

    std::fs::remove_dir_all(&root).expect("test style dir should be removed");
}

#[test]
fn invalid_color_is_rejected() {
    let result = StyleBook::from_sources(
        vec![(
            "bad.kdl".to_owned(),
            r##"theme "espresso" { color "accent" "wat" }"##.to_owned(),
        )],
        Vec::new(),
    );
    assert!(result.is_err());
}

#[test]
fn unknown_class_is_rejected() {
    let result = StyleBook::from_sources(
        vec![(
            "bad.kdl".to_owned(),
            r##"
                theme "espresso" {}
                theme "light" {}
                component "Nope" { normal background="#000000" }
                "##
            .to_owned(),
        )],
        Vec::new(),
    );
    assert!(result.is_err());
}

#[test]
fn token_references_and_mix_expressions_compile() {
    let style_book = StyleBook::from_sources(
        vec![(
            "style.kdl".to_owned(),
            r##"
                theme "espresso" { color "accent" "#D4A853" }
                theme "light" { color "accent" "#9C732B" }
                component "ToolbarButton" {
                    normal background="$surface" text="mix($accent, #000000, 0.5)"
                }
                "##
            .to_owned(),
        )],
        Vec::new(),
    )
    .expect("style book should compile");

    let tokens = style_book.tokens("espresso");
    let style = tokens.class_styles[Class::ToolbarButton.index()].resolve(ComponentState::Normal);
    assert_eq!(style.background, Some(tokens.surface));
    assert!(style.text_color.is_some());
}

#[test]
fn border_shorthand_applies_to_all_sides() {
    let style_book = StyleBook::from_sources(
        vec![(
            "style.kdl".to_owned(),
            r##"
                theme "espresso" {}
                theme "light" {}
                component "Toolbar" {
                    normal {
                        border width=0 color="#00000000"
                    }
                }
                "##
            .to_owned(),
        )],
        Vec::new(),
    )
    .expect("style book should compile");

    let style = style_book.tokens("espresso").class_styles[Class::Toolbar.index()]
        .resolve(ComponentState::Normal);
    let border = style.border.expect("border should be set");
    assert_eq!(border.uniform_style(), Some((0.0, Color::TRANSPARENT)));
}

#[test]
fn border_sides_can_override_width_and_color_independently() {
    let style_book = StyleBook::from_sources(
        vec![(
            "style.kdl".to_owned(),
            r##"
                theme "espresso" {}
                theme "light" {}
                component "Toolbar" {
                    normal {
                        border width=1 color="#111111" {
                            top width=2 color="#222222"
                            right width=3 color="#333333"
                            bottom width=4 color="#444444"
                            left width=0 color="#00000000"
                        }
                    }
                }
                "##
            .to_owned(),
        )],
        Vec::new(),
    )
    .expect("style book should compile");

    let style = style_book.tokens("espresso").class_styles[Class::Toolbar.index()]
        .resolve(ComponentState::Normal);
    let border = style.border.expect("border should be set");
    assert_eq!(border.top.width, Some(2.0));
    assert_eq!(border.top.color, Some(Color::from_rgb8(0x22, 0x22, 0x22)));
    assert_eq!(border.right.width, Some(3.0));
    assert_eq!(border.right.color, Some(Color::from_rgb8(0x33, 0x33, 0x33)));
    assert_eq!(border.bottom.width, Some(4.0));
    assert_eq!(
        border.bottom.color,
        Some(Color::from_rgb8(0x44, 0x44, 0x44))
    );
    assert_eq!(border.left.width, Some(0.0));
    assert_eq!(border.left.color, Some(Color::TRANSPARENT));
}


