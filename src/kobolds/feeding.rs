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

use super::junkworks::{current_den_rig, den_needs_striker};
use super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::scavenging::{
    ScavengeRoute, den_cache_space, is_contestable_drop, is_portable_drop, is_servant_era_salvage,
    matches_salvage_goal, reachable_inherited_salvage,
};
use super::{
    DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL,
    SCOUT_MEAL_INTERVAL_TICKS, SCOUT_MEAL_MASS,
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

/// Vacate unusable edible-form custody as physical cargo. Spoilage, excessive heat and
/// contaminated composition all make a food parcel unfit for this resident's meal.
fn discard_unfit_den_food(
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
                && resource.commodity().form() == crate::content::FORM_FOOD
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
                && !matches!(
                    food_condition(registries, resource, at),
                    Some(FoodFreshness::Fresh { .. })
                )
        })?
        .id();
    den_exit_is_safe(registries, state, den, junction)
        .then(|| relocate_scavenged_drop_step(registries, state, source, den, junction))
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
    meal_shortage: Mass,
) -> Option<SurfaceResourceId> {
    let free = den_cache_space(state, den);
    if free >= meal_shortage || !den_exit_is_safe(registries, state, den, junction) {
        return None;
    }
    // Some space is not necessarily *enough* space: a missed five-gram meal
    // must not leave the scout endlessly foraging five-gram portions while
    // the rest of the den is inert cargo. Consider only recoverable food seen
    // from home, and only evict stock when that food exceeds free capacity.
    let forage_ready = state
        .kobold_state()
        .colony
        .as_ref()
        .is_some_and(|colony| colony.next_forage_at.is_none_or(|due| at >= due));
    let player = state.logistics().player().map(|record| record.position());
    let wanted = std::iter::once(junction)
        .chain(
            state
                .logistics()
                .traversable_destinations(junction)
                .filter(|position| *position != den),
        )
        .filter(|position| {
            super::visibility::scout_visible_from(registries, state, den, *position)
                && (*position == junction
                    || (scout_can_enter(registries, state, *position, player)
                        && !scout_sees_guarded_cell(registries, state, den, *position)
                        && assess_local_illumination(registries, state, *position).level()
                            < SCOUT_DETERRENT_LIGHT_LEVEL))
        })
        .flat_map(|position| state.surface().available_resources_at(Some(position)))
        .filter(|resource| match resource.kind() {
            SurfaceResourceKind::Opportunity => {
                forage_ready && scout_wants_food_opportunity(registries, resource, at)
            }
            SurfaceResourceKind::Drop => {
                resource.commodity().form() == crate::content::FORM_FOOD
                    && is_contestable_drop(resource, SCOUT_MEAL_MASS)
                    && matches!(
                        food_condition(registries, resource, at),
                        Some(FoodFreshness::Fresh { .. })
                    )
            }
        })
        .map(|resource| resource.remaining_mass().min(meal_shortage))
        .max()?;
    if wanted <= free {
        return None;
    }
    let missing = wanted.milligrams() - free.milligrams();
    let source_id = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
        })
        // Among parcels that fill the actual shortage, preserve precision
        // copper and release the least raw mass. If none fills it, releasing
        // the largest parcel still enables a larger physical partial catch.
        .min_by_key(|resource| {
            let mass = resource.remaining_mass().milligrams();
            (
                mass < missing,
                is_servant_era_salvage(resource),
                if mass < missing { u64::MAX - mass } else { mass },
                resource.id(),
            )
        })?
        .id();
    relocate_scavenged_drop_step(registries, state, source_id, den, junction).then_some(source_id)
}

/// A reported machine threat gives the old servant culture a concrete
/// salvage objective. A den short of capacity may release *one* expendable
/// inert parcel when it cannot fit an intact visible precision component.
/// The freed mass is custody, never an abstract purchase price.
fn clear_inert_cache_for_industry(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
    at: SimulationTick,
) -> Option<SurfaceResourceId> {
    let colony = state.kobold_state().colony.as_ref()?;
    colony.mobilized_at?;
    let route = ScavengeRoute::new(
        colony.route,
        state.logistics().player().map(|record| record.position()),
    );
    let needs_striker = den_needs_striker(state, den);
    let protected_striker = current_den_rig(state, colony).map(|rig| rig.striker());
    if !den_exit_is_safe(registries, state, den, junction) {
        return None;
    }
    let free = den_cache_space(state, den);
    // If a whole visible component already fits, a raid needs no eviction.
    // Do not expel local matter merely to take a different, larger haul.
    if !free.is_zero()
        && reachable_inherited_salvage(registries, state, route, den, free, at).is_some_and(
            |(first, far)| {
                state
                    .surface()
                    .available_resources_at(Some(far.unwrap_or(first)))
                    .any(|item| {
                        matches_salvage_goal(item, needs_striker)
                            && is_portable_drop(item)
                            && item.remaining_mass() <= free
                    })
            },
        )
    {
        return None;
    }
    // Only an actually visible precision source can justify an exchange.
    // Prefer the smallest expendable parcel that will fit a whole manufactured
    // component; otherwise free the largest available space for a real split.
    // This avoids losing a huge raw cache just because its ID came first, or
    // making repeated tiny raids when a single intact pickup is possible.
    let source_id = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
                && !is_servant_era_salvage(resource)
                && Some(resource.id()) != protected_striker
                && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
        })
        .filter_map(|resource| {
            // Adding a released parcel to any existing free space is bounded
            // by the den's actual 500 g capacity, not the scout's carry limit.
            let capacity = Mass::from_milligrams(
                free.milligrams()
                    .saturating_add(resource.remaining_mass().milligrams())
                    .min(DEN_LOOSE_CACHE_LIMIT.milligrams()),
            );
            if capacity <= free {
                return None;
            }
            let (first, far) =
                reachable_inherited_salvage(registries, state, route, den, capacity, at)?;
            let target = far.unwrap_or(first);
            let whole = state
                .surface()
                .available_resources_at(Some(target))
                .any(|candidate| {
                    matches_salvage_goal(candidate, needs_striker)
                        && is_portable_drop(candidate)
                        && candidate.remaining_mass() <= capacity
                });
            let mass = resource.remaining_mass().milligrams();
            Some((
                (
                    !whole,
                    if whole {
                        mass
                    } else {
                        u64::MAX - capacity.milligrams()
                    },
                    resource.id(),
                ),
                resource.id(),
            ))
        })
        .min_by_key(|(priority, _)| *priority)?
        .1;
    relocate_scavenged_drop_step(registries, state, source_id, den, junction).then_some(source_id)
}

fn den_exit_is_safe(
    registries: &Registries,
    state: &AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
) -> bool {
    // A sentry standing in the junction prevents the scout from physically
    // pushing its discard or freed cache through the occupied route cell.
    if state.kobold_state().colony.as_ref().is_some_and(|colony| {
        colony.watch_deployed_at.is_some() && colony.watch_position() == junction
    }) {
        return false;
    }
    scout_can_enter(
        registries,
        state,
        junction,
        state.logistics().player().map(|player| player.position()),
    ) && !scout_sees_guarded_cell(registries, state, den, junction)
        && assess_local_illumination(registries, state, junction).level()
            < SCOUT_DETERRENT_LIGHT_LEVEL
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
