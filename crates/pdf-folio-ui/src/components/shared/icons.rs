//! # Built-in SVG icons
//!
//! Raw SVG byte constants under `components::shared::icons` for chrome that
//! cannot pull assets from disk at paint time. Stroke paths use a neutral
//! black fill; callers recolor via iced `svg::Style` theme colors.
//!
//! Consumed by library toolbar toggles, sidebar and viewer disclosure chevrons,
//! history undo/redo buttons, destructive trash actions, and the viewer toolbar
//! tool cluster (annotate, copy, find, visibility). Prefer these over ad-hoc
//! inline SVGs when the same glyph appears in more than one surface.
//!
//! Viewer-toolbar glyphs (`ANNOTATE_SVG`, `COPY_SVG`, `FIND_SVG`, `EYE_OFF_SVG`)
//! are sized for ~18px paint boxes; `FIND_SVG` uses a cropped 100×100 viewBox so
//! the stacked-document mark fills its box next to the eye icon.
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
/// Message square with plus — viewer toolbar “annotate selected text” control.
///
/// Lucide-style stroke-only 24×24 glyph (stroke-width 2, same language as the
/// chevrons/undo icons) so it stays crisp and balanced at ~18–20px. Plus sits
/// inside the bubble for a single cohesive mark rather than a floating badge.
/// All ink is black so iced `svg::Style` recoloring works.
pub(crate) const ANNOTATE_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/><path d="M12 7v6"/><path d="M9 10h6"/></svg>"##;
/// Overlapping pages — viewer toolbar “copy selected text” control.
pub(crate) const COPY_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>"##;
/// Stacked documents + magnifying glass — viewer toolbar find-in-document control.
///
/// Masked document stack with cutouts for the front sheet and glass. viewBox is
/// cropped to the ink bounds so the glyph fills its paint box and sits
/// optically next to the eye icon (not tiny in a padded 112×112 frame).
/// Stroke/fill ink is black for iced `svg::Style` recoloring.
pub(crate) const FIND_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="6 6 100 100" width="100" height="100">
  <defs>
    <mask id="find-back-mask">
      <rect x="6" y="6" width="100" height="100" fill="white"/>
      <rect x="20" y="20" width="68" height="84" rx="8" fill="black"/>
    </mask>
    <mask id="find-front-mask">
      <rect x="6" y="6" width="100" height="100" fill="white"/>
      <line x1="85" y1="89" x2="98" y2="102" stroke="black" stroke-width="16" stroke-linecap="round"/>
      <circle cx="74" cy="78" r="21" fill="black"/>
    </mask>
  </defs>
  <rect x="12" y="12" width="56" height="72" rx="6" fill="none" stroke="#000" stroke-width="8" mask="url(#find-back-mask)"/>
  <g mask="url(#find-front-mask)">
    <rect x="26" y="26" width="56" height="72" rx="6" fill="none" stroke="#000" stroke-width="8"/>
    <path d="M 38 40 L 68 40 M 38 52 L 68 52 M 38 64 L 68 64 M 38 76 L 68 76" stroke="#000" stroke-width="6" stroke-linecap="round"/>
  </g>
  <circle cx="74" cy="78" r="14" fill="none" stroke="#000" stroke-width="8"/>
  <line x1="85" y1="89" x2="98" y2="102" stroke="#000" stroke-width="10" stroke-linecap="round"/>
</svg>"##;
/// Eye with slash — viewer toolbar visibility menu (sidebar / comments).
pub(crate) const EYE_OFF_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49"/><path d="M14.084 14.158a3 3 0 0 1-4.242-4.242"/><path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-4.86"/><path d="m2 2 20 20"/></svg>"##;
