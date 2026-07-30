//! # Built-in SVG icons
//!
//! Raw SVG byte constants under `components::shared::icons` for chrome that
//! cannot pull assets from disk at paint time. Stroke paths use a neutral
//! black fill; callers recolor via iced `svg::Style` theme colors.
//!
//! Consumed by library toolbar toggles, sidebar and viewer disclosure chevrons,
//! history undo/redo buttons, and destructive trash actions. Prefer these over
//! ad-hoc inline SVGs when the same glyph appears in more than one surface.
//!
//! Related: asset-backed overflow icons in [`super::menus`]; file-tree fold
//! chevrons remain local to `components::library::folder_tree`.

/// Left-pointing chevron for “back / previous / collapse left” controls
/// (viewer library-back affordances, page prev, sidebar hide).
pub(crate) const CHEVRON_LEFT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>"##;
/// Right-pointing chevron for “forward / next / expand right” controls
/// (page next, horizontal disclosure).
pub(crate) const CHEVRON_RIGHT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"##;
/// Up-pointing chevron for “previous match” on the viewer find bar and similar
/// vertical step-backward controls.
pub(crate) const CHEVRON_UP_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m18 15-6-6-6 6"/></svg>"##;
/// Down-pointing chevron for dropdown disclosure (zoom menu) and “next match”
/// on the viewer find bar.
pub(crate) const CHEVRON_DOWN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"##;
/// Curved undo arrow for library organization history toolbar actions.
pub(crate) const UNDO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 1 1 0 11H11"/></svg>"##;
/// Curved redo arrow for library organization history toolbar actions.
pub(crate) const REDO_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 1 0 0 11H13"/></svg>"##;
/// Four-cell grid glyph shown on the library layout toggle when switching
/// into grid mode (currently in list/compact view).
pub(crate) const GRID_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="7" height="7" x="3" y="3" rx="1"/><rect width="7" height="7" x="14" y="3" rx="1"/><rect width="7" height="7" x="14" y="14" rx="1"/><rect width="7" height="7" x="3" y="14" rx="1"/></svg>"##;
/// List-rows glyph shown on the library layout toggle when switching into
/// compact list mode (currently in grid view).
pub(crate) const LIST_LAYOUT_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" x2="21" y1="6" y2="6"/><line x1="8" x2="21" y1="12" y2="12"/><line x1="8" x2="21" y1="18" y2="18"/><line x1="3" x2="3.01" y1="6" y2="6"/><line x1="3" x2="3.01" y1="12" y2="12"/><line x1="3" x2="3.01" y1="18" y2="18"/></svg>"##;
/// Compact trash-can glyph for destructive toolbar and inspector delete
/// actions (move to trash, remove tag, etc.).
pub(crate) const TRASH_CAN_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v5"/><path d="M14 11v5"/></svg>"##;
