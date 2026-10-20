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

/// Sparse den care: eat fresh food, or dispose of one spoiled parcel along a
/// real, unblocked edge. Both outcomes preserve finite source custody.
pub(super) fn apply_scheduled_den_care(
    registries: &Registries,
    state: &mut AppState,
    from: VoxelCoord,
    at: SimulationTick,
    pressured: bool,
) -> (Option<(SurfaceResourceId, Mass)>, Option<SurfaceResourceId>) {
    let Some(colony) = state.kobold_state().colony.as_ref() else {
        return (None, None);
    };
    if at < colony.next_meal_at {
        return (None, None);
    }
    let at_den = colony.scout_step == 0;
    let wary_at_den = colony.wary_until.is_some_and(|until| at < until);
    let junction = colony.route[1];
    let eaten = if at_den && !pressured {
        consume_den_meal(registries, state, from, at)
    } else {
        None
    };
    // A wary scout must not move a parcel into the junction while sheltering.
    let discarded = if at_den && !pressured && !wary_at_den && eaten.is_none() {
        discard_spoiled_den_food(registries, state, from, junction, at)
    } else {
        None
    };
    state
        .kobold_state_mut()
        .colony
        .as_mut()
        .expect("resident feeding requires existing colony")
        .next_meal_at = at
        .checked_add_span(TickSpan::new(SCOUT_MEAL_INTERVAL_TICKS))
        .expect("bounded simulation cannot overflow meal schedule");
    (eaten, discarded)
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
    if !scout_can_enter(
        registries,
        state,
        junction,
        state.logistics().player().map(|player| player.position()),
    ) || assess_local_illumination(registries, state, junction).level()
        >= SCOUT_DETERRENT_LIGHT_LEVEL
    {
        return None;
    }
    relocate_scavenged_drop_step(state, source, den, junction).then_some(source)
}

/// Deplete one fresh, physically local food parcel and retain the exact mass
/// in the colony's terminal consumption ledger. Both writes are prechecked.
pub(super) fn consume_den_meal(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    at: SimulationTick,
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
