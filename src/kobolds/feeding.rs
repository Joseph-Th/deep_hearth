//! Sparse resident feeding from recoverable den custody into terminal matter accounting.

use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::assess_local_illumination;
use crate::inventory::AMBIENT_PRESERVATION_MULTIPLIER_PPM;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord, relocate_scavenged_drop_step,
};
use crate::survival::{FoodFreshness, freshness_from_history};

use super::patrol::scout_can_enter;
use super::scavenging::{den_cache_space, is_servant_era_salvage, reachable_inherited_salvage};
use super::{
    SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS,
};

/// A food parcel is evaluated from its actual aging history, not its form alone.
/// Unfit or spoiled food cannot be collected as edible scout loot.
pub(super) fn food_condition(
    registries: &Registries,
    resource: &SurfaceResourceRecord,
    at: SimulationTick,
) -> Option<FoodFreshness> {
    let food = registries.survival().get_food(resource.commodity())?;
    if resource.composition().pure_material() != Some(resource.commodity().material())
        || !food
            .consumption_temperature()
            .contains(resource.temperature())
    {
        return None;
    }
    freshness_from_history(
        resource.ambient_storage_history_at(at)?,
        AMBIENT_PRESERVATION_MULTIPLIER_PPM,
        at,
        food.shelf_life(),
    )
    .ok()
}

/// A wild forage source must remain edible at the authoritative action tick.
/// Expired patches stay in world custody without blocking fresh neighboring food.
pub(super) fn scout_wants_food_opportunity(
    registries: &Registries,
    resource: &SurfaceResourceRecord,
    at: SimulationTick,
) -> bool {
    resource.kind() == SurfaceResourceKind::Opportunity
        && matches!(
            food_condition(registries, resource, at),
            Some(FoodFreshness::Fresh { .. })
        )
}

pub(super) fn scout_wants_drop(
    registries: &Registries,
    resource: &SurfaceResourceRecord,
    at: SimulationTick,
) -> bool {
    resource.commodity().form() != crate::content::FORM_FOOD
        || matches!(
            food_condition(registries, resource, at),
            Some(FoodFreshness::Fresh { .. })
        )
}

/// Sparse den care: a missed scheduled meal makes hunger persistent until
/// the scout actually eats. It may eat a catch-up meal on a later den visit,
/// but never backlogs missed servings or alters the original meal clock.
/// Spoiled disposal and food-driven cache clearing stay on the regular sparse schedule.
pub(super) fn apply_scheduled_den_care(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    pressured: bool,
) -> (
    Option<(SurfaceResourceId, Mass)>,
    Option<SurfaceResourceId>,
    Option<SurfaceResourceId>,
) {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return (None, None, None);
    };
    let due = at >= colony.next_meal_at;
    let hungry = colony.hungry;
    if !due && !hungry {
        return (None, None, None);
    }
    let at_den = colony.scout_step == 0;
    let wary_at_den = colony.wary_until.is_some_and(|until| at < until);
    let junction = colony.route[1];
    let eaten = if at_den && !pressured {
        consume_den_meal(registries, state, from, at, Mass::ZERO)
    } else {
        None
    };
    // A wary scout must not move a parcel into the junction while sheltering.
    let can_tend_den = due && at_den && !pressured && !wary_at_den && eaten.is_none();
    let discarded = can_tend_den
        .then(|| discard_spoiled_den_food(registries, state, from, junction, at))
        .flatten();
    // Food is a survival priority. Release one inert parcel only if the den
    // is entirely blocked and the scout can actually reach fresh provisions.
    let cleared_cache = (can_tend_den && discarded.is_none())
        .then(|| {
            clear_inert_cache_for_food(registries, state, from, junction, at)
                .or_else(|| clear_inert_cache_for_industry(registries, state, from, junction, at))
        })
        .flatten();
    let colony = state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("resident feeding requires existing colony");
    if due {
        colony.next_meal_at = at
            .checked_add_span(TickSpan::new(SCOUT_MEAL_INTERVAL_TICKS))
            .expect("bounded simulation cannot overflow meal schedule");
    }
    if eaten.is_some() {
        colony.hungry = false;
    } else if due {
        colony.hungry = true;
    }
    (eaten, discarded, cleared_cache)
}

fn discard_spoiled_den_food(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
    at: SimulationTick,
) -> Option<SurfaceResourceId> {
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
                && matches!(
                    food_condition(registries, resource, at),
                    Some(FoodFreshness::Spoiled { .. })
                )
        })?
        .id();
    den_exit_is_safe(registries, state, junction)
        .then(|| relocate_scavenged_drop_step(state, source, den, junction))
        .filter(|moved| *moved)
        .map(|_| source)
}

/// Reorganize a blocked den toward food, not toward an arbitrary scripted
/// desire. Only real local fresh supplies justify releasing a physical parcel.
fn clear_inert_cache_for_food(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
    at: SimulationTick,
) -> Option<SurfaceResourceId> {
    if !den_cache_space(state, den).is_zero() {
        return None;
    }
    let player = state.logistics().player().map(|record| record.position());
    let fresh_nearby = std::iter::once(junction)
        .chain(
            state
                .logistics()
                .traversable_destinations(junction)
                .filter(|position| *position != den),
        )
        .any(|position| {
            state
                .surface()
                .available_resources_at(Some(position))
                .any(|resource| {
                    matches!(
                        food_condition(registries, resource, at),
                        Some(FoodFreshness::Fresh { .. })
                    )
                })
                && (position == junction
                    || (scout_can_enter(registries, state, position, player)
                        && assess_local_illumination(registries, state, position).level()
                            < SCOUT_DETERRENT_LIGHT_LEVEL))
        });
    if !fresh_nearby || !den_exit_is_safe(registries, state, junction) {
        return None;
    }
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
        })?
        .id();
    relocate_scavenged_drop_step(state, source, den, junction).then_some(source)
}

/// A reported machine threat gives the old servant culture a concrete
/// salvage objective. A full den may release *one* expendable inert parcel
/// only when a precision copper component can truly be reached and brought
/// back. The freed mass is custody, never an abstract purchase price.
fn clear_inert_cache_for_industry(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
    at: SimulationTick,
) -> Option<SurfaceResourceId> {
    let colony = state.kobold_state().colony.as_ref()?;
    if colony.mobilized_at.is_none() || !den_cache_space(state, den).is_zero() {
        return None;
    }
    let tip = colony.route[2];
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .find(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
                && !is_servant_era_salvage(resource)
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
        })?;
    let source_id = source.id();
    let released_capacity = source.remaining_mass();
    let player = state.logistics().player().map(|record| record.position());
    if !den_exit_is_safe(registries, state, junction)
        || reachable_inherited_salvage(
            registries,
            state,
            junction,
            den,
            tip,
            player,
            released_capacity,
            at,
        )
        .is_none()
    {
        return None;
    }
    relocate_scavenged_drop_step(state, source_id, den, junction).then_some(source_id)
}

fn den_exit_is_safe(registries: &Registries, state: &AppState, junction: VoxelCoord) -> bool {
    scout_can_enter(
        registries,
        state,
        junction,
        state.logistics().player().map(|player| player.position()),
    ) && assess_local_illumination(registries, state, junction).level()
        < SCOUT_DETERRENT_LIGHT_LEVEL
}

/// Deplete one fresh, physically local food parcel and retain the exact mass
/// in the colony's terminal consumption ledger. Both writes are prechecked.
pub(super) fn consume_den_meal(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    at: SimulationTick,
    minimum_ration: Mass,
) -> Option<(SurfaceResourceId, Mass)> {
    if !state.can_spend_surface_revisions(1) {
        return None;
    }
    let (source, material, mass, consumed_after) = state
        .surface()
        .available_resources_at(Some(den))
        .find_map(|resource| {
            if resource.kind() != SurfaceResourceKind::Drop {
                return None;
            }
            if !matches!(
                food_condition(registries, resource, at),
                Some(FoodFreshness::Fresh { .. })
            ) {
                return None;
            }
            let mass = resource.remaining_mass().min(SCOUT_MEAL_MASS);
            if mass < minimum_ration {
                return None;
            }
            let material = resource.commodity().material();
            let consumed_after = state
                .kobold_state()
                .consumed_matter
                .get(&material)
                .copied()
                .unwrap_or(AggregateMass::ZERO)
                .checked_add(AggregateMass::from_mass(mass))?;
            Some((resource.id(), material, mass, consumed_after))
        })?;
    let next_revision = state
        .surface()
        .revision()
        .checked_add(1)
        .expect("feeding checked surface and future revision headroom");
    state
        .surface_state_mut()
        .apply_gathering(source, mass, at, next_revision);
    state
        .kobold_state_mut()
        .apply_meal(material, consumed_after);
    Some((source, mass))
}
