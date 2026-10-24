//! Deterministic search for exposed scout loot and bounded recoverable portions.

use std::cmp::Reverse;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

use super::feeding::{scout_wants_drop, scout_wants_food_opportunity};
use super::junkworks::{
    den_or_relay_needs_striker, den_or_relay_wants_iron_chime, is_iron_chime_striker,
    is_junk_striker, is_staged_relay_component,
};
use super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::scrap_tools::{is_needed_outer_tool_part, is_staged_scrap_tool_part};
use super::visibility::scout_visible_from;
use super::{
    DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_MASS,
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
    scout_wants_drop(registries, resource, at)
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

/// Select whole, den-fitting parcels before sources requiring a split.
/// Within each transfer-cost tier, edible loot wins, then a complete meal,
/// then the largest partial meal. Stable identity breaks remaining ties.
fn detour_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    capacity: Mass,
) -> (bool, bool, bool, u64, SurfaceResourceId) {
    let (inadequate_food, partial_size) = food_serving_priority(resource);
    (
        !is_portable_drop(resource) || resource.remaining_mass() > capacity,
        resource.commodity().form() != crate::content::FORM_FOOD,
        inadequate_food,
        partial_size,
        resource.id(),
    )
}

fn unguarded_unlit_cell(
    registries: &Registries,
    state: &AppState,
    scout: VoxelCoord,
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    scout_can_enter(registries, state, position, player_position)
        // Current visibility, not the hypothetical view from a future step,
        // decides if a live player makes this destination too risky.
        && !(player_position.is_some()
            && scout_sees_guarded_cell(registries, state, scout, position))
        && assess_local_illumination(registries, state, position).level()
            < SCOUT_DETERRENT_LIGHT_LEVEL
}

/// Search only neighboring traversal cells, not the full history of world resources.
/// Moving a whole parcel precedes splitting a bulky pile at the same route distance.
pub(super) fn reachable_side_drop(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| *destination != den && *destination != authored_tip)
        .filter_map(|destination| {
            state
                .surface()
                .available_resources_at(Some(destination))
                .filter(|resource| {
                    is_contestable_drop(resource, capacity)
                        && scout_wants_loose_drop(registries, state, resource, at)
                        && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
                })
                .min_by_key(|resource| detour_loot_priority(resource, capacity))
                .map(|resource| (detour_loot_priority(resource, capacity), destination))
        })
        // Lighting propagation is more expensive than an indexed local parcel lookup.
        .filter(|(_, destination)| {
            unguarded_unlit_cell(registries, state, junction, *destination, player_position)
        })
        .min_by_key(|(priority, _)| *priority)
        .map(|(_, destination)| destination)
}

/// Search a second authored traversal edge only when neighboring caches are absent.
/// The first eligible intermediate route wins when the same resource has multiple paths.
pub(super) fn reachable_far_drop(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<(VoxelCoord, VoxelCoord)> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .flat_map(|first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| {
                    *goal != first && *goal != den && *goal != junction && *goal != authored_tip
                })
                .filter_map(move |goal| {
                    state
                        .surface()
                        .available_resources_at(Some(goal))
                        .filter(|resource| {
                            is_contestable_drop(resource, capacity)
                                && scout_wants_loose_drop(registries, state, resource, at)
                                && (!food_only
                                    || resource.commodity().form() == crate::content::FORM_FOOD)
                        })
                        .min_by_key(|resource| detour_loot_priority(resource, capacity))
                        .map(|resource| (detour_loot_priority(resource, capacity), first, goal))
                })
        })
        .filter(|(_, first, goal)| {
            scout_visible_from(registries, state, junction, *goal)
                && unguarded_unlit_cell(registries, state, junction, *first, player_position)
                && unguarded_unlit_cell(registries, state, junction, *goal, player_position)
        })
        .min_by_key(|(priority, _, _)| *priority)
        .map(|(_, first, goal)| (first, goal))
}

/// A hungry scout seeking loose food prefers a whole recoverable ration to
/// splitting a nearer bulky food pile, even one graph edge farther out.
/// Ordinary mixed scrap retains its distance-first patrol behavior until an
/// actual industrial report activates the separate precision salvage priority.
/// All routes use current light, terrain and player protection.
pub(super) fn reachable_detour_drop(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = reachable_side_drop(
        registries,
        state,
        junction,
        den,
        authored_tip,
        player_position,
        capacity,
        at,
        food_only,
    );
    // Generic scrap must not gain knowledge of distant manufactured copper
    // just because that parcel is easier to lift than a nearer stone pile.
    if !food_only {
        if let Some(first) = side {
            return Some((first, None));
        }
    }
    let whole_at = |position, minimum_mass| {
        state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| {
                is_portable_drop(resource)
                    && resource.remaining_mass() <= capacity
                    && resource.remaining_mass() >= minimum_mass
                    && scout_wants_loose_drop(registries, state, resource, at)
                    && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
            })
    };
    // A tiny intact fragment should not defeat a genuinely adequate ration
    // one more route edge away. When no full meal can be carried, retain the
    // existing whole-before-split, distance-first fallback for partial food.
    if food_only {
        if let Some(first) = side.filter(|first| whole_at(*first, SCOUT_MEAL_MASS)) {
            return Some((first, None));
        }
    }
    let far = reachable_far_drop(
        registries,
        state,
        junction,
        den,
        authored_tip,
        player_position,
        capacity,
        at,
        food_only,
    );
    if food_only {
        if let Some((first, goal)) = far.filter(|(_, goal)| whole_at(*goal, SCOUT_MEAL_MASS)) {
            return Some((first, Some(goal)));
        }
    }
    if let Some(first) = side.filter(|first| whole_at(*first, Mass::ZERO)) {
        return Some((first, None));
    }
    if let Some((first, goal)) = far.filter(|(_, goal)| whole_at(*goal, Mass::ZERO)) {
        return Some((first, Some(goal)));
    }
    side.map(|first| (first, None))
        .or_else(|| far.map(|(first, goal)| (first, Some(goal))))
}

/// When junction wild food is exhausted, prefer the largest real forage serving
/// among adjacent traversable patches, with stable identity resolving equal yields.
pub(super) fn reachable_side_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| *destination != den && *destination != authored_tip)
        .filter_map(|destination| {
            best_local_forage(registries, state, destination, at, maximum_mass)
                .map(|(mass, id)| (mass, id, destination))
        })
        .filter(|(_, _, destination)| {
            unguarded_unlit_cell(registries, state, junction, *destination, player_position)
        })
        .max_by_key(|(mass, id, _)| (*mass, Reverse(*id)))
        .map(|(_, _, destination)| destination)
}

/// Seek finite natural food beyond one real intermediate route junction.
/// Candidate resources are position-indexed and checked before expensive light
/// and terrain resolution; each candidate uses only two adjacent graph edges.
pub(super) fn reachable_far_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(VoxelCoord, VoxelCoord)> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .flat_map(|first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| *goal != junction && *goal != den && *goal != authored_tip)
                .filter_map(move |goal| {
                    best_local_forage(registries, state, goal, at, maximum_mass)
                        .map(|(mass, id)| (mass, id, first, goal))
                })
        })
        .filter(|(_, _, first, goal)| {
            scout_visible_from(registries, state, junction, *goal)
                && unguarded_unlit_cell(registries, state, junction, *first, player_position)
                && unguarded_unlit_cell(registries, state, junction, *goal, player_position)
        })
        .max_by_key(|(mass, id, first, goal)| {
            (*mass, Reverse(*id), Reverse(*first), Reverse(*goal))
        })
        .map(|(_, _, first, goal)| (first, goal))
}

/// Nearby patches save travel, but a distant complete meal is more useful
/// than a local crumb when a single forage triggers the same recovery clock.
/// When both patches are insufficient, prefer the larger recoverable serving.
/// Only previously admitted, sighted and guarded route candidates compete.
pub(super) fn reachable_detour_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    at: SimulationTick,
    maximum_mass: Mass,
    meal_shortage: Mass,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = reachable_side_food(
        registries,
        state,
        junction,
        den,
        authored_tip,
        player_position,
        at,
        maximum_mass,
    );
    let far = reachable_far_food(
        registries,
        state,
        junction,
        den,
        authored_tip,
        player_position,
        at,
        maximum_mass,
    );
    match (side, far) {
        (Some(first), Some((second, goal))) => {
            let side_mass = best_local_forage(registries, state, first, at, maximum_mass)?.0;
            let far_mass = best_local_forage(registries, state, goal, at, maximum_mass)?.0;
            if side_mass >= meal_shortage || side_mass >= far_mass {
                Some((first, None))
            } else {
                Some((second, Some(goal)))
            }
        }
        (Some(first), None) => Some((first, None)),
        (None, Some((first, goal))) => Some((first, Some(goal))),
        (None, None) => None,
    }
}

/// Remnants of precision craft matter more to former machine-servants than
/// raw rock or copper ore. This only classifies existing, authored loose goods:
/// an installed or carried machine is never loot in this surface-resource query.
pub(super) fn is_servant_era_salvage(resource: &crate::surface::SurfaceResourceRecord) -> bool {
    use crate::content::{
        FORM_DRILL_BIT, FORM_ELECTRICAL_WINDING, FORM_FLYWHEEL, FORM_REINFORCEMENT, FORM_SAW_BLADE,
        FORM_TOOL, MATERIAL_COPPER,
    };
    let commodity = resource.commodity();
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
    junction: VoxelCoord,
    observer: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    capacity: Mass,
    at: SimulationTick,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
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
