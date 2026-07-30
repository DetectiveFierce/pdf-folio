//! In-memory LRU cache for rendered PDF page tiles.
//!
//! The UI viewer renders pages at a target pixel width via
//! [`super::document::PdfDoc::render_page`] and stores the raw RGBA bytes here
//! so scroll/pan does not re-render every frame. A tile is uniquely identified
//! by page index and render width ([`TileKey`]); changing zoom invalidates
//! keys with a different `width_px`.
//!
//! [`TileCache`] is cheap to clone (`Arc` interior) and safe to share across
//! the iced task pool. Capacity is measured in whole page tiles, not bytes.
//!
//! # See also
//!
//! - [`super::document::RenderedPage`] for the bitmap layout of cached data.
//! - [`super::document::PdfDoc`] for the actual Pdfium render path.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use lru::LruCache;

/// Cache key for a single rendered PDF page at a given pixel width.
///
/// Two renders of the same page at different widths are distinct entries so
/// the viewer can keep low-res previews while a higher-res tile loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    /// Zero-based page index within the open document.
    pub page: u16,
    /// Target render width in physical pixels (matches `render_page` width).
    pub width_px: u16,
}

/// Thread-safe LRU cache mapping [`TileKey`] → shared RGBA8 byte buffers.
///
/// Inserted buffers are wrapped in [`Arc`] so multiple widgets can hold the
/// same tile without copying pixel data. Poisoned mutexes are recovered so a
/// panicking reader does not permanently brick the cache.
#[derive(Debug, Clone)]
pub struct TileCache {
    inner: Arc<Mutex<LruCache<TileKey, Arc<Vec<u8>>>>>,
}

impl TileCache {
    /// Creates a cache that retains at most `capacity` page tiles.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(capacity))),
        }
    }

    /// Creates a cache with the default capacity of 64 rendered pages.
    pub fn with_default_capacity() -> Self {
        let capacity = NonZeroUsize::new(64).unwrap_or(NonZeroUsize::MIN);
        Self::new(capacity)
    }

    /// Inserts RGBA tile bytes, evicting the least-recently-used tile if full.
    pub fn insert(&self, key: TileKey, data: Vec<u8>) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.put(key, Arc::new(data));
    }

    /// Returns cached RGBA bytes for `key`, promoting it to most-recently used.
    pub fn get(&self, key: &TileKey) -> Option<Arc<Vec<u8>>> {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(key).cloned()
    }

    /// Resizes the cache; excess least-recently-used tiles are dropped immediately.
    pub fn set_capacity(&self, capacity: NonZeroUsize) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.resize(capacity);
    }

    /// Drops every cached tile (e.g. when the open document changes).
    pub fn clear(&self) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.clear();
    }

    /// Returns how many tiles are currently retained.
    pub fn len(&self) -> usize {
        let cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.len()
    }

    /// Returns `true` when no tiles are cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for TileCache {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}
