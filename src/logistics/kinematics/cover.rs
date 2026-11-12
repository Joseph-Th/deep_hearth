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

#[must_use]
pub fn assess_player_overhead_cover(
    registries: &Registries,
    state: &AppState,
) -> Option<PlayerOverheadCoverAssessment> {
    // Physical cover remains world-owned, but its player-facing observation
    // must not use a dead actor's persisted body position.
    state.player_observation_position()?;
    let player = state.logistics().player().copied()?;
    let locomotion = registries.survival().physiology().locomotion();
    let Some((min_x, max_x, min_z, max_z)) = horizontal_voxel_bounds(player.body(), locomotion)
    else {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    };
    let Some(height) = length_nanometers(body_height(locomotion, player.body().stance())) else {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    };
    let Some(top_exclusive) = player
        .body()
        .foot_position()
        .y_nanometers()
        .checked_add(height)
    else {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    };
    let mut cover_y = top_exclusive.div_euclid(NANOMETERS_PER_VOXEL);
    if top_exclusive.rem_euclid(NANOMETERS_PER_VOXEL) != 0 {
        let Some(next) = cover_y.checked_add(1) else {
            return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
        };
        cover_y = next;
    }
    if !(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&cover_y) {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    }
    let mut unavailable = false;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let voxel = VoxelCoord::new(x, cover_y, z);
            // A generated chunk owns its entire vertical column. Reuse the
            // world's section-aware sky query rather than fetching and
            // decoding each of the hundreds of cells above the player's head.
            let Some(chunk) = state.world().get_chunk(ChunkCoord::containing(voxel)) else {
                unavailable = true;
                continue;
            };
            let structural_cover =
                state
                    .structures()
                    .has_active_overhead_cover(x, z, cover_y, WORLD_MAX_Y_EXCLUSIVE);
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
