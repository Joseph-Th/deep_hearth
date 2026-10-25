//! Overhead refuge assessment from generated terrain and active physical structures.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{NANOMETERS_PER_VOXEL, VoxelCoord};
use crate::structural::StructuralLifecycle;
use crate::world::WORLD_MAX_Y_EXCLUSIVE;

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
    if cover_y >= WORLD_MAX_Y_EXCLUSIVE {
        return Some(PlayerOverheadCoverAssessment::WorldUnavailable);
    }
    let mut unavailable = false;
    for z in min_z..=max_z {
        for x in min_x..=max_x {
            let voxel = VoxelCoord::new(x, cover_y, z);
            if state.world().block_at(voxel).is_none() {
                unavailable = true;
                continue;
            }
            let structural_cover = state.structures().elements().any(|record| {
                record.lifecycle() == StructuralLifecycle::Active
                    && record.geometry().covers_horizontal_footprint()
                    && record.bounds().min().x() <= x
                    && record.bounds().max_exclusive().x() > x
                    && record.bounds().min().z() <= z
                    && record.bounds().max_exclusive().z() > z
                    // Roofs entirely above generated world space cannot intercept
                    // exposure along the body's in-world column.
                    && record.bounds().min().y() < WORLD_MAX_Y_EXCLUSIVE
                    && record.bounds().max_exclusive().y() > cover_y
            });
            let terrain_cover = !structural_cover
                && (cover_y..WORLD_MAX_Y_EXCLUSIVE).any(|y| {
                    let block_state = state
                        .world()
                        .block_at(VoxelCoord::new(x, y, z))
                        .unwrap_or_else(|| {
                            unreachable!("generated chunk owns every in-range vertical cell")
                        });
                    registries
                        .world()
                        .block_for_state(block_state)
                        .unwrap_or_else(|| {
                            panic!("runtime invariant broken: world block state disappeared")
                        })
                        .is_opaque()
                });
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
