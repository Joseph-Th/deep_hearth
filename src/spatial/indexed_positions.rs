//! Bounded traversal of sparse voxel-keyed B-trees without scanning excluded rows.

use std::collections::BTreeMap;

use super::VoxelCoord;

/// Visits an inclusive axis-aligned box in the map's original X/Y/Z order.
///
/// The endpoints of a lexicographic range alone do not restrict its inner
/// axes. Seek past each excluded run instead of visiting every distant
/// position in it. Empty coordinate space costs no iteration, and a dense
/// matching run still uses the existing B-tree iterator without reseeking.
/// Inclusive endpoints also represent the last i64 voxel on every axis.
pub(crate) fn indexed_positions_in_box<T>(
    index: &BTreeMap<VoxelCoord, T>,
    min: VoxelCoord,
    max: VoxelCoord,
) -> impl Iterator<Item = (&VoxelCoord, &T)> {
    assert!(min.x() <= max.x() && min.y() <= max.y() && min.z() <= max.z());
    let mut entries = index.range(min..=max);
    std::iter::from_fn(move || {
        loop {
            let (position, value) = entries.next()?;
            let next_x = || {
                (position.x() < max.x())
                    .then(|| VoxelCoord::new(position.x() + 1, min.y(), min.z()))
            };
            let next = if position.y() < min.y() {
                VoxelCoord::new(position.x(), min.y(), min.z())
            } else if position.y() > max.y() {
                next_x()?
            } else if position.z() < min.z() {
                VoxelCoord::new(position.x(), position.y(), min.z())
            } else if position.z() > max.z() {
                if position.y() < max.y() {
                    VoxelCoord::new(position.x(), position.y() + 1, min.z())
                } else {
                    next_x()?
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
