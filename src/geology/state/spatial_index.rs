//! Derived geological broadphase, including depleted bodies for historical evidence.

use std::collections::{BTreeMap, BTreeSet};

use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::world::ChunkCoord;

use super::GeologicalDepositId;

// Bound memory even when a decoded deposit covers an immense region.
const MAX_INDEXED_CHUNKS_PER_DEPOSIT: u128 = 64;

#[derive(Clone, Copy)]
struct ChunkSpan {
    first: ChunkCoord,
    last: ChunkCoord,
}

impl ChunkSpan {
    fn from_bounds(bounds: VoxelBounds) -> Self {
        let max = bounds.max_exclusive();
        Self {
            first: ChunkCoord::containing(bounds.min()),
            // Nonempty half-open bounds guarantee these subtractions.
            last: ChunkCoord::containing(VoxelCoord::new(max.x() - 1, max.y() - 1, max.z() - 1)),
        }
    }

    fn locally_indexable(self) -> bool {
        let x = u128::from(self.first.x().abs_diff(self.last.x())) + 1;
        let z = u128::from(self.first.z().abs_diff(self.last.z())) + 1;
        x.checked_mul(z)
            .is_some_and(|count| count <= MAX_INDEXED_CHUNKS_PER_DEPOSIT)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(super) struct DepositSpatialIndex {
    // Range both axes independently, including in a long narrow explored strip.
    chunks: BTreeMap<i64, BTreeMap<i64, BTreeSet<GeologicalDepositId>>>,
    broad: BTreeMap<GeologicalDepositId, VoxelBounds>,
}

impl DepositSpatialIndex {
    pub(super) const fn new() -> Self {
        Self {
            chunks: BTreeMap::new(),
            broad: BTreeMap::new(),
        }
    }

    pub(super) fn insert(&mut self, id: GeologicalDepositId, bounds: VoxelBounds) {
        let span = ChunkSpan::from_bounds(bounds);
        if !span.locally_indexable() {
            assert!(self.broad.insert(id, bounds).is_none());
            return;
        }
        for x in span.first.x()..=span.last.x() {
            let columns = self.chunks.entry(x).or_default();
            for z in span.first.z()..=span.last.z() {
                assert!(columns.entry(z).or_default().insert(id));
            }
        }
    }

    /// Unique candidates in persistent identity order. Range only existing
    /// chunks: even a huge query does not enumerate empty coordinates.
    pub(super) fn candidates(
        &self,
        min_x: i64,
        max_x: i64,
        min_z: i64,
        max_z: i64,
    ) -> BTreeSet<GeologicalDepositId> {
        let first = ChunkCoord::containing(VoxelCoord::new(min_x, 0, min_z));
        let last = ChunkCoord::containing(VoxelCoord::new(max_x, 0, max_z));
        let mut found = BTreeSet::new();
        for (_, columns) in self.chunks.range(first.x()..=last.x()) {
            for (_, ids) in columns.range(first.z()..=last.z()) {
                found.extend(ids.iter().copied());
            }
        }
        for (&id, bounds) in &self.broad {
            if intersects_columns(*bounds, min_x, max_x, min_z, max_z) {
                found.insert(id);
            }
        }
        found
    }
}

pub(super) fn intersects_columns(
    bounds: VoxelBounds,
    min_x: i64,
    max_x: i64,
    min_z: i64,
    max_z: i64,
) -> bool {
    let min = bounds.min();
    let max = bounds.max_exclusive();
    min.x() <= max_x && max.x() > min_x && min.z() <= max_z && max.z() > min_z
}
