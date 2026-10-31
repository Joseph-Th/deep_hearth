//! Sparse resident feeding from recoverable den custody into terminal matter accounting.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, Mass};
use crate::core::state::AppState;
use crate::core::time::{SimulationTick, TickSpan};
use crate::inventory::AMBIENT_PRESERVATION_MULTIPLIER_PPM;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord};
use crate::survival::{FoodFreshness, freshness_from_history};

mod eviction;

use super::{SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS};
use eviction::{
    clear_inert_cache_for_food, clear_inert_cache_for_industry, discard_unfit_den_food,
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
/// A hungry scout also tends a blocked den on its ordinary five-tick home visits,
/// so newly accessible food need not wait for the next 400-tick meal deadline.
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
    // Missing rations are not backlogged, but a real consumed fragment remains
    // credited until the same 20 g meal is completed at a later home visit.
    let remaining = SCOUT_MEAL_MASS
        .checked_sub(colony.partial_meal)
        .expect("trusted resident cannot already have a complete partial meal");
    if !due && !hungry {
        return (None, None, None);
    }
    let at_den = colony.scout_step == 0;
    let wary_at_den = colony.wary_until.is_some_and(|until| at < until);
    let junction = colony.route[1];
    let eaten = if at_den && !pressured {
        // A local serving that finishes the remaining ration takes priority
        // over smaller fragments. Every consumed scrap remains credited toward
        // that same meal across actual den visits.
        consume_den_meal(registries, state, from, at, remaining, remaining)
            .or_else(|| consume_den_meal(registries, state, from, at, remaining, Mass::ZERO))
    } else {
        None
    };
    // A partial serving cannot end the hungry resident's den care. Once it
    // eats an inadequate fragment, it may still free space for a proper next
    // meal on the same scheduled home visit (through another real surface
    // transaction). A complete serving stops further custody changes.
    let complete_meal = eaten.is_some_and(|(_, mass)| mass >= remaining);
    let still_needed = remaining
        .checked_sub(eaten.map(|(_, mass)| mass).unwrap_or(Mass::ZERO))
        .expect("no den serving exceeds the resident's unmet ration");
    let can_tend_den = (due || hungry) && at_den && !pressured && !wary_at_den && !complete_meal;
    let discarded = can_tend_den
        .then(|| discard_unfit_den_food(registries, state, from, junction, at))
        .flatten();
    // Food is a survival priority. Release inert cargo only when the remaining
    // den capacity cannot hold the actual fresh food the scout can collect.
    let cleared_cache = (can_tend_den && discarded.is_none())
        .then(|| {
            clear_inert_cache_for_food(registries, state, from, junction, at, still_needed).or_else(
                || {
                    (due && eaten.is_none())
                        .then(|| {
                            clear_inert_cache_for_industry(registries, state, from, junction, at)
                        })
                        .flatten()
                },
            )
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
    if complete_meal {
        colony.hungry = false;
        colony.partial_meal = Mass::ZERO;
    } else if due || eaten.is_some() {
        colony.hungry = true;
        if let Some((_, mass)) = eaten {
            colony.partial_meal = colony
                .partial_meal
                .checked_add(mass)
                .expect("incomplete ration cannot overflow its bounded 20 g total");
        }
    }
    (eaten, discarded, cleared_cache)
}

/// Deplete one fresh, physically local food parcel and retain the exact mass
/// in the colony's terminal consumption ledger. Both writes are prechecked.
pub(super) fn consume_den_meal(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    at: SimulationTick,
    maximum_ration: Mass,
    minimum_ration: Mass,
) -> Option<(SurfaceResourceId, Mass)> {
    if !state.can_spend_surface_revisions(1) {
        return None;
    }
    let (source, material, mass, consumed_after, _) = state
        .surface()
        .available_resources_at(Some(den))
        .filter_map(|resource| {
            if resource.kind() != SurfaceResourceKind::Drop {
                return None;
            }
            if !matches!(
                food_condition(registries, resource, at),
                Some(FoodFreshness::Fresh { .. })
            ) {
                return None;
            }
            let mass = resource.remaining_mass().min(maximum_ration);
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
            Some((
                resource.id(),
                material,
                mass,
                consumed_after,
                resource.remaining_mass(),
            ))
        })
        // Leave an intact full ration when a smaller parcel can finish this
        // meal. If no parcel is sufficient, choose the largest usable partial
        // serving. Neither selection needs an intermediate allocation.
        .min_by_key(|(id, _, mass, _, available)| {
            (
                if minimum_ration.is_zero() {
                    u64::MAX - mass.milligrams()
                } else {
                    available.milligrams()
                },
                *id,
            )
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

/// A watch sortie needs one complete meal, not one arbitrarily large food
/// parcel. A full serving is preferred; otherwise several fresh den fragments
/// may together supply its exact ration in one physical feeding opportunity.
///
/// Check the entire ration, terminal matter ledgers and surface revision
/// headroom before touching any custody. An insufficient or unrepresentable
/// collection leaves every fragment untouched.
pub(super) fn consume_complete_den_ration(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    at: SimulationTick,
) -> bool {
    if consume_den_meal(registries, state, den, at, SCOUT_MEAL_MASS, SCOUT_MEAL_MASS).is_some() {
        return true;
    }
    let mut fragments = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && matches!(
                    food_condition(registries, resource, at),
                    Some(FoodFreshness::Fresh { .. })
                )
        })
        .map(|resource| {
            (
                resource.id(),
                resource.commodity().material(),
                resource.remaining_mass(),
            )
        })
        .collect::<Vec<_>>();
    // Consume larger fragments first to minimize physical handling and owner
    // writes. Stable IDs break ties independently of map insertion history.
    fragments.sort_unstable_by_key(|(id, _, mass)| (Reverse(*mass), *id));
    let mut remaining = SCOUT_MEAL_MASS;
    let mut portions = Vec::new();
    let mut ledgers = BTreeMap::new();
    for (source, material, available) in fragments {
        if remaining.is_zero() {
            break;
        }
        let mass = available.min(remaining);
        let before = ledgers.get(&material).copied().unwrap_or_else(|| {
            state
                .kobold_state()
                .consumed_matter
                .get(&material)
                .copied()
                .unwrap_or(AggregateMass::ZERO)
        });
        let Some(after) = before.checked_add(AggregateMass::from_mass(mass)) else {
            return false;
        };
        ledgers.insert(material, after);
        portions.push((source, material, mass, after));
        remaining = remaining
            .checked_sub(mass)
            .expect("selected watch ration never exceeds its missing mass");
    }
    if !remaining.is_zero()
        || portions.len() < 2
        || !state.can_spend_surface_revisions(portions.len() as u64)
    {
        return false;
    }
    for (source, material, mass, consumed_after) in portions {
        let revision = state
            .surface()
            .revision()
            .checked_add(1)
            .expect("complete watch ration reserved every surface revision");
        state
            .surface_state_mut()
            .apply_gathering(source, mass, at, revision);
        state
            .kobold_state_mut()
            .apply_meal(material, consumed_after);
    }
    true
}
