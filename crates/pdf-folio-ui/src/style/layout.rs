//! Layout constants and small spacing primitives.

/// Default application window size.
pub const WINDOW_SIZE: [f32; 2] = [960.0, 1080.0];
/// Sidebar width for the viewer table of contents.
pub const VIEWER_SIDEBAR_WIDTH: f32 = 228.0;
/// Sidebar width for library tag filters.
pub const LIBRARY_SIDEBAR_WIDTH: f32 = 270.0;
/// Minimum width for the resizable library tag sidebar.
pub const LIBRARY_SIDEBAR_MIN_WIDTH: f32 = 210.0;
/// Maximum width for the resizable library tag sidebar.
pub const LIBRARY_SIDEBAR_MAX_WIDTH: f32 = 340.0;
/// Width of the draggable sidebar resize handle.
pub const SIDEBAR_RESIZE_HANDLE_WIDTH: f32 = 8.0;
/// Visible width of the sidebar resize handle when idle.
pub const SIDEBAR_RESIZE_HANDLE_VISUAL_WIDTH: f32 = 2.0;
/// Toolbar height used as a sizing token for future settings persistence.
pub const TOOLBAR_HEIGHT: f32 = 58.0;
/// Overscan rows rendered above and below the visible library window.
pub const LIBRARY_OVERSCAN_ROWS: usize = 4;
/// Minimum number of columns in the masonry library view.
pub const CARD_GRID_COLUMNS: usize = 2;
/// Fixed visual width for PDF cards in masonry mode.
pub const LIBRARY_GRID_CARD_WIDTH: f32 = 210.0;
/// Library card row height in grid mode.
pub const LIBRARY_GRID_ROW_HEIGHT: f32 = 376.0;
/// Folder card row height in grid mode; intentionally shorter than PDF cards.
pub const LIBRARY_FOLDER_GRID_ROW_HEIGHT: f32 = 86.0;
/// Library row height in list mode.
pub const LIBRARY_LIST_ROW_HEIGHT: f32 = 78.0;
/// Folder row height in list mode; intentionally shorter than PDF rows.
pub const LIBRARY_FOLDER_LIST_ROW_HEIGHT: f32 = 50.0;
/// Default thumbnail width in grid cards.
pub const LIBRARY_CARD_THUMBNAIL_WIDTH: f32 = 128.0;
/// Default thumbnail width in list rows.
pub const LIBRARY_ROW_THUMBNAIL_WIDTH: f32 = 46.0;
/// Width of the progress area in compact library rows.
pub const LIBRARY_ROW_PROGRESS_WIDTH: f32 = 120.0;
/// Logical pixels per wheel line.
pub const LINE_SCROLL_PIXELS: f32 = 48.0;
/// Default jump overlay input width.
pub const JUMP_INPUT_WIDTH: f32 = 90.0;
