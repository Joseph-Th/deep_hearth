//! Physical den discard and finite-cache eviction for fresh food and salvaged machinery.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, relocate_scavenged_drop_step,
    split_bulk_scavenged_drop_step,
};
use crate::survival::FoodFreshness;

use super::{food_condition, scout_wants_food_opportunity};
use crate::kobolds::junkworks::{current_den_rig, den_or_relay_needs_striker};
use crate::kobolds::patrol::{scout_can_enter, scout_sees_guarded_cell};
use crate::kobolds::scavenging::{
    ScavengeRoute, den_cache_space, is_contestable_drop, is_portable_drop, is_servant_era_salvage,
    matches_salvage_goal, reachable_inherited_salvage, scout_wants_loose_drop,
};
use crate::kobolds::{
    DEN_LOOSE_CACHE_LIMIT, SCOUT_BULK_SPLIT_BATCH, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL,
    SCOUT_MEAL_MASS, SCOUT_SAFE_SCAVENGE_TEMPERATURE,
};

/// Vacate unusable edible-form custody as physical cargo. Spoilage, excessive heat and
/// contaminated composition all make a food parcel unfit for this resident's meal.
/// Only hand-safe parcels can be physically discarded; hot food remains in den custody.
/// Oversized hand-safe parcels are divided into real carried portions.
pub(super) fn discard_unfit_den_food(
    registries: &Registries,
    state: &mut AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
    at: SimulationTick,
) -> Option<SurfaceResourceId> {
    if !den_exit_is_safe(registries, state, den, junction) {
        return None;
    }
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() == crate::content::FORM_FOOD
                && !matches!(
                    food_condition(registries, resource, at),
                    Some(FoodFreshness::Fresh { .. })
                )
        })
        // Whole cargo requires no new identity or split, so clear it first.
        .min_by_key(|resource| (resource.remaining_mass() > SCOUT_CARRY_LIMIT, resource.id()))?;
    let source_id = source.id();
    if source.remaining_mass() <= SCOUT_CARRY_LIMIT {
        relocate_scavenged_drop_step(registries, state, source_id, den, junction)
            .then_some(source_id)
    } else {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source_id,
            den,
            junction,
            at,
            SCOUT_BULK_SPLIT_BATCH,
        )
        .map(|(daughter, _)| daughter)
    }
}

/// Reorganize a blocked den toward food, not toward an arbitrary scripted
/// desire. Only real local fresh supplies justify releasing a physical parcel.
pub(super) fn clear_inert_cache_for_food(
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
            crate::kobolds::visibility::scout_visible_from(registries, state, den, *position)
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
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
        })
        // Prefer a whole portable parcel: it frees space without allocating a
        // new identity. Among those, preserve precision salvage and release
        // the least raw mass that fills the shortage. If only oversized inert
        // cargo remains, split a real portion instead of starving beside food.
        .min_by_key(|resource| {
            let mass = resource.remaining_mass().milligrams();
            (
                resource.remaining_mass() > SCOUT_CARRY_LIMIT,
                mass < missing,
                is_servant_era_salvage(resource),
                if mass < missing { u64::MAX - mass } else { mass },
                resource.id(),
            )
        })?;
    let source_id = source.id();
    if source.remaining_mass() <= SCOUT_CARRY_LIMIT {
        relocate_scavenged_drop_step(registries, state, source_id, den, junction)
            .then_some(source_id)
    } else {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source_id,
            den,
            junction,
            at,
            SCOUT_BULK_SPLIT_BATCH,
        )
        .map(|(daughter, _)| daughter)
    }
}

/// A reported machine threat gives the old servant culture a concrete
/// salvage objective. A den short of capacity may release *one* expendable
/// inert parcel when it cannot fit an intact visible precision component.
/// The freed mass is custody, never an abstract purchase price.
pub(super) fn clear_inert_cache_for_industry(
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
    let needs_striker = den_or_relay_needs_striker(state, den);
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
                            && scout_wants_loose_drop(registries, state, item, at)
                            && is_portable_drop(item)
                            && item.remaining_mass() <= free
                    })
            },
        )
    {
        return None;
    }
    // Only an actually visible precision source can justify an exchange.
    // Prefer whole portable cargo and the smallest release that fits an intact
    // manufactured component. If every expendable parcel is oversized, assess
    // the actual split portion rather than pretending the full pile can move.
    let source = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && resource.kind() == SurfaceResourceKind::Drop
                && resource.commodity().form() != crate::content::FORM_FOOD
                && !is_servant_era_salvage(resource)
                && Some(resource.id()) != protected_striker
        })
        .filter_map(|resource| {
            // An oversized pile frees only one physically split batch on this
            // visit, not its full mass. The den itself holds at most 500 g.
            let portable = resource.remaining_mass() <= SCOUT_CARRY_LIMIT;
            let freed = if portable {
                resource.remaining_mass()
            } else {
                SCOUT_BULK_SPLIT_BATCH
            };
            let capacity = Mass::from_milligrams(
                free.milligrams()
                    .saturating_add(freed.milligrams())
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
                        && scout_wants_loose_drop(registries, state, candidate, at)
                        && is_portable_drop(candidate)
                        && candidate.remaining_mass() <= capacity
                });
            let mass = resource.remaining_mass().milligrams();
            Some((
                (
                    !portable,
                    !whole,
                    if whole {
                        mass
                    } else {
                        u64::MAX - capacity.milligrams()
                    },
                    resource.id(),
                ),
                resource.id(),
                portable,
            ))
        })
        .min_by_key(|(priority, _, _)| *priority)?;
    let source_id = source.1;
    if source.2 {
        relocate_scavenged_drop_step(registries, state, source_id, den, junction)
            .then_some(source_id)
    } else {
        split_bulk_scavenged_drop_step(
            registries,
            state,
            source_id,
            den,
            junction,
            at,
            SCOUT_BULK_SPLIT_BATCH,
        )
        .map(|(daughter, _)| daughter)
    }
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
