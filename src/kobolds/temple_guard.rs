//! Heavy temple wardens physically challenge intruders on a short, supported hall lane.
//!
//! The architect's original post is implicit in generated geometry. Only a
//! resident actually displaced by local player contact needs persistent state.
//! Distant temples are neither scanned nor advanced, and there is no combat.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::survival::Vitality;
use crate::world::{
    BlockCollisionShape, BlockStateId, ChunkCoord, WORLD_MIN_Y, temple_warden_route_at,
};

use super::{SCOUT_STEP_TICKS, visibility::scout_visible_from};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TempleWardenRecord {
    pub(super) chunk: ChunkCoord,
    pub(super) step: u8,
}

fn clear_body_cell(state: &AppState, position: VoxelCoord) -> bool {
    state.world().block_at(position) == Some(BlockStateId::AIR)
        && state.world().block_at(VoxelCoord::new(
            position.x(),
            position.y() + 1,
            position.z(),
        )) == Some(BlockStateId::AIR)
}

fn clear_standing_cell(registries: &Registries, state: &AppState, position: VoxelCoord) -> bool {
    position.y() > WORLD_MIN_Y
        && clear_body_cell(state, position)
        && state
            .world()
            .block_at(VoxelCoord::new(
                position.x(),
                position.y() - 1,
                position.z(),
            ))
            .and_then(|block| registries.world().block_for_state(block))
            .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

/// Advance one resident over at most one real face-adjacent hall cell on the
/// existing sparse scout clock. A living, visible intruder draws the guardian
/// forward; otherwise it retreats one cell toward its ancestral post.
pub(super) fn apply_temple_warden_tick(
    registries: &Registries,
    state: &mut AppState,
    at: SimulationTick,
) {
    if !at.value().is_multiple_of(SCOUT_STEP_TICKS) {
        return;
    }
    let Some(player) = state.logistics().player().map(|actor| actor.position()) else {
        return;
    };
    // Ordinary homebound defenders remain implicit. Only an actually displaced
    // defender continues its return after the player leaves its chunk. Iterating
    // the sparse saved cursors does not discover or animate dormant remote sites.
    let mut active = state
        .kobold_state()
        .temple_wardens
        .iter()
        .filter_map(|record| {
            let x = record.chunk.world_min_x()?.checked_add(8)?;
            let z = record.chunk.world_min_z()?.checked_add(8)?;
            temple_warden_route_at(registries, state, VoxelCoord::new(x, -19, z))
                .map(|route| (record.chunk, route))
        })
        .collect::<Vec<_>>();
    if let Some(route) = temple_warden_route_at(registries, state, player) {
        let chunk = ChunkCoord::containing(route[0]);
        if !active.iter().any(|(present, _)| *present == chunk) {
            active.push((chunk, route));
        }
    }
    let alive = state
        .survival()
        .player()
        .is_some_and(|actor| actor.vitality() != Vitality::ZERO);
    for (chunk, route) in active {
        let step = state.kobold_state().temple_warden_step(chunk);
        let current = route[usize::from(step)];
        if !clear_standing_cell(registries, state, current) {
            continue;
        }
        let aware = alive
            && ChunkCoord::containing(player) == chunk
            && player.z() < current.z()
            && scout_visible_from(registries, state, current, player);
        let next_step = if aware && step < 2 {
            step + 1
        } else if !aware && step > 0 {
            step - 1
        } else {
            step
        };
        if next_step == step {
            continue;
        }
        let destination = route[usize::from(next_step)];
        if crate::logistics::player_body_occupies_voxel(registries, state, destination)
            || !clear_standing_cell(registries, state, destination)
        {
            continue;
        }
        let displaced = &mut state.kobold_state_mut().temple_wardens;
        match displaced.binary_search_by_key(&chunk, |record| record.chunk) {
            Ok(index) if next_step == 0 => {
                displaced.remove(index);
            }
            Ok(index) => displaced[index].step = next_step,
            Err(index) => displaced.insert(
                index,
                TempleWardenRecord {
                    chunk,
                    step: next_step,
                },
            ),
        }
    }
}

/// A saved displacement is only a possible step along one real generated
/// temple lane. Reject arbitrary invented guardians, duplicate keys, absent
/// columns, obstructed locations and out-of-range cursors on trusted load.
pub(super) fn validate_temple_wardens(
    registries: &Registries,
    state: &AppState,
) -> Result<(), &'static str> {
    let mut previous = None;
    for &TempleWardenRecord { chunk, step } in &state.kobold_state().temple_wardens {
        if previous.is_some_and(|before| before >= chunk) {
            return Err("kobold temple warden records must have unique sorted homes");
        }
        previous = Some(chunk);
        if !(1..=2).contains(&step) {
            return Err("kobold temple warden has an invalid off-post cursor");
        }
        let Some(x) = chunk.world_min_x().and_then(|min| min.checked_add(8)) else {
            return Err("kobold temple warden has an overflowing home chunk");
        };
        let Some(z) = chunk.world_min_z().and_then(|min| min.checked_add(8)) else {
            return Err("kobold temple warden has an overflowing home chunk");
        };
        let Some(route) = temple_warden_route_at(registries, state, VoxelCoord::new(x, -19, z))
        else {
            return Err("kobold temple warden lacks an authored generated sanctuary");
        };
        // A player may mine away the supporting floor *after* the warden
        // occupies this cell. Preserve that stranded location across saves;
        // movement stays suspended until a real supported step is available.
        if !clear_body_cell(state, route[usize::from(step)]) {
            return Err("kobold temple warden occupies obstructed or absent world geometry");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "temple_guard_tests.rs"]
mod tests;
