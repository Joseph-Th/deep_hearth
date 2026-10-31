//! A thirsty surface scout seeks real, reachable water and drinks on site.
//! Water is never teleported into the den or removed without terminal custody.

use crate::content::FLUID_WATER;
use crate::core::quantity::{AggregateVolume, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::fluid::{FluidStoreId, validate_fluid_egress};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{WindCondition, exposed_air_at};

use super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::visibility::scout_visible_from;
use super::{
    KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_DRINK_INTERVAL_TICKS, SCOUT_DRINK_VOLUME,
    SCOUT_SAFE_SCAVENGE_TEMPERATURE,
};

/// The scout's next physical water requirement responds to actual outdoor
/// conditions at its last drink. Missing terrain authority and shelter
/// retain the ordinary rate; heat and exposed gusts accelerate demand.
pub(super) fn drink_interval_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> u64 {
    let Some(outdoor) = exposed_air_at(registries, state, position, at) else {
        return SCOUT_DRINK_INTERVAL_TICKS;
    };
    match outdoor.ambient_temperature().millikelvin() {
        298_150.. => SCOUT_DRINK_INTERVAL_TICKS / 2,
        293_150..=298_149 if outdoor.wind() == WindCondition::Gusting => {
            SCOUT_DRINK_INTERVAL_TICKS * 3 / 4
        }
        0..=278_149 => SCOUT_DRINK_INTERVAL_TICKS * 3 / 2,
        _ => SCOUT_DRINK_INTERVAL_TICKS,
    }
}

/// Missing a real water visit by a second full baseline interval limits
/// optional labor. The resident can still forage, finish carried cargo, and
/// follow physically reachable water; an actual drink resolves deprivation.
pub(super) fn prolonged_water_deprivation(colony: &KoboldColony, at: SimulationTick) -> bool {
    at.value()
        >= colony
            .next_drink_at
            .value()
            .saturating_add(SCOUT_DRINK_INTERVAL_TICKS)
}

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
    // The water location index is cheap; visibility and local light are not.
    // Keep only the shortest deterministic route, without allocating a list.
    let mut best_route = None;
    for first in state.logistics().traversable_destinations(junction) {
        if first == colony.route[0]
            || first == colony.route[2]
            || !safe(junction, first)
            || !scout_visible_from(registries, state, junction, first)
        {
            continue;
        }
        if usable_water_at(state, first).is_some() {
            let route = (0_u8, first, None);
            best_route = Some(best_route.map_or(route, |best| route.min(best)));
        }
        for goal in state.logistics().traversable_destinations(first) {
            if goal == junction
                || goal == colony.route[0]
                || goal == colony.route[2]
                || usable_water_at(state, goal).is_none()
                || !safe(first, goal)
                || !scout_visible_from(registries, state, junction, goal)
            {
                continue;
            }
            let route = (1_u8, first, Some(goal));
            best_route = Some(best_route.map_or(route, |best| route.min(best)));
        }
    }
    best_route.map(|(_, first, goal)| (first, goal))
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
    let interval = drink_interval_at(registries, state, from, at);
    let next_due = at.checked_add_span(TickSpan::new(interval))?;
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
    let colony = state.kobold_state_mut().colony.as_mut()?;
    colony.next_drink_at = next_due;
    colony.last_drink_at = Some(at);
    colony.last_drink_interval_ticks = Some(interval);
    Some((store, SCOUT_DRINK_VOLUME))
}

#[cfg(test)]
#[path = "hydration_tests.rs"]
mod tests;
