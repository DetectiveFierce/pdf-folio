//! Tile keys and in-memory LRU tile cache.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use lru::LruCache;

/// Cache key for a rendered PDF page tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    /// Zero-based page index.
    pub page: u16,
    /// Rendered page width in physical pixels.
    pub width_px: u16,
}

/// Thread-safe LRU cache for rendered RGBA page tiles.
#[derive(Debug, Clone)]
pub struct TileCache {
    inner: Arc<Mutex<LruCache<TileKey, Arc<Vec<u8>>>>>,
}

impl TileCache {
    /// Creates a cache with room for `capacity` rendered pages.
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

    /// Inserts RGBA tile bytes into the cache.
    pub fn insert(&self, key: TileKey, data: Vec<u8>) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.put(key, Arc::new(data));
    }

    /// Returns cached RGBA tile bytes and marks the tile as recently used.
    pub fn get(&self, key: &TileKey) -> Option<Arc<Vec<u8>>> {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.get(key).cloned()
    }

    /// Changes cache capacity and evicts least-recently-used entries if needed.
    pub fn set_capacity(&self, capacity: NonZeroUsize) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.resize(capacity);
    }

    /// Removes all cached tiles.
    pub fn clear(&self) {
        let mut cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.clear();
    }

    /// Returns the number of cached tiles.
    pub fn len(&self) -> usize {
        let cache = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.len()
    }

    /// Returns true when the cache contains no tiles.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for TileCache {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_tile() {
        let cache = TileCache::new(NonZeroUsize::new(2).unwrap_or(NonZeroUsize::MIN));
        let first = TileKey {
            page: 0,
            width_px: 800,
        };
        let second = TileKey {
            page: 1,
            width_px: 800,
        };
        let third = TileKey {
            page: 2,
            width_px: 800,
        };

        cache.insert(first, vec![1]);
        cache.insert(second, vec![2]);
        cache.insert(third, vec![3]);

        assert!(cache.get(&first).is_none());
        assert_eq!(
            cache.get(&second).as_deref().map(Vec::as_slice),
            Some(&[2][..])
        );
        assert_eq!(
            cache.get(&third).as_deref().map(Vec::as_slice),
            Some(&[3][..])
        );
    }
}
