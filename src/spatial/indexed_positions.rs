//! Bounded traversal of sparse voxel-keyed B-trees without scanning excluded rows.

use std::collections::BTreeMap;

use super::VoxelCoord;

/// A voxel key whose `Ord` matches the lexicographic order of its components.
/// Different indexes may prioritize different physical axes.
pub(crate) trait OrderedVoxelKey: Copy + Ord {
    fn components(self) -> [i64; 3];
    fn from_components(components: [i64; 3]) -> Self;
}

impl OrderedVoxelKey for VoxelCoord {
    fn components(self) -> [i64; 3] {
        [self.x(), self.y(), self.z()]
    }

    fn from_components([x, y, z]: [i64; 3]) -> Self {
        Self::new(x, y, z)
    }
}

/// Visits an inclusive axis-aligned box in the map's original key order.
///
/// The endpoints of a lexicographic range alone do not restrict its inner
/// axes. Seek past each excluded run instead of visiting every distant
/// position in it. Empty coordinate space costs no iteration, and a dense
/// matching run still uses the existing B-tree iterator without reseeking.
/// Inclusive endpoints also represent the last i64 voxel on every axis.
pub(crate) fn indexed_positions_in_box<K: OrderedVoxelKey, T>(
    index: &BTreeMap<K, T>,
    min: K,
    max: K,
) -> impl Iterator<Item = (&K, &T)> {
    let [a0, b0, c0] = min.components();
    let [a1, b1, c1] = max.components();
    assert!(a0 <= a1 && b0 <= b1 && c0 <= c1);
    let mut entries = index.range(min..=max);
    std::iter::from_fn(move || {
        loop {
            let (position, value) = entries.next()?;
            let [a, b, c] = position.components();
            let next_first = || (a < a1).then(|| K::from_components([a + 1, b0, c0]));
            let next = if b < b0 {
                K::from_components([a, b0, c0])
            } else if b > b1 {
                next_first()?
            } else if c < c0 {
                K::from_components([a, b, c0])
            } else if c > c1 {
                if b < b1 {
                    K::from_components([a, b + 1, c0])
                } else {
                    next_first()?
                }
            } else {
                return Some((position, value));
            };
            entries = index.range(next..=max);
        }
    })
}

#[cfg(test)]
#[path = "indexed_positions_tests.rs"]
mod tests;
