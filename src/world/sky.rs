//! Direct outdoor skylight from calendar time and authoritative generated geometry.
//!
//! This is an unowned environmental light field. It consumes no finite fuel,
//! cannot be invoked in ungenerated authored opening space, and cannot pass
//! through a roof, a stone ceiling or an unknown column.

use crate::core::state::AppState;
use crate::core::time::{Season, SimulationTick};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::{ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y};

const MAX_CANOPY_RADIUS: i64 = 3;
const CANOPY_VERTICAL_REACH_ABOVE_ROOT: i64 = 5;
const CANOPY_VERTICAL_REACH_BELOW_ROOT: i64 = 1;

/// Quarter-day direct-sky brightness. The same levels drive both visible
/// skylight and the daylight budget supporting a rooted plant's next crop.
const SKY_PHASE_BRIGHTNESS: [u8; 4] = [9, 14, 6, 0];
pub(super) const OPEN_DAYLIGHT_EXPOSURE_SUM: u64 = SKY_PHASE_BRIGHTNESS[0] as u64
    + SKY_PHASE_BRIGHTNESS[1] as u64
    + SKY_PHASE_BRIGHTNESS[2] as u64
    + SKY_PHASE_BRIGHTNESS[3] as u64;

/// Total represented daylight received over the four equal day phases.
/// This is an exposure potential, independent of the ripening front's hour.
pub(super) fn daylight_exposure_after_canopy(shade: u8) -> u64 {
    SKY_PHASE_BRIGHTNESS
        .into_iter()
        .map(|level| u64::from(level.saturating_sub(shade)))
        .sum()
}

/// The crown core extends to neighboring cells; farther rings receive
/// progressively more light without undoing the mature tree's proven
/// dusk concealment immediately beside the trunk.
fn crown_shade_at_distance(radius: u8, horizontal: u64) -> u8 {
    if radius == 0 || horizontal > u64::from(radius) {
        return 0;
    }
    u8::try_from((u64::from(radius) * 2).saturating_sub(horizontal.saturating_sub(1) * 2))
        .expect("bounded radius cannot exceed the 15-level shade scale")
}

/// Grown crowns cast cumulative diffuse shade, not solid-block shadows.
/// Trunk opacity remains separate. A deciduous crown has little
/// leaf area in winter, and spring/autumn attenuate less than summer. Multiple
/// overlapping crowns can block substantially more daylight than one crown.
pub(crate) fn living_canopy_shade_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> u8 {
    let season = registries.core().calendar().date_at(at).season();
    if season == Season::Winter {
        return 0;
    }
    state
        .world()
        .trees_near_horizontal(position, MAX_CANOPY_RADIUS)
        .filter_map(|tree| {
            let root = tree.root();
            let radius = tree.canopy_radius_voxels(at);
            let horizontal = root
                .x()
                .abs_diff(position.x())
                .max(root.z().abs_diff(position.z()));
            let vertical = i128::from(position.y()) - i128::from(root.y());
            if horizontal > u64::from(radius)
                || !(-i128::from(CANOPY_VERTICAL_REACH_BELOW_ROOT)
                    ..=i128::from(CANOPY_VERTICAL_REACH_ABOVE_ROOT))
                    .contains(&vertical)
            {
                return None;
            }
            let attenuated = crown_shade_at_distance(radius, horizontal);
            Some(if season == Season::Summer {
                attenuated
            } else {
                attenuated.div_ceil(2)
            })
        })
        .fold(0_u8, u8::saturating_add)
        .min(15)
}

/// Conservative direct daylight levels on the same 0..15 scale as local fire.
/// There is no indirect bounce or weather-driven cloud cover yet.
fn sky_brightness(registries: &Registries, at: SimulationTick) -> u8 {
    let calendar = registries.core().calendar();
    let day = calendar.date_at(at).day_tick();
    let phase = u128::from(day) * 4 / u128::from(calendar.ticks_per_day());
    SKY_PHASE_BRIGHTNESS[usize::try_from(phase).expect("four sky phases fit usize")]
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

/// Physical sky opening independent of day phase: rock, built roofs, standing
/// trunks, and absent world truth stop direct outdoor exposure even at night.
pub(crate) fn has_open_sky(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    if !(WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE).contains(&position.y()) {
        return false;
    }
    // Every vertical cell in one horizontal column belongs to the same
    // generated chunk. Resolve that authority once instead of repeating a
    // chunk-index lookup for every elevation and every light/weather query.
    let Some(chunk) = state.world().get_chunk(ChunkCoord::containing(position)) else {
        return false;
    };
    // The structural owner already indexes full-footprint members by
    // horizontal column. One bounded vertical query detects roofs without
    // repeating the spatial-index traversal for every cell above the actor.
    if state.structures().has_full_cell_member_in_voxel_box(
        position,
        VoxelCoord::new(position.x(), WORLD_MAX_Y_EXCLUSIVE - 1, position.z()),
    ) {
        return false;
    }
    // The trunk is only two cells tall, and tree roots are already indexed
    // by horizontal location. One range lookup replaces one or two tree-map
    // searches per vertical cell without treating diffuse canopies as solids.
    if state
        .world()
        .trees_near_horizontal(position, 0)
        .any(|tree| {
            let root = tree.root();
            root.y() < WORLD_MAX_Y_EXCLUSIVE && i128::from(root.y()) + 1 >= i128::from(position.y())
        })
    {
        return false;
    }
    for y in position.y()..WORLD_MAX_Y_EXCLUSIVE {
        let at = VoxelCoord::new(position.x(), y, position.z());
        if registries
            .world()
            .block_for_state(chunk.block_at(at))
            .is_none_or(|definition| definition.is_opaque())
        {
            return false;
        }
    }
    true
}

#[must_use]
pub fn assess_generated_skylight(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> u8 {
    let light = sky_brightness(registries, state.tick());
    if light == 0 || !has_open_sky(registries, state, position) {
        return 0;
    }
    light.saturating_sub(living_canopy_shade_at(
        registries,
        state,
        position,
        state.tick(),
    ))
}

#[cfg(test)]
#[path = "sky_tests.rs"]
mod tests;
