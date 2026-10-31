//! A thirsty surface scout seeks real, reachable water and drinks on site.
//! Water is never teleported into the den or removed without terminal custody.

use crate::content::FLUID_WATER;
use crate::core::quantity::{AggregateVolume, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::fluid::{FluidStoreId, validate_fluid_egress};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::visibility::scout_visible_from;
use super::{
    KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_DRINK_INTERVAL_TICKS, SCOUT_DRINK_VOLUME,
    SCOUT_SAFE_SCAVENGE_TEMPERATURE,
};

/// No distant fluid census. Only fluid whose real spatial index lies at
/// this reached location can satisfy the resident.
pub(super) fn usable_water_at(
    state: &AppState,
    position: VoxelCoord,
) -> Option<(FluidStoreId, Volume)> {
    state.logistics().fluid_stores_at(position).find_map(|id| {
        let contents = state.fluid().get_store(id)?.contents()?;
        (contents.fluid() == FLUID_WATER
            && contents.volume() >= SCOUT_DRINK_VOLUME
            && contents.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE)
            .then_some((id, contents.volume()))
    })
}

/// A scout standing at the junction can see adjacent water and, when the
/// sightline is open, one more traversable edge. Darkness and guarded cells
/// still deter a supply trip. No remote spring map or telepathic target.
pub(super) fn reachable_water_from_junction(
    registries: &Registries,
    state: &AppState,
    colony: &KoboldColony,
    player_position: Option<VoxelCoord>,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let junction = colony.route[1];
    let safe = |from, to| {
        scout_can_enter(registries, state, to, player_position)
            && !scout_sees_guarded_cell(registries, state, from, to)
            && crate::energy::assess_local_illumination(registries, state, to).level()
                < SCOUT_DETERRENT_LIGHT_LEVEL
    };
    let mut routes = Vec::new();
    for first in state.logistics().traversable_destinations(junction) {
        if first == colony.route[0]
            || first == colony.route[2]
            || !safe(junction, first)
            || !scout_visible_from(registries, state, junction, first)
        {
            continue;
        }
        if usable_water_at(state, first).is_some() {
            routes.push((0_u8, first, None));
        }
        for goal in state.logistics().traversable_destinations(first) {
            if goal == junction
                || goal == colony.route[0]
                || goal == colony.route[2]
                || !safe(first, goal)
                || !scout_visible_from(registries, state, junction, goal)
                || usable_water_at(state, goal).is_none()
            {
                continue;
            }
            routes.push((1_u8, first, Some(goal)));
        }
    }
    routes
        .into_iter()
        .min()
        .map(|(_, first, goal)| (first, goal))
}

/// The resident takes one physical 60 mL serving while standing beside the
/// source. This uses the same fluid egress validation as player drinking and
/// records every microliter as consumed by the colony. A blocked egress leaves
/// both the water and the unmet need unchanged.
pub(super) fn drink_on_scout_visit(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
) -> Option<(FluidStoreId, Volume)> {
    let colony = state.kobold_state().colony.as_ref()?;
    if at < colony.next_drink_at || colony.scavenged_drop.is_some() {
        return None;
    }
    let (store, _) = usable_water_at(state, from)?;
    let next_due = at.checked_add_span(TickSpan::new(SCOUT_DRINK_INTERVAL_TICKS))?;
    let next_total = state
        .kobold_state()
        .consumed_fluids
        .get(&FLUID_WATER)
        .copied()
        .unwrap_or(AggregateVolume::ZERO)
        .checked_add(AggregateVolume::from_volume(SCOUT_DRINK_VOLUME))?;
    let withdrawal = validate_fluid_egress(registries, state, store, SCOUT_DRINK_VOLUME).ok()?;
    withdrawal.commit(state).ok()?;
    state
        .kobold_state_mut()
        .apply_drink(FLUID_WATER, next_total);
    state.kobold_state_mut().colony.as_mut()?.next_drink_at = next_due;
    Some((store, SCOUT_DRINK_VOLUME))
}

#[cfg(test)]
#[path = "hydration_tests.rs"]
mod tests;
