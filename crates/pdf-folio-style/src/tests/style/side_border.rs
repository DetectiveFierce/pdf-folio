use super::*;

#[test]
fn side_border_rects_use_exact_side_widths() {
    let bounds = Rectangle {
        x: 10.0,
        y: 20.0,
        width: 200.0,
        height: 40.0,
    };
    let border = VisualBorder {
        top: BorderSide::new(2.0, Color::BLACK),
        right: BorderSide::new(4.0, Color::BLACK),
        bottom: BorderSide::new(6.0, Color::BLACK),
        left: BorderSide::new(12.0, Color::BLACK),
    };

    let [top, right, bottom, left] = side_border_rects(bounds, border);

    assert_eq!(top.1.height, 2.0);
    assert_eq!(right.1.width, 4.0);
    assert_eq!(right.1.x, 206.0);
    assert_eq!(bottom.1.height, 6.0);
    assert_eq!(bottom.1.y, 54.0);
    assert_eq!(left.1.width, 12.0);
    assert_eq!(left.1.x, 10.0);
}

#[test]
fn side_border_rects_clamp_negative_widths_to_zero() {
    let bounds = Rectangle {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 24.0,
    };
    let border = VisualBorder {
        left: BorderSide::new(-4.0, Color::BLACK),
        ..VisualBorder::EMPTY
    };

    let [_, _, _, left] = side_border_rects(bounds, border);

    assert_eq!(left.1.width, 0.0);
}
