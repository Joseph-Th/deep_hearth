//! Non-persisted, bounded local lookup for structural element geometry.
//!
//! Ordinary posts and panels occupy a few cells. Index those exactly so
//! control-rate body collision and eye-ray targeting do not scan every built
//! element. Unusually broad valid prisms remain searchable without allocating
//! a voxel-sized index for their potentially enormous bounds.

use std::collections::{BTreeMap, BTreeSet};

use crate::spatial::{VoxelBounds, VoxelCoord};

use super::StructuralElementId;

const MAX_INDEXED_VOXELS_PER_ELEMENT: u128 = 512;

fn index_locally(bounds: VoxelBounds) -> bool {
    bounds
        .voxel_count()
        .is_some_and(|count| count <= MAX_INDEXED_VOXELS_PER_ELEMENT)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ElementSpatialIndex {
    by_voxel: BTreeMap<VoxelCoord, BTreeSet<StructuralElementId>>,
    /// Horizontal projections let thermal cover check only local members.
    by_column: BTreeMap<(i64, i64), BTreeSet<StructuralElementId>>,
    broad_elements: BTreeMap<StructuralElementId, VoxelBounds>,
}

impl ElementSpatialIndex {
    pub(super) const fn new() -> Self {
        Self {
            by_voxel: BTreeMap::new(),
            by_column: BTreeMap::new(),
            broad_elements: BTreeMap::new(),
        }
    }

    pub(super) fn insert(&mut self, id: StructuralElementId, bounds: VoxelBounds) {
        if !index_locally(bounds) {
            assert!(
                self.broad_elements.insert(id, bounds).is_none(),
                "duplicate broad structural index entry"
            );
            return;
        }
        for voxel in occupied_voxels(bounds) {
            assert!(
                self.by_voxel.entry(voxel).or_default().insert(id),
                "duplicate structural voxel index entry"
            );
        }
        for column in occupied_columns(bounds) {
            assert!(
                self.by_column.entry(column).or_default().insert(id),
                "duplicate structural column index entry"
            );
        }
    }

    pub(super) fn remove(&mut self, id: StructuralElementId, bounds: VoxelBounds) {
        if !index_locally(bounds) {
            assert_eq!(
                self.broad_elements.remove(&id),
                Some(bounds),
                "broad structural index entry missing or changed"
            );
            return;
        }
        for voxel in occupied_voxels(bounds) {
            let occupants = self
                .by_voxel
                .get_mut(&voxel)
                .expect("prevalidated structural voxel index entry disappeared");
            assert!(
                occupants.remove(&id),
                "structural voxel index identity missing"
            );
            if occupants.is_empty() {
                self.by_voxel.remove(&voxel);
            }
        }
        for column in occupied_columns(bounds) {
            let occupants = self
                .by_column
                .get_mut(&column)
                .expect("structural column index entry disappeared");
            assert!(
                occupants.remove(&id),
                "structural column index identity missing"
            );
            if occupants.is_empty() {
                self.by_column.remove(&column);
            }
        }
    }

    pub(super) fn candidates(
        &self,
        voxel: VoxelCoord,
    ) -> impl Iterator<Item = StructuralElementId> + '_ {
        self.by_voxel
            .get(&voxel)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
            .chain(
                self.broad_elements
                    .iter()
                    .filter(move |(_, bounds)| bounds.has_voxel(voxel))
                    .map(|(&id, _)| id),
            )
    }

    pub(super) fn column_candidates(
        &self,
        x: i64,
        z: i64,
    ) -> impl Iterator<Item = StructuralElementId> + '_ {
        self.by_column
            .get(&(x, z))
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
            .chain(
                self.broad_elements
                    .iter()
                    .filter(move |(_, bounds)| {
                        let min = bounds.min();
                        let max = bounds.max_exclusive();
                        (min.x()..max.x()).contains(&x) && (min.z()..max.z()).contains(&z)
                    })
                    .map(|(&id, _)| id),
            )
    }
}

fn occupied_columns(bounds: VoxelBounds) -> impl Iterator<Item = (i64, i64)> {
    let start = bounds.min();
    let end = bounds.max_exclusive();
    (start.z()..end.z()).flat_map(move |z| (start.x()..end.x()).map(move |x| (x, z)))
}

pub(super) fn occupied_voxels(bounds: VoxelBounds) -> impl Iterator<Item = VoxelCoord> {
    let start = bounds.min();
    let end = bounds.max_exclusive();
    (start.y()..end.y()).flat_map(move |y| {
        (start.z()..end.z())
            .flat_map(move |z| (start.x()..end.x()).map(move |x| VoxelCoord::new(x, y, z)))
    })
}

#[cfg(test)]
#[path = "spatial_index_tests.rs"]
mod tests;
