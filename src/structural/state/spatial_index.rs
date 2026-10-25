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
    broad_elements: BTreeMap<StructuralElementId, VoxelBounds>,
}

impl ElementSpatialIndex {
    pub(super) const fn new() -> Self {
        Self {
            by_voxel: BTreeMap::new(),
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
