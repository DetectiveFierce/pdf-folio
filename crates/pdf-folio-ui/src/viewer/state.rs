//! Viewer state helper types.

use iced::widget::image;
use pdf_folio_core::{PageTextLayer, RenderedPage};

/// Direction and paging model used to arrange PDF pages in the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerScrollMode {
    /// Advance one page/spread at a time.
    Page,
    /// Stack pages or spreads top-to-bottom.
    Vertical,
    /// Place pages or spreads left-to-right.
    Horizontal,
    /// Wrap pages or spreads into rows that fit the viewport width.
    Wrapped,
}

impl ViewerScrollMode {
    /// All user-facing scroll modes in menu order.
    pub const ALL: [Self; 4] = [Self::Page, Self::Vertical, Self::Horizontal, Self::Wrapped];

    /// User-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Page => "Page Scrolling",
            Self::Vertical => "Vertical Scrolling",
            Self::Horizontal => "Horizontal Scrolling",
            Self::Wrapped => "Wrapped Scrolling",
        }
    }

    /// Short help text for menus.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Page => "one page at a time",
            Self::Vertical => "continuous vertical",
            Self::Horizontal => "continuous horizontal",
            Self::Wrapped => "rows wrap to viewport",
        }
    }
}

/// Two-page spread pairing behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerSpreadMode {
    /// Show one page per slot.
    None,
    /// Pair pages with odd-numbered pages on the left.
    Odd,
    /// Pair pages with even-numbered pages on the left, leaving the cover alone.
    Even,
}

impl ViewerSpreadMode {
    /// All user-facing spread modes in menu order.
    pub const ALL: [Self; 3] = [Self::None, Self::Odd, Self::Even];

    /// User-facing label.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No Spreads",
            Self::Odd => "Odd Spreads",
            Self::Even => "Even Spreads",
        }
    }
}

/// A rendered page prepared for display by iced.
#[derive(Debug, Clone)]
pub struct RenderedPageView {
    /// Rendered image width in pixels.
    pub width: u16,
    /// Rendered image height in pixels.
    pub height: u16,
    /// Iced image handle backed by RGBA pixels.
    pub handle: image::Handle,
}

impl From<RenderedPage> for RenderedPageView {
    fn from(page: RenderedPage) -> Self {
        Self {
            width: page.width,
            height: page.height,
            handle: image::Handle::from_rgba(
                u32::from(page.width),
                u32::from(page.height),
                page.rgba,
            ),
        }
    }
}

/// A concrete character position in the viewer text layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewerTextAnchor {
    /// Zero-based page index.
    pub page: u16,
    /// Zero-based character index inside the page text layer.
    pub char_index: usize,
}

impl ViewerTextAnchor {
    /// Creates a new character anchor.
    pub fn new(page: u16, char_index: usize) -> Self {
        Self { page, char_index }
    }
}

/// Per-character text selection state for the raster viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerTextSelection {
    /// Character where the drag selection started.
    pub anchor: ViewerTextAnchor,
    /// Character currently under the selection drag.
    pub focus: ViewerTextAnchor,
    /// Whether the pointer is still dragging the selection.
    pub dragging: bool,
}

impl ViewerTextSelection {
    /// Starts a new selection anchored to one character.
    pub fn new(anchor: ViewerTextAnchor) -> Self {
        Self {
            anchor,
            focus: anchor,
            dragging: true,
        }
    }

    /// Returns the ordered selection endpoints.
    pub fn ordered(self) -> (ViewerTextAnchor, ViewerTextAnchor) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }

    /// Returns whether a page is inside the selection.
    pub fn contains_page(self, page: u16) -> bool {
        let (start, end) = self.ordered();
        (start.page..=end.page).contains(&page)
    }

    /// Returns the selected character range for a single page.
    pub fn char_range_for_page(
        self,
        page: u16,
        page_char_count: usize,
    ) -> Option<std::ops::RangeInclusive<usize>> {
        if page_char_count == 0 || !self.contains_page(page) {
            return None;
        }

        let (start, end) = self.ordered();
        let last = page_char_count - 1;
        let start_index = if page == start.page {
            start.char_index.min(last)
        } else {
            0
        };
        let end_index = if page == end.page {
            end.char_index.min(last)
        } else {
            last
        };

        (start_index <= end_index).then_some(start_index..=end_index)
    }
}

/// One text match in the open PDF viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewerFindMatch {
    /// Zero-based page index.
    pub page: u16,
    /// Inclusive zero-based start character index inside the page text layer.
    pub start: usize,
    /// Exclusive zero-based end character index inside the page text layer.
    pub end: usize,
}

impl ViewerFindMatch {
    /// Returns the inclusive range used by highlight drawing helpers.
    pub fn char_range(self) -> Option<std::ops::RangeInclusive<usize>> {
        (self.start < self.end).then_some(self.start..=self.end - 1)
    }
}

/// User-visible find-in-document state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerFindState {
    /// Whether the find bar is visible.
    pub open: bool,
    /// Search input contents.
    pub query: String,
    /// Whether all matches should be highlighted.
    pub highlight_all: bool,
    /// Whether matching should preserve case.
    pub match_case: bool,
    /// Whether matching should distinguish diacritic marks.
    pub match_diacritics: bool,
    /// All known matches in currently loaded text layers.
    pub matches: Vec<ViewerFindMatch>,
    /// Selected match index within `matches`.
    pub selected: Option<usize>,
}

impl Default for ViewerFindState {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            highlight_all: true,
            match_case: false,
            match_diacritics: false,
            matches: Vec::new(),
            selected: None,
        }
    }
}

impl ViewerFindState {
    /// Recomputes matches from loaded text layers.
    pub fn refresh_matches<'a>(
        &mut self,
        layers: impl Iterator<Item = (&'a u16, &'a PageTextLayer)>,
    ) {
        let previous = self.selected_match();
        self.matches =
            viewer_find_matches(layers, &self.query, self.match_case, self.match_diacritics);
        self.selected = if self.matches.is_empty() {
            None
        } else if let Some(previous) = previous {
            self.matches
                .iter()
                .position(|candidate| *candidate >= previous)
                .or(Some(0))
        } else {
            Some(0)
        };
    }

    /// Returns the currently selected match.
    pub fn selected_match(&self) -> Option<ViewerFindMatch> {
        self.selected
            .and_then(|index| self.matches.get(index).copied())
    }

    /// Selects the next match, wrapping at the end.
    pub fn select_next(&mut self) {
        self.select_relative(1);
    }

    /// Selects the previous match, wrapping at the beginning.
    pub fn select_previous(&mut self) {
        self.select_relative(-1);
    }

    fn select_relative(&mut self, delta: isize) {
        if self.matches.is_empty() {
            self.selected = None;
            return;
        }

        let current = self.selected.unwrap_or(0);
        let len = self.matches.len();
        self.selected = Some(if delta < 0 {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        });
    }
}

/// Finds all non-overlapping query matches in loaded page text layers.
pub fn viewer_find_matches<'a>(
    layers: impl Iterator<Item = (&'a u16, &'a PageTextLayer)>,
    query: &str,
    match_case: bool,
    match_diacritics: bool,
) -> Vec<ViewerFindMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let needle = normalize_find_text(query, match_case, match_diacritics);
    if needle.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    let mut layers = layers.collect::<Vec<_>>();
    layers.sort_by_key(|(page, _)| **page);

    for (page, layer) in layers {
        let mut haystack = String::new();
        let mut char_map = Vec::new();
        for (char_index, character) in layer.chars.iter().enumerate() {
            for normalized in character
                .text
                .chars()
                .flat_map(|character| normalize_find_char(character, match_case, match_diacritics))
            {
                haystack.push(normalized);
                char_map.push(char_index);
            }
        }

        let mut offset = 0;
        while let Some(relative) = haystack[offset..].find(&needle) {
            let start_byte = offset + relative;
            let end_byte = start_byte + needle.len();
            let start_normalized = haystack[..start_byte].chars().count();
            let end_normalized = haystack[..end_byte].chars().count();

            if let (Some(start), Some(end)) = (
                char_map.get(start_normalized).copied(),
                char_map.get(end_normalized.saturating_sub(1)).copied(),
            ) {
                matches.push(ViewerFindMatch {
                    page: *page,
                    start,
                    end: end.saturating_add(1),
                });
            }

            offset = end_byte;
        }
    }

    matches
}

fn normalize_find_text(text: &str, match_case: bool, match_diacritics: bool) -> String {
    text.chars()
        .flat_map(|character| normalize_find_char(character, match_case, match_diacritics))
        .collect()
}

fn normalize_find_char(character: char, match_case: bool, match_diacritics: bool) -> Vec<char> {
    let mut chars = if match_case {
        vec![character]
    } else {
        character.to_lowercase().collect()
    };

    if !match_diacritics {
        for character in &mut chars {
            *character = fold_latin_diacritic(*character);
        }
    }

    chars
}

fn fold_latin_diacritic(character: char) -> char {
    match character {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å'
        | 'ā' | 'ă' | 'ą' => 'a',
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' | 'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ð' | 'Ď' | 'Đ' | 'ð' | 'ď' | 'đ' => 'd',
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ'
        | 'ė' | 'ę' | 'ě' => 'e',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' | 'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĥ' | 'Ħ' | 'ĥ' | 'ħ' => 'h',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' | 'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī'
        | 'ĭ' | 'į' | 'ı' => 'i',
        'Ĵ' | 'ĵ' => 'j',
        'Ķ' | 'ķ' | 'ĸ' => 'k',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' | 'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' | 'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø'
        | 'ō' | 'ŏ' | 'ő' => 'o',
        'Ŕ' | 'Ŗ' | 'Ř' | 'ŕ' | 'ŗ' | 'ř' => 'r',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'ś' | 'ŝ' | 'ş' | 'š' | 'ſ' => 's',
        'Ţ' | 'Ť' | 'Ŧ' | 'ţ' | 'ť' | 'ŧ' => 't',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' | 'ù' | 'ú' | 'û' | 'ü' | 'ũ'
        | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ŵ' | 'ŵ' => 'w',
        'Ý' | 'Ŷ' | 'Ÿ' | 'ý' | 'ÿ' | 'ŷ' => 'y',
        'Ź' | 'Ż' | 'Ž' | 'ź' | 'ż' | 'ž' => 'z',
        _ => character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_folio_core::{PageTextChar, TextRect};

    #[test]
    fn viewer_find_matches_ignore_case_by_default() {
        let layer = text_layer(0, "Find find FIND");

        let matches = viewer_find_matches([(&0, &layer)].into_iter(), "find", false, false);

        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].start, 0);
        assert_eq!(matches[1].start, 5);
        assert_eq!(matches[2].start, 10);
    }

    #[test]
    fn viewer_find_matches_can_match_case() {
        let layer = text_layer(0, "Find find");

        let matches = viewer_find_matches([(&0, &layer)].into_iter(), "find", true, false);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, 5);
    }

    #[test]
    fn viewer_find_matches_can_match_diacritics() {
        let layer = text_layer(0, "cafe café");

        let folded = viewer_find_matches([(&0, &layer)].into_iter(), "cafe", false, false);
        let strict = viewer_find_matches([(&0, &layer)].into_iter(), "cafe", false, true);

        assert_eq!(folded.len(), 2);
        assert_eq!(strict.len(), 1);
    }

    fn text_layer(page: u16, text: &str) -> PageTextLayer {
        PageTextLayer {
            page,
            width_points: 100.0,
            height_points: 100.0,
            chars: text
                .chars()
                .enumerate()
                .map(|(index, character)| PageTextChar {
                    index,
                    text: character.to_string(),
                    bounds: TextRect {
                        x: index as f32 * 0.01,
                        y: 0.1,
                        width: 0.01,
                        height: 0.05,
                    },
                })
                .collect(),
        }
    }
}
