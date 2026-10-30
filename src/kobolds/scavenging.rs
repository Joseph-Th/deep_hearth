//! Deterministic search for exposed scout loot and bounded recoverable portions.

use std::cmp::Reverse;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::material::CommodityKey;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

mod routes;
mod stockpiles;

use routes::unguarded_unlit_cell;
pub(super) use routes::{
    ScavengeRoute, reachable_detour_drop, reachable_detour_food, reachable_far_stockpile,
    reachable_side_stockpile,
};
#[cfg(test)]
use routes::{reachable_far_drop, reachable_far_food, reachable_side_drop, reachable_side_food};

pub(super) use stockpiles::local_unenclosed_stockpile_target;

use super::feeding::{scout_wants_drop, scout_wants_food_opportunity};
use super::junkworks::{
    den_or_relay_needs_striker, den_or_relay_wants_iron_chime, is_iron_chime_striker,
    is_junk_striker, is_staged_relay_component,
};
use super::scrap_tools::{is_needed_outer_tool_part, is_staged_scrap_tool_part};
use super::visibility::scout_visible_from;
use super::{
    DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_MEAL_MASS, SCOUT_SAFE_SCAVENGE_TEMPERATURE,
};

/// Choose the largest *recoverable* local forage portion, rather than the oldest
/// nearly depleted patch. A forage attempt consumes the same long cooldown
/// regardless of its yield. Identity only breaks ties between equal servings.
/// Both scouting and the eventual physical transfer use this ranking.
pub(super) fn best_local_forage(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(Mass, SurfaceResourceId)> {
    if maximum_mass.is_zero() {
        return None;
    }
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| scout_wants_food_opportunity(registries, resource, at))
        .map(|resource| (resource.remaining_mass().min(maximum_mass), resource.id()))
        .max_by_key(|(mass, id)| (*mass, Reverse(*id)))
}

/// Selected staged repeater parts remain untouched by their own returning
/// scavenger while the den signal source exists, even when only the first
/// part has arrived. They are never reserved from player pickup.
pub(super) fn scout_wants_loose_drop(
    registries: &Registries,
    state: &AppState,
    resource: &crate::surface::SurfaceResourceRecord,
    at: SimulationTick,
) -> bool {
    // Loose parcels must satisfy the same bare-handed heat limit as exposed stockpile lots.
    resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
        && scout_wants_drop(registries, resource, at)
        && !is_staged_relay_component(state, resource)
        && !is_staged_scrap_tool_part(state, resource)
}

/// Short food parcels cannot satisfy one meal. Prefer a complete local ration;
/// among inadequate fragments, take the largest available serving first.
/// All adequate servings remain tied so stable identity controls equal-value food.
fn food_serving_priority(resource: &crate::surface::SurfaceResourceRecord) -> (bool, u64) {
    let inadequate = resource.commodity().form() == crate::content::FORM_FOOD
        && resource.remaining_mass() < SCOUT_MEAL_MASS;
    (
        inadequate,
        if inadequate {
            u64::MAX - resource.remaining_mass().milligrams()
        } else {
            0
        },
    )
}

/// Survival food always precedes inert cargo, but mobilized scavengers use
/// inherited service-work knowledge to recognize shaped precision copper.
/// Stable surface identity breaks every equal-value tie.
fn scout_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    mobilized: bool,
    needs_striker: bool,
    needed_tool_part: bool,
    wants_iron: bool,
) -> (bool, bool, bool, bool, bool, u64, SurfaceResourceId) {
    let not_food = resource.commodity().form() != crate::content::FORM_FOOD;
    let (inadequate_food, partial_size) = food_serving_priority(resource);
    (
        not_food,
        inadequate_food,
        !(wants_iron && is_iron_chime_striker(resource)),
        !(mobilized && matches_salvage_goal(resource, needs_striker)),
        !needed_tool_part,
        partial_size,
        resource.id(),
    )
}

/// Actual den/repeater need controls component preference without remote discovery.
fn scout_striker_interest(state: &AppState, mobilized: bool) -> (bool, bool) {
    state
        .kobold_state()
        .colony
        .as_ref()
        .filter(|_| mobilized)
        .map_or((false, false), |colony| {
            let missing = den_or_relay_needs_striker(state, colony.route[0]);
            (
                missing,
                missing || den_or_relay_wants_iron_chime(state, colony),
            )
        })
}

pub(super) fn matches_salvage_goal(
    resource: &crate::surface::SurfaceResourceRecord,
    needs_striker: bool,
) -> bool {
    if needs_striker {
        is_junk_striker(resource)
    } else {
        is_servant_era_salvage(resource)
    }
}

pub(super) fn is_portable_drop(resource: &crate::surface::SurfaceResourceRecord) -> bool {
    resource.kind() == SurfaceResourceKind::Drop && resource.remaining_mass() <= SCOUT_CARRY_LIMIT
}

/// A reachable loose pile is unsafe even if too large to move as one parcel.
pub(super) fn is_contestable_drop(
    resource: &crate::surface::SurfaceResourceRecord,
    space: Mass,
) -> bool {
    resource.kind() == SurfaceResourceKind::Drop
        && resource.lifecycle() == SurfaceResourceLifecycle::Available
        && !resource.remaining_mass().is_zero()
        && !space.is_zero()
}

/// Detect new loose matter without confusing a scout's own just-discarded
/// parcel with a newly discovered player cache.
pub(super) fn finds_external_drop(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    at: SimulationTick,
    ignored: [Option<SurfaceResourceId>; 2],
) -> bool {
    state
        .surface()
        .available_resources_at(Some(position))
        .any(|resource| {
            is_contestable_drop(resource, space)
                && scout_wants_loose_drop(registries, state, resource, at)
                && !ignored.contains(&Some(resource.id()))
        })
}

/// A two-edge food raid must recheck actual edible opportunity when arriving.
/// An intervening pickup, spoilage or depleted source cancels the last step.
pub(super) fn scout_goal_has_supplies(
    registries: &Registries,
    state: &AppState,
    goal: VoxelCoord,
    space: Mass,
    forage_ready: bool,
    at: SimulationTick,
    mobilized: bool,
) -> bool {
    state
        .surface()
        .available_resources_at(Some(goal))
        .any(|resource| {
            (is_contestable_drop(resource, space)
                && scout_wants_loose_drop(registries, state, resource, at))
                || (forage_ready
                    && !space.is_zero()
                    && scout_wants_food_opportunity(registries, resource, at))
        })
        || (mobilized
            && !space.is_zero()
            && local_unenclosed_stockpile_target(registries, state, goal, at).is_some())
}

/// Decide whether the scout should turn home from its present cell.
/// A hungry scout ignores inert parcels at the junction while finding food.
pub(super) fn has_local_scout_interest(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    forage_ready: bool,
    at: SimulationTick,
    seeking_food: bool,
) -> bool {
    state
        .surface()
        .available_resources_at(Some(position))
        .any(|resource| {
            let loose = is_contestable_drop(resource, space)
                && scout_wants_loose_drop(registries, state, resource, at);
            if seeking_food {
                (loose && resource.commodity().form() == crate::content::FORM_FOOD)
                    || (forage_ready
                        && !space.is_zero()
                        && scout_wants_food_opportunity(registries, resource, at))
            } else {
                loose
            }
        })
}

/// Hunger must suppress inert cargo only while an edible local alternative exists.
pub(super) fn should_seek_local_food(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    forage_ready: bool,
    at: SimulationTick,
    hungry: bool,
) -> bool {
    hungry && has_local_scout_interest(registries, state, position, space, forage_ready, at, true)
}

/// Choose one locally portable drop without scanning remote or depleted records.
pub(super) fn local_portable_source(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    at: SimulationTick,
    food_only: bool,
    mobilized: bool,
) -> Option<SurfaceResourceId> {
    let (needs_striker, wants_iron) = scout_striker_interest(state, mobilized);
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_portable_drop(resource)
                && resource.remaining_mass() <= space
                && scout_wants_loose_drop(registries, state, resource, at)
                && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
        })
        .min_by_key(|resource| {
            scout_loot_priority(
                resource,
                mobilized,
                needs_striker,
                is_needed_outer_tool_part(state, position, resource),
                wants_iron,
            )
        })
        .map(|resource| resource.id())
}

/// Choose one oversized local pile to split on a real retreat edge.
/// Stable surface IDs decide equal forms without a historical-world scan.
pub(super) fn local_split_source(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    at: SimulationTick,
    food_only: bool,
    mobilized: bool,
) -> Option<SurfaceResourceId> {
    let (needs_striker, wants_iron) = scout_striker_interest(state, mobilized);
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_contestable_drop(resource, space)
                && scout_wants_loose_drop(registries, state, resource, at)
                && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
                && (!is_portable_drop(resource) || resource.remaining_mass() > space)
        })
        .min_by_key(|resource| {
            scout_loot_priority(
                resource,
                mobilized,
                needs_striker,
                is_needed_outer_tool_part(state, position, resource),
                wants_iron,
            )
        })
        .map(|resource| resource.id())
}

/// The den uses ordinary surface custody. Its finite loose storage includes all available
/// player and scout drops, not just parcels labelled as belonging to the colony.
pub(super) fn den_cache_space(state: &AppState, den: VoxelCoord) -> Mass {
    let occupied: u128 = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| resource.kind() == SurfaceResourceKind::Drop)
        .map(|resource| u128::from(resource.remaining_mass().milligrams()))
        .sum();
    let available = u128::from(DEN_LOOSE_CACHE_LIMIT.milligrams()).saturating_sub(occupied);
    Mass::from_milligrams(
        u64::try_from(available).expect("remaining den space fits its fixed mass limit"),
    )
}

/// Remnants of precision craft matter more to former machine-servants than
/// raw rock or copper ore. This only classifies existing, authored loose goods:
/// an installed or carried machine is never loot in this surface-resource query.
pub(super) fn is_servant_era_salvage(resource: &crate::surface::SurfaceResourceRecord) -> bool {
    is_servant_era_commodity(resource.commodity())
}

fn is_servant_era_commodity(commodity: CommodityKey) -> bool {
    use crate::content::{
        FORM_DRILL_BIT, FORM_ELECTRICAL_WINDING, FORM_FLYWHEEL, FORM_REINFORCEMENT, FORM_SAW_BLADE,
        FORM_TOOL, MATERIAL_COPPER,
    };
    commodity.material() == MATERIAL_COPPER
        && [
            FORM_DRILL_BIT,
            FORM_ELECTRICAL_WINDING,
            FORM_FLYWHEEL,
            FORM_REINFORCEMENT,
            FORM_SAW_BLADE,
            FORM_TOOL,
        ]
        .contains(&commodity.form())
}

fn engineered_drop_at(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    space: Mass,
    at: SimulationTick,
    needs_striker: bool,
) -> Option<(bool, SurfaceResourceId)> {
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            matches_salvage_goal(resource, needs_striker)
                && is_contestable_drop(resource, space)
                && scout_wants_loose_drop(registries, state, resource, at)
        })
        .min_by_key(|resource| {
            (
                !is_portable_drop(resource) || resource.remaining_mass() > space,
                resource.id(),
            )
        })
        .map(|resource| {
            (
                !is_portable_drop(resource) || resource.remaining_mass() > space,
                resource.id(),
            )
        })
}

/// After a *delivered* industrial report, prioritize locally reachable precision
/// copper parts even beyond nearer generic scrap. The caller supplies the
/// scout's *current* observation voxel, which is the den during den care and
/// the junction during actual branch planning. The target is not reserved;
/// entry and source-local movement still recheck custody, clearance and light.
pub(super) fn reachable_inherited_salvage(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    observer: VoxelCoord,
    capacity: Mass,
    at: SimulationTick,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        player_position,
    } = route;
    if capacity.is_zero() {
        return None;
    }
    let needs_striker = den_or_relay_needs_striker(state, den);
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .filter_map(|first| {
            engineered_drop_at(registries, state, first, capacity, at, needs_striker)
                .map(|priority| (priority, first))
        })
        .filter(|(_, first)| {
            scout_visible_from(registries, state, observer, *first)
                && unguarded_unlit_cell(registries, state, observer, *first, player_position)
        })
        .min_by_key(|(priority, first)| (*priority, *first));
    // A complete nearby component is always preferable to a longer detour.
    // When the nearer source must be split, look one edge farther for a whole
    // component before choosing the extra handling work. Both alternatives
    // remain bound to the scout's current sightline and real graph edges.
    if let Some(((false, _), first)) = side {
        return Some((first, None));
    }
    let far = state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .flat_map(|first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| *goal != junction && *goal != den && *goal != authored_tip)
                .filter_map(move |goal| {
                    engineered_drop_at(registries, state, goal, capacity, at, needs_striker)
                        .map(|priority| (priority, first, goal))
                })
        })
        .filter(|(_, first, goal)| {
            scout_visible_from(registries, state, observer, *goal)
                && unguarded_unlit_cell(registries, state, observer, *first, player_position)
                && unguarded_unlit_cell(registries, state, observer, *goal, player_position)
        })
        .min_by_key(|(priority, first, goal)| (*priority, *first, *goal));
    match (side, far) {
        (_, Some(((false, _), first, goal))) => Some((first, Some(goal))),
        (Some((_, first)), _) => Some((first, None)),
        (None, Some((_, first, goal))) => Some((first, Some(goal))),
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "scavenging_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loot_sight_tests.rs"]
mod loot_sight_tests;

#[cfg(test)]
#[path = "stockpile_raid_tests.rs"]
mod stockpile_raid_tests;
