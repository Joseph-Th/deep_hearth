//! Overhead refuge assessment from generated terrain and active physical structures.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{NANOMETERS_PER_VOXEL, VoxelCoord};
use crate::world::{ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

use super::{body_height, horizontal_voxel_bounds, length_nanometers};

/// Actor-safe local assessment of opaque world cover over the current body footprint.
///
/// This is intentionally narrower than shelter. `Covered` means every horizontal cell occupied by
/// the player's current stance has an opaque generated block or active full-footprint roof
/// somewhere between the body top and the generated world's sky boundary.
/// Missing world truth never grants protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerOverheadCoverAssessment {
    Open,
    Covered,
    WorldUnavailable,
}

/// The continuous body's horizontal footprint and lowest viable overhead roof level.
///
/// A single one-cell panel can cover the whole player only if its column spans
/// the complete horizontal footprint. This projection is read-only and makes
/// no claim that a proposed panel has sufficient structural support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerOverheadCoverFootprint {
    min_x: i64,
    max_x: i64,
    min_z: i64,
    max_z: i64,
    minimum_y: i64,
}

impl PlayerOverheadCoverFootprint {
    #[must_use]
    pub const fn minimum_y(self) -> i64 {
        self.minimum_y
    }

    #[must_use]
    pub const fn intersects_column(self, column: VoxelCoord) -> bool {
        column.x() >= self.min_x
            && column.x() <= self.max_x
            && column.z() >= self.min_z
            && column.z() <= self.max_z
    }

    #[must_use]
    pub const fn fits_single_column(self) -> bool {
        self.min_x == self.max_x && self.min_z == self.max_z
    }
}

/// Returns the same physical footprint and head clearance used to assess
/// actual overhead cover, including crouched stance and cell-edge overlap.
#[must_use]
pub fn project_player_overhead_cover_footprint(
    registries: &Registries,
    state: &AppState,
) -> Option<PlayerOverheadCoverFootprint> {
    state.player_observation_position()?;
    let player = state.logistics().player().copied()?;
    let locomotion = registries.survival().physiology().locomotion();
    let (min_x, max_x, min_z, max_z) = horizontal_voxel_bounds(player.body(), locomotion)?;
    let height = length_nanometers(body_height(locomotion, player.body().stance()))?;
    let top_exclusive = player
        .body()
        .foot_position()
        .y_nanometers()
        .checked_add(height)?;
    let mut minimum_y = top_exclusive.div_euclid(NANOMETERS_PER_VOXEL);
    if top_exclusive.rem_euclid(NANOMETERS_PER_VOXEL) != 0 {
        minimum_y = minimum_y.checked_add(1)?;
    }
    if !(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&minimum_y) {
        return None;
    }
    Some(PlayerOverheadCoverFootprint {
        min_x,
        max_x,
        min_z,
        max_z,
        minimum_y,
    })
}

#[must_use]
pub fn assess_player_overhead_cover(
    registries: &Registries,
    state: &AppState,
) -> Option<PlayerOverheadCoverAssessment> {
    // Physical cover remains world-owned, but its player-facing observation
    // must not use a dead actor's persisted body position.
    state.player_observation_position()?;
    let Some(footprint) = project_player_overhead_cover_footprint(registries, state) else {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    };
    let mut unavailable = false;
    for z in footprint.min_z..=footprint.max_z {
        for x in footprint.min_x..=footprint.max_x {
            let voxel = VoxelCoord::new(x, footprint.minimum_y, z);
            // A generated chunk owns its entire vertical column. Reuse the
            // world's section-aware sky query rather than fetching and
            // decoding each of the hundreds of cells above the player's head.
            let Some(chunk) = state.world().get_chunk(ChunkCoord::containing(voxel)) else {
                unavailable = true;
                continue;
            };
            let structural_cover = state.structures().has_active_overhead_cover(
                x,
                z,
                footprint.minimum_y,
                WORLD_MAX_Y_EXCLUSIVE,
            );
            let terrain_cover =
                !structural_cover && chunk.has_opaque_block_at_or_above(registries.world(), voxel);
            if !structural_cover && !terrain_cover {
                return Some(PlayerOverheadCoverAssessment::Open);
            }
        }
    }
    Some(if unavailable {
        PlayerOverheadCoverAssessment::WorldUnavailable
    } else {
        PlayerOverheadCoverAssessment::Covered
    })
}
