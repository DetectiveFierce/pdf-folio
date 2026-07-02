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
