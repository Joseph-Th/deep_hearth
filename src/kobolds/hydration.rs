//! A thirsty surface scout seeks real, reachable water and drinks on site.
//! Water is never teleported into the den or removed without terminal custody.

use crate::content::FLUID_WATER;
use crate::core::quantity::{AggregateVolume, Volume};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::fluid::{FluidStoreId, ValidatedFluidEgress, validate_fluid_egress};
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::{WindCondition, generated_air_at, wind_at_generated_position};

use super::patrol::{opening_resident_can_depart, scout_can_enter, scout_sees_guarded_cell};
use super::visibility::scout_visible_from;
use super::{
    KoboldColony, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_DRINK_INTERVAL_TICKS, SCOUT_DRINK_VOLUME,
};

/// The scout's next water requirement responds to the ambient temperature at
/// its last real drink, even under a roof. Wind changes need only when an
/// actual sky or upwind inlet lets the regional gust reach the source.
/// Ungenerated terrain has no inferred climate and retains the baseline.
pub(super) fn drink_interval_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
) -> u64 {
    let Some(air) = generated_air_at(registries, state, position, at) else {
        return SCOUT_DRINK_INTERVAL_TICKS;
    };
    match air.ambient_temperature().millikelvin() {
        298_150.. => SCOUT_DRINK_INTERVAL_TICKS / 2,
        293_150..=298_149
            if air.wind() == WindCondition::Gusting
                && wind_at_generated_position(registries, state, position, at)
                    == Some(WindCondition::Gusting) =>
        {
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
/// this reached location and whose withdrawal is currently admissible can
/// satisfy the resident. Route choice must agree with the eventual drink.
pub(super) fn usable_water_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Option<(FluidStoreId, Volume)> {
    selectable_water_withdrawal(registries, state, position)
        .map(|(store, withdrawal)| (store, withdrawal.volume()))
}

/// Pick the most useful physically withdrawable local serving, with stable
/// identity ties. A tiny first vessel must not displace a full second vessel.
/// The selected fluid-owner proof is also reused by the actual drinking edge.
fn selectable_water_withdrawal(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Option<(FluidStoreId, ValidatedFluidEgress)> {
    let remaining =
        SCOUT_DRINK_VOLUME.checked_sub(state.kobold_state().colony.as_ref()?.partial_drink)?;
    usable_water_sources_at(registries, state, position)
        .filter_map(|(store, available)| {
            let volume = available.min(remaining);
            validate_fluid_egress(registries, state, store, volume)
                .ok()
                .map(|withdrawal| (store, withdrawal))
        })
        .max_by_key(|(store, withdrawal)| (withdrawal.volume(), std::cmp::Reverse(*store)))
}

fn usable_water_sources_at<'a>(
    registries: &Registries,
    state: &'a AppState,
    position: VoxelCoord,
) -> impl Iterator<Item = (FluidStoreId, Volume)> + 'a {
    let drink = registries.survival().get_drink(FLUID_WATER).copied();
    state
        .logistics()
        .fluid_stores_at(position)
        .filter_map(move |id| {
            let contents = state.fluid().get_store(id)?.contents()?;
            (contents.fluid() == FLUID_WATER
                && !contents.volume().is_zero()
                && drink.is_some_and(|definition| {
                    definition
                        .consumption_temperature()
                        .contains(contents.temperature())
                }))
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
    let remaining = SCOUT_DRINK_VOLUME.checked_sub(colony.partial_drink)?;
    // Compare the serving that the fluid owner can *actually withdraw*, not
    // merely the existence of a water store. A drop on the normal patrol
    // must not outrank a complete drink one safe step away.
    //
    // Volume is capped at the remaining ration. Equal servings prefer the
    // patrol tip, then one-edge branches, then two-edge branches. Stable
    // coordinates break ties; no remote water records or extra state are used.
    let tip = colony.route[2];
    let tip_volume = (safe(junction, tip) && scout_visible_from(registries, state, junction, tip))
        .then(|| usable_water_at(registries, state, tip).map(|(_, volume)| volume))
        .flatten();
    if tip_volume == Some(remaining) {
        return Some((tip, None));
    }
    let mut best_route = tip_volume.map(|volume| {
        (
            volume,
            std::cmp::Reverse(0_u8),
            std::cmp::Reverse(tip),
            std::cmp::Reverse(None),
        )
    });
    // The indexed water lookup is cheap; expensive sight and terrain
    // decisions stay bounded to genuinely traversable, locally seen cells.
    for first in state.logistics().traversable_destinations(junction) {
        if first == colony.route[0]
            || first == tip
            || !safe(junction, first)
            || !scout_visible_from(registries, state, junction, first)
        {
            continue;
        }
        if let Some((_, volume)) = usable_water_at(registries, state, first) {
            let route = (
                volume,
                std::cmp::Reverse(1_u8),
                std::cmp::Reverse(first),
                std::cmp::Reverse(None),
            );
            best_route = Some(best_route.map_or(route, |best| route.max(best)));
        }
        for goal in state.logistics().traversable_destinations(first) {
            if goal == junction
                || goal == colony.route[0]
                || goal == tip
                || !safe(first, goal)
                || !scout_visible_from(registries, state, junction, goal)
            {
                continue;
            }
            if let Some((_, volume)) = usable_water_at(registries, state, goal) {
                let route = (
                    volume,
                    std::cmp::Reverse(2_u8),
                    std::cmp::Reverse(first),
                    std::cmp::Reverse(Some(goal)),
                );
                best_route = Some(best_route.map_or(route, |best| route.max(best)));
            }
        }
    }
    best_route.map(|(_, _, std::cmp::Reverse(first), std::cmp::Reverse(goal))| (first, goal))
}

/// The resident drinks at most the remaining part of one physical 60 mL
/// serving. Every microliter enters terminal consumption accounting, including
/// a partial drink that leaves the need overdue until another real visit.
/// Blocked egress leaves both the water and the unmet need unchanged.
pub(super) fn drink_on_scout_visit(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
) -> Option<(FluidStoreId, Volume)> {
    let colony = state.kobold_state().colony.as_ref()?;
    // Carrying a parcel does not prevent drinking from water reached on the patrol.
    // The carried drop remains at this voxel until its separate custody transfer.
    // Hydration precedes ordinary patrol planning. A mined-out generated floor
    // must not permit a stranded resident to extract water before movement's
    // independent source-footing gate runs.
    if at < colony.next_drink_at || !opening_resident_can_depart(registries, state, from) {
        return None;
    }
    let remaining = SCOUT_DRINK_VOLUME.checked_sub(colony.partial_drink)?;
    let (store, withdrawal) = selectable_water_withdrawal(registries, state, from)?;
    let volume = withdrawal.volume();
    let next_partial = colony.partial_drink.checked_add(volume)?;
    let completed = volume == remaining;
    let next_schedule = if completed {
        let interval = drink_interval_at(registries, state, from, at);
        Some((interval, at.checked_add_span(TickSpan::new(interval))?))
    } else {
        None
    };
    let next_total = state
        .kobold_state()
        .consumed_fluids
        .get(&FLUID_WATER)
        .copied()
        .unwrap_or(AggregateVolume::ZERO)
        .checked_add(AggregateVolume::from_volume(volume))?;
    withdrawal.commit(state).ok()?;
    state
        .kobold_state_mut()
        .apply_drink(FLUID_WATER, next_total);
    let colony = state.kobold_state_mut().colony.as_mut()?;
    colony.partial_drink = if completed {
        Volume::ZERO
    } else {
        next_partial
    };
    if let Some((interval, next_due)) = next_schedule {
        colony.next_drink_at = next_due;
        colony.last_drink_at = Some(at);
        colony.last_drink_interval_ticks = Some(interval);
    }
    Some((store, volume))
}

#[cfg(test)]
#[path = "hydration_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hydration_partial_tests.rs"]
mod partial_tests;

#[cfg(test)]
#[path = "hydration_route_tests.rs"]
mod route_tests;
