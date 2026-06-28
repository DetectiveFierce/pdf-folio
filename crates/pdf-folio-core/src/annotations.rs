//! Annotation data model shared by storage and UI layers.

use std::fmt;

/// Stable annotation identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnnotationId(String);

impl AnnotationId {
    /// Creates a new annotation identifier.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AnnotationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A point in PDF page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PagePoint {
    /// Horizontal page coordinate.
    pub x: f32,
    /// Vertical page coordinate.
    pub y: f32,
}

/// A rectangle in PDF page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    /// Left page coordinate.
    pub x: f32,
    /// Top page coordinate.
    pub y: f32,
    /// Rectangle width.
    pub width: f32,
    /// Rectangle height.
    pub height: f32,
}

/// An sRGB color with alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorRgba {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

/// Annotation variants supported by PDF-Folio.
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationKind {
    /// Text highlight over one or more page rectangles.
    Highlight {
        rects: Vec<PageRect>,
        color: ColorRgba,
    },
    /// Sticky note anchored at a page coordinate.
    Note { position: PagePoint, body: String },
    /// Freehand drawing represented as strokes.
    Drawing {
        strokes: Vec<Vec<PagePoint>>,
        color: ColorRgba,
        width: f32,
    },
}

/// A user annotation attached to a PDF page.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// Stable annotation identifier.
    pub id: AnnotationId,
    /// Zero-based page index.
    pub page: u16,
    /// Annotation content.
    pub kind: AnnotationKind,
}
