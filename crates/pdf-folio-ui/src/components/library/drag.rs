//! # Library drag/drop pure helpers
//!
//! Constants, state structs, and geometry functions for manual reorder and
//! folder assignment drags. Free of iced widgets and `Db` access.
//!
//! Live drag state is stored on `app.library` (`LibraryDragState` /
//! `FolderDragState`); domain methods in `crate::library::actions` mutate it
//! using the hit-tests and reorder helpers defined here.
//!
//! Autoscroll uses edge-band velocity curves; folder drops require a dwell
//! before activation to avoid accidental nesting while reordering.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use iced::Point;
use iced::Rectangle;
use pdf_folio_core::{EntryId, Folder, FolderId, LibrarySortMode};

/// Interval between drag autoscroll ticks (milliseconds).
pub const LIBRARY_DRAG_AUTOSCROLL_TICK_MS: u64 = 16;
/// Distance from the viewport edge that starts autoscroll (logical px).
pub const LIBRARY_DRAG_AUTOSCROLL_EDGE_BAND: f32 = 96.0;
/// Peak autoscroll speed in pixels per second.
pub const LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED: f32 = 980.0;
/// Minimum non-zero autoscroll speed once the edge band is entered.
pub const LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED: f32 = 80.0;
/// Cap on integration timestep so a stalled frame cannot jump the scroll.
pub const LIBRARY_DRAG_AUTOSCROLL_MAX_DT: f32 = 1.0 / 20.0;
/// Pointer movement required before a press becomes an active drag.
pub const LIBRARY_DRAG_ACTIVATION_DISTANCE: f32 = 6.0;
/// Hover time before a folder becomes an active drop target.
pub const LIBRARY_FOLDER_DROP_DWELL_MS: u64 = 500;
/// Duration of the success flash after dropping onto a folder.
pub const LIBRARY_FOLDER_DROP_FLASH_MS: u64 = 900;

/// Current manual-reorder drag state for the library view.
#[derive(Debug, Clone)]
pub struct LibraryDragState {
    /// Entry being dragged.
    pub entry_id: EntryId,
    /// Entries moved by this drag, in their original visible order.
    pub entry_ids: Vec<EntryId>,
    /// Original zero-based index in the visible manual-order list.
    pub source_index: usize,
    /// Current insertion index after removing the dragged entry from the visible list.
    pub target_index: usize,
    /// Whether this drag moves multiple selected entries as a group.
    pub multi: bool,
    /// Active folder target for additive assignment.
    pub drop_target: Option<FolderId>,
    /// Whether the active drop target is the parent directory strip.
    pub parent_drop_target: bool,
    /// Folder currently hovered while waiting for dwell activation.
    pub pending_drop_target: Option<FolderId>,
    /// Time when the current folder hover began.
    pub pending_drop_started_at: Option<Instant>,
    /// Sidebar folders expanded by drag dwell and eligible to collapse on leave.
    pub expanded_during_drag: HashSet<FolderId>,
    /// Whether pointer movement has crossed the drag threshold.
    pub active: bool,
    /// Cursor position recorded when the press began.
    pub press_cursor: Option<Point>,
    /// Latest cursor position in window coordinates.
    pub cursor: Option<Point>,
    /// Last auto-scroll tick used for frame-rate independent scrolling.
    pub last_auto_scroll_tick: Option<Instant>,
}

impl LibraryDragState {
    /// Start an inactive entry drag at `source_index` (activates after movement threshold).
    pub fn new(
        entry_id: EntryId,
        entry_ids: Vec<EntryId>,
        source_index: usize,
        multi: bool,
    ) -> Self {
        Self {
            entry_id,
            entry_ids,
            source_index,
            target_index: source_index,
            multi,
            drop_target: None,
            parent_drop_target: false,
            pending_drop_target: None,
            pending_drop_started_at: None,
            expanded_during_drag: HashSet::new(),
            active: false,
            press_cursor: None,
            cursor: None,
            last_auto_scroll_tick: None,
        }
    }

    /// Track pointer motion; returns whether the drag has activated.
    pub fn update_cursor(&mut self, cursor: Point) -> bool {
        let press_cursor = *self.press_cursor.get_or_insert(cursor);
        self.cursor = Some(cursor);
        if distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE {
            self.active = true;
        }
        self.active
    }

    /// Update the dwell-pending folder hover target; returns whether it changed.
    pub fn set_pending_folder_target(&mut self, folder_id: Option<FolderId>, now: Instant) -> bool {
        if self.pending_drop_target == folder_id {
            return false;
        }

        self.pending_drop_target = folder_id.clone();
        self.pending_drop_started_at = folder_id.as_ref().map(|_| now);
        if self.drop_target != folder_id {
            self.drop_target = None;
        }
        true
    }

    /// Toggle parent-strip targeting; clears folder targets when enabled.
    pub fn set_parent_drop_target(&mut self, active: bool) -> bool {
        if self.parent_drop_target == active {
            return false;
        }

        self.parent_drop_target = active;
        if active {
            self.drop_target = None;
            self.pending_drop_target = None;
            self.pending_drop_started_at = None;
        }
        true
    }

    /// Folder id whose dwell completed and can become the active drop target.
    pub fn pending_target_ready(&self, now: Instant) -> Option<FolderId> {
        if !self.active || self.drop_target.is_some() {
            return None;
        }
        let folder_id = self.pending_drop_target.clone()?;
        let started_at = self.pending_drop_started_at?;
        folder_drop_target_ready(started_at, now).then_some(folder_id)
    }
}

/// Current drag state for moving a folder into another folder.
#[derive(Debug, Clone)]
pub struct FolderDragState {
    /// Folder being dragged.
    pub folder_id: FolderId,
    /// Active folder target for nesting.
    pub drop_target: Option<FolderId>,
    /// Whether the active drop target is the parent directory strip.
    pub parent_drop_target: bool,
    /// Folder currently hovered while waiting for dwell activation.
    pub pending_drop_target: Option<FolderId>,
    /// Time when the current folder hover began.
    pub pending_drop_started_at: Option<Instant>,
    /// Sidebar folders expanded by drag dwell and eligible to collapse on cancel.
    pub expanded_during_drag: HashSet<FolderId>,
    /// Whether pointer movement has crossed the drag threshold.
    pub active: bool,
    /// Cursor position recorded when the press began.
    pub press_cursor: Option<Point>,
    /// Latest cursor position in window coordinates.
    pub cursor: Option<Point>,
}

impl FolderDragState {
    /// Start an inactive folder drag for `folder_id`.
    pub fn new(folder_id: FolderId) -> Self {
        Self {
            folder_id,
            drop_target: None,
            parent_drop_target: false,
            pending_drop_target: None,
            pending_drop_started_at: None,
            expanded_during_drag: HashSet::new(),
            active: false,
            press_cursor: None,
            cursor: None,
        }
    }

    /// Track pointer motion; returns whether the drag has activated.
    pub fn update_cursor(&mut self, cursor: Point) -> bool {
        let press_cursor = *self.press_cursor.get_or_insert(cursor);
        self.cursor = Some(cursor);
        if distance_between(press_cursor, cursor) >= LIBRARY_DRAG_ACTIVATION_DISTANCE {
            self.active = true;
        }
        self.active
    }

    /// Update pending/active folder nest target; optionally activate immediately.
    pub fn set_drop_target(
        &mut self,
        target: Option<FolderId>,
        now: Instant,
        active_immediately: bool,
    ) -> bool {
        if self.pending_drop_target == target && self.drop_target == target {
            return false;
        }

        self.pending_drop_target = target.clone();
        self.pending_drop_started_at = target.as_ref().map(|_| now);
        self.drop_target = if active_immediately && self.active {
            target
        } else {
            None
        };
        true
    }

    /// Toggle parent-strip targeting; clears folder targets when enabled.
    pub fn set_parent_drop_target(&mut self, active: bool) -> bool {
        if self.parent_drop_target == active {
            return false;
        }

        self.parent_drop_target = active;
        if active {
            self.drop_target = None;
            self.pending_drop_target = None;
            self.pending_drop_started_at = None;
        }
        true
    }

    /// Folder id whose dwell completed and can become the active drop target.
    pub fn pending_target_ready(&self, now: Instant) -> Option<FolderId> {
        if !self.active || self.drop_target.is_some() {
            return None;
        }
        let folder_id = self.pending_drop_target.clone()?;
        let started_at = self.pending_drop_started_at?;
        folder_drop_target_ready(started_at, now).then_some(folder_id)
    }
}

/// Whether drag-to-reorder is allowed in the current sort mode.
pub fn can_drag_reorder_library(
    sort_mode: LibrarySortMode,
    search_query: &str,
    search_active: bool,
    tag_filter_active: bool,
    _folder_selected: bool,
) -> bool {
    sort_mode == LibrarySortMode::Manual
        && search_query.trim().is_empty()
        && !search_active
        && !tag_filter_active
}

/// Resolves the active folder drop target during a drag.
pub fn active_folder_drop_target<'a>(
    library_drag: Option<&'a LibraryDragState>,
    folder_drag: Option<&'a FolderDragState>,
) -> Option<&'a FolderId> {
    library_drag
        .and_then(|drag| drag.drop_target.as_ref())
        .or_else(|| folder_drag.and_then(|drag| drag.drop_target.as_ref()))
}

/// Auto-scroll velocity when dragging near viewport edges.
pub fn drag_auto_scroll_velocity(cursor_y: f32, viewport_y: f32, viewport_height: f32) -> f32 {
    if viewport_height <= 1.0 {
        return 0.0;
    }

    let viewport_bottom = viewport_y + viewport_height;
    let band = LIBRARY_DRAG_AUTOSCROLL_EDGE_BAND.min(viewport_height / 2.0);
    if band <= 0.0 {
        return 0.0;
    }

    let strength = if cursor_y < viewport_y + band {
        -((viewport_y + band - cursor_y) / band).clamp(0.0, 1.0)
    } else if cursor_y > viewport_bottom - band {
        ((cursor_y - (viewport_bottom - band)) / band).clamp(0.0, 1.0)
    } else {
        0.0
    };

    if strength == 0.0 {
        return 0.0;
    }

    let eased = strength.abs().powi(2);
    let speed = LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED
        + (LIBRARY_DRAG_AUTOSCROLL_MAX_SPEED - LIBRARY_DRAG_AUTOSCROLL_MIN_SPEED) * eased;
    strength.signum() * speed
}

/// Euclidean distance between two points.
pub fn distance_between(a: Point, b: Point) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// Return a new sibling order with `dragged_folder` moved before `target_folder`.
pub fn reorder_folder_ids_before_target(
    folders: &[FolderId],
    dragged_folder: &FolderId,
    target_folder: &FolderId,
) -> Option<Vec<FolderId>> {
    if dragged_folder == target_folder {
        return None;
    }

    let dragged_index = folders
        .iter()
        .position(|folder_id| folder_id == dragged_folder)?;
    let target_index = folders
        .iter()
        .position(|folder_id| folder_id == target_folder)?;
    let mut next_order = folders.to_vec();
    let dragged = next_order.remove(dragged_index);
    let target_index = next_order
        .iter()
        .position(|folder_id| folder_id == target_folder)
        .unwrap_or(target_index.min(next_order.len()));
    next_order.insert(target_index, dragged);
    Some(next_order)
}

/// Whether the folder hover dwell has elapsed and the drop target may activate.
pub fn folder_drop_target_ready(started_at: Instant, now: Instant) -> bool {
    now.checked_duration_since(started_at)
        .is_some_and(|elapsed| elapsed >= Duration::from_millis(LIBRARY_FOLDER_DROP_DWELL_MS))
}

/// Whether `folder_id` is within the post-drop flash window at `now`.
pub fn folder_drop_flash_active_at(
    folder_id: &FolderId,
    flash: Option<(&FolderId, Instant)>,
    now: Instant,
) -> bool {
    flash.is_some_and(|(flashed_folder_id, started_at)| {
        flashed_folder_id == folder_id
            && now.saturating_duration_since(started_at)
                < Duration::from_millis(LIBRARY_FOLDER_DROP_FLASH_MS)
    })
}

/// Whether a folder can be moved into a candidate parent.
pub fn folder_can_move_into(
    folders: &[Folder],
    folder_id: &FolderId,
    target_id: &FolderId,
) -> bool {
    if folder_id == target_id {
        return false;
    }

    let mut current = Some(target_id);
    while let Some(id) = current {
        if id == folder_id {
            return false;
        }
        current = folders
            .iter()
            .find(|folder| &folder.id == id)
            .and_then(|folder| folder.parent_id.as_ref());
    }

    folders.iter().any(|folder| &folder.id == target_id)
}

/// Hit-tests folder cards under the cursor.
pub fn folder_card_target_at_cursor(
    cursor: Point,
    folders: &[Folder],
    dragged_folder_id: &FolderId,
    viewport_x: f32,
    viewport_y: f32,
    scroll_offset: f32,
    card_width: f32,
    row_height: f32,
    column_gap: f32,
    row_gap: f32,
    per_row: usize,
) -> Option<FolderId> {
    if folders.is_empty() || per_row == 0 {
        return None;
    }

    let content_x = cursor.x - viewport_x;
    let content_y = cursor.y - viewport_y + scroll_offset;
    if content_x < 0.0 || content_y < 0.0 {
        return None;
    }

    let column_pitch = card_width + column_gap;
    let row_pitch = row_height + row_gap;
    if column_pitch <= 0.0 || row_pitch <= 0.0 {
        return None;
    }

    let column = (content_x / column_pitch).floor() as usize;
    let row = (content_y / row_pitch).floor() as usize;
    if column >= per_row {
        return None;
    }

    let x_in_cell = content_x - column as f32 * column_pitch;
    let y_in_cell = content_y - row as f32 * row_pitch;
    if x_in_cell > card_width || y_in_cell > row_height {
        return None;
    }

    let index = row.saturating_mul(per_row).saturating_add(column);
    folders
        .get(index)
        .filter(|folder| &folder.id != dragged_folder_id)
        .map(|folder| folder.id.clone())
}

/// First folder whose bounds contain the cursor among precomputed targets.
pub fn folder_drop_target_at_cursor(
    cursor: Point,
    targets: &[(FolderId, Rectangle)],
) -> Option<FolderId> {
    targets
        .iter()
        .find(|(_, bounds)| {
            cursor.x >= bounds.x
                && cursor.x <= bounds.x + bounds.width
                && cursor.y >= bounds.y
                && cursor.y <= bounds.y + bounds.height
        })
        .map(|(folder_id, _)| folder_id.clone())
}

/// Whether the cursor is over the parent-directory drop strip geometry.
pub fn parent_directory_target_at_cursor(
    cursor: Point,
    viewport_x: f32,
    viewport_y: f32,
    scroll_offset: f32,
    width: f32,
    height: f32,
) -> bool {
    if width <= 0.0 || height <= 0.0 {
        return false;
    }

    let content_x = cursor.x - viewport_x;
    let content_y = cursor.y - viewport_y + scroll_offset;
    content_x >= 0.0 && content_x <= width && content_y >= 0.0 && content_y <= height
}
