//! Exact union coverage geometry for geological observation footprints.

use crate::spatial::{VoxelBounds, VoxelCoord};

/// Tracks the exact part of one voxel region not yet covered by geological bodies.
///
/// Coverage is represented as disjoint axis-aligned remainder boxes, so work scales with covering
/// geometry rather than with the number of voxels in the observed region. Runtime prospecting and
/// trusted-load replay share this helper to keep positive-area evidence semantics identical.
pub(super) struct VoxelCoverage {
    uncovered: Vec<VoxelBounds>,
}

impl VoxelCoverage {
    pub(super) fn new(region: VoxelBounds) -> Self {
        Self {
            uncovered: vec![region],
        }
    }

    pub(super) fn cover(&mut self, cover: VoxelBounds) {
        if self.uncovered.is_empty() {
            return;
        }
        self.uncovered = std::mem::take(&mut self.uncovered)
            .into_iter()
            .flat_map(|bounds| subtract_bounds(bounds, cover))
            .collect();
    }

    pub(super) fn is_complete(&self) -> bool {
        self.uncovered.is_empty()
    }
}

fn subtract_bounds(bounds: VoxelBounds, cover: VoxelBounds) -> Vec<VoxelBounds> {
    let Some(overlap) = bounds.intersection(cover) else {
        return vec![bounds];
    };
    let min = bounds.min();
    let max = bounds.max_exclusive();
    let overlap_min = overlap.min();
    let overlap_max = overlap.max_exclusive();
    let mut remainder = Vec::with_capacity(6);

    push_bounds(
        &mut remainder,
        VoxelCoord::new(min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_min.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_max.x(), min.y(), min.z()),
        VoxelCoord::new(max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_min.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_max.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), max.y(), max.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), min.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), overlap_min.z()),
    );
    push_bounds(
        &mut remainder,
        VoxelCoord::new(overlap_min.x(), overlap_min.y(), overlap_max.z()),
        VoxelCoord::new(overlap_max.x(), overlap_max.y(), max.z()),
    );
    remainder
}

fn push_bounds(remainder: &mut Vec<VoxelBounds>, min: VoxelCoord, max: VoxelCoord) {
    if min.x() >= max.x() || min.y() >= max.y() || min.z() >= max.z() {
        return;
    }
    remainder.push(
        VoxelBounds::new(min, max)
            .unwrap_or_else(|_| unreachable!("positive geological coverage remainder is valid")),
    );
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
