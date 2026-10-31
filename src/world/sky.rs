//! Direct outdoor skylight from calendar time and authoritative generated geometry.
//!
//! This is an unowned environmental light field. It consumes no finite fuel,
//! cannot be invoked in ungenerated authored opening space, and cannot pass
//! through a roof, a stone ceiling or an unknown column.

use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

/// Conservative direct daylight levels on the same 0..15 scale as local fire.
/// There is no indirect bounce or weather-driven cloud cover yet.
fn sky_brightness(registries: &Registries, at: SimulationTick) -> u8 {
    let calendar = registries.core().calendar();
    let day = calendar.date_at(at).day_tick();
    let phase = u128::from(day) * 4 / u128::from(calendar.ticks_per_day());
    match phase {
        0 => 9,
        1 => 14,
        2 => 6,
        _ => 0,
    }
}

/// Outdoor day/night applies to inhabitants only in actually generated land;
/// an interior roof does not make the outside clock advance to night.
pub(crate) fn is_generated_night(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> bool {
    state.world().has_generated_terrain_at(position) && sky_brightness(registries, at) == 0
}

#[must_use]
pub fn assess_generated_skylight(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> u8 {
    if !(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&position.y())
        || !state.world().has_generated_terrain_at(position)
    {
        return 0;
    }
    let light = sky_brightness(registries, state.tick());
    if light == 0 {
        return 0;
    }
    // Short circuit on any real overhead obstruction. The destination itself
    // also has to be transparent. Partial open sections remain traversable.
    for y in position.y()..WORLD_MAX_Y_EXCLUSIVE {
        let at = VoxelCoord::new(position.x(), y, position.z());
        if state.structures().blocks_full_cell(at) || state.world().tree_trunk_at(at).is_some() {
            return 0;
        }
        let Some(block) = state.world().block_at(at) else {
            return 0;
        };
        if registries
            .world()
            .block_for_state(block)
            .is_none_or(|definition| definition.is_opaque())
        {
            return 0;
        }
    }
    light
}

#[cfg(test)]
#[path = "sky_tests.rs"]
mod tests;
