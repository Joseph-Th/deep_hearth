//! Padded block neighborhood and presentation skylight for one section.
//!
//! Skylight here is a renderer-neutral visual field: each open-sky cell has level 15 and light
//! spreads through non-opaque cells, losing two levels per face step. It reaches at most seven
//! steps, so an eight-cell margin contains every path that can light a sampled cell; meshing a
//! section therefore never needs global light state. Ungenerated columns read as open air.

use std::collections::VecDeque;

use crate::spatial::VoxelCoord;
use crate::world::{BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldState};

use super::StateLooks;

pub(super) const SKY_LEVEL_MAX: u8 = 15;
const SKY_LEVEL_STEP: u8 = 2;
/// One cell for face/occlusion samples plus seven skylight steps.
pub(super) const MARGIN: i64 = 8;
pub(super) const SIDE: usize = 16;
const PADDED: usize = SIDE + 2 * MARGIN as usize;
const PADDED_I64: i64 = PADDED as i64;

pub(super) struct PaddedVolume {
    states: Vec<BlockStateId>,
    opaque: Vec<bool>,
    sky: Vec<u8>,
}

const fn index(x: usize, y: usize, z: usize) -> usize {
    (y * PADDED + z) * PADDED + x
}

/// Converts a section-local coordinate (which may reach into the margin) to a padded index.
fn padded(local: [i64; 3]) -> Option<usize> {
    let [x, y, z] = local.map(|value| value + MARGIN);
    let inside = |value: i64| (0..PADDED_I64).contains(&value);
    (inside(x) && inside(y) && inside(z)).then(|| index(x as usize, y as usize, z as usize))
}

impl PaddedVolume {
    /// Gathers states around the section whose minimum world cell is `section_min`.
    pub(super) fn gather(
        world: &WorldState,
        looks: &StateLooks<'_>,
        section_min: [i64; 3],
    ) -> Self {
        let cell_count = PADDED * PADDED * PADDED;
        let mut volume = Self {
            states: vec![BlockStateId::AIR; cell_count],
            opaque: vec![false; cell_count],
            sky: vec![0; cell_count],
        };
        let [min_x, min_y, min_z] = section_min;
        let min_y = min_y - MARGIN;
        for z in 0..PADDED {
            for x in 0..PADDED {
                // The generated chunk itself can touch either i64 boundary.
                // Unrepresentable margin columns are outside the world, like
                // ungenerated neighbors, not wrapped copies of distant terrain.
                let column = min_x
                    .checked_add(x as i64 - MARGIN)
                    .zip(min_z.checked_add(z as i64 - MARGIN));
                volume.gather_column(world, looks, [x, z], column, min_y);
            }
        }
        volume.propagate_sky();
        volume
    }

    fn gather_column(
        &mut self,
        world: &WorldState,
        looks: &StateLooks<'_>,
        [x, z]: [usize; 2],
        column: Option<(i64, i64)>,
        min_y: i64,
    ) {
        let (world_x, world_z) = column.unwrap_or((0, 0));
        let chunk = column.and_then(|_| {
            world.get_chunk(ChunkCoord::containing(VoxelCoord::new(world_x, 0, world_z)))
        });
        let above = VoxelCoord::new(world_x, min_y + PADDED_I64, world_z);
        let mut open_sky =
            chunk.is_none_or(|chunk| !chunk.has_opaque_block_at_or_above(looks.registry(), above));
        for y in (0..PADDED).rev() {
            let world_y = min_y + y as i64;
            let cell = index(x, y, z);
            if world_y < WORLD_MIN_Y {
                // Below the world floor is unbroken ground: it hides faces and blocks light.
                self.opaque[cell] = true;
                open_sky = false;
                continue;
            }
            let state = match chunk {
                Some(chunk) if world_y < WORLD_MAX_Y_EXCLUSIVE => {
                    chunk.block_at(VoxelCoord::new(world_x, world_y, world_z))
                }
                _ => BlockStateId::AIR,
            };
            let opaque = looks.is_opaque(state);
            open_sky &= !opaque;
            self.states[cell] = state;
            self.opaque[cell] = opaque;
            if open_sky {
                self.sky[cell] = SKY_LEVEL_MAX;
            }
        }
    }

    fn propagate_sky(&mut self) {
        let mut queue: VecDeque<usize> = (0..self.sky.len())
            .filter(|&cell| self.sky[cell] == SKY_LEVEL_MAX)
            .collect();
        while let Some(cell) = queue.pop_front() {
            let level = self.sky[cell];
            if level <= SKY_LEVEL_STEP {
                continue;
            }
            let next = level - SKY_LEVEL_STEP;
            for neighbor in neighbors(cell) {
                if !self.opaque[neighbor] && self.sky[neighbor] < next {
                    self.sky[neighbor] = next;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    /// State at a section-local coordinate; the margin reaches eight cells beyond the section.
    pub(super) fn state(&self, local: [i64; 3]) -> BlockStateId {
        padded(local).map_or(BlockStateId::AIR, |cell| self.states[cell])
    }

    pub(super) fn is_opaque(&self, local: [i64; 3]) -> bool {
        padded(local).is_some_and(|cell| self.opaque[cell])
    }

    pub(super) fn sky(&self, local: [i64; 3]) -> u8 {
        padded(local).map_or(0, |cell| self.sky[cell])
    }
}

fn neighbors(cell: usize) -> impl Iterator<Item = usize> {
    let x = cell % PADDED;
    let z = (cell / PADDED) % PADDED;
    let y = cell / (PADDED * PADDED);
    let last = PADDED - 1;
    [
        (x > 0).then(|| cell - 1),
        (x < last).then(|| cell + 1),
        (z > 0).then(|| cell - PADDED),
        (z < last).then(|| cell + PADDED),
        (y > 0).then(|| cell - PADDED * PADDED),
        (y < last).then(|| cell + PADDED * PADDED),
    ]
    .into_iter()
    .flatten()
}
