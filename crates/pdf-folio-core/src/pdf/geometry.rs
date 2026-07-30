//! PDF page geometry primitives.

/// A normalized top-left-origin rectangle in page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRect {
    /// Left edge as a fraction of page width.
    pub x: f32,
    /// Top edge as a fraction of page height.
    pub y: f32,
    /// Width as a fraction of page width.
    pub width: f32,
    /// Height as a fraction of page height.
    pub height: f32,
}
