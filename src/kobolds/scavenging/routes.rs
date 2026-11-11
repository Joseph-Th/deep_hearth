//! Bounded real-edge routes to food, loose parcels, and exposed stockpiles.

use std::cmp::Reverse;

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceId;

use super::super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::super::visibility::scout_visible_from;
use super::super::{SCOUT_BULK_SPLIT_BATCH, SCOUT_DETERRENT_LIGHT_LEVEL};
use super::{
    SCOUT_MEAL_MASS, best_local_forage, food_serving_priority, is_contestable_drop,
    is_portable_drop, local_unenclosed_stockpile_target, scout_wants_loose_drop,
};

/// One real three-stop patrol graph with the currently observed player position.
/// Search methods may vary resource capacity or freshness time, but must not
/// accidentally exchange the den, junction, or authored outer endpoint.
#[derive(Clone, Copy)]
pub(in crate::kobolds) struct ScavengeRoute {
    pub(super) junction: VoxelCoord,
    pub(super) den: VoxelCoord,
    pub(super) authored_tip: VoxelCoord,
    pub(super) player_position: Option<VoxelCoord>,
}

impl ScavengeRoute {
    pub(in crate::kobolds) const fn new(
        [den, junction, authored_tip]: [VoxelCoord; 3],
        player_position: Option<VoxelCoord>,
    ) -> Self {
        Self {
            junction,
            den,
            authored_tip,
            player_position,
        }
    }
}

/// An informed scout may divert to one genuinely visible, reachable exposed
/// outpost store from its patrol junction. Distant inventory is never scanned:
/// the existing short traversal graph, local sight, barriers, light and player
/// presence admit every candidate before a side route is selected.
pub(in crate::kobolds) fn reachable_side_stockpile(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(route.junction)
        .filter(|cell| *cell != route.den && *cell != route.authored_tip)
        .filter(|cell| local_unenclosed_stockpile_target(registries, state, *cell, at).is_some())
        .filter(|cell| {
            scout_visible_from(registries, state, route.junction, *cell)
                && unguarded_unlit_cell(
                    registries,
                    state,
                    route.junction,
                    *cell,
                    route.player_position,
                )
        })
        .min()
}

/// A two-edge industrial outpost is a legitimate raid destination only if
/// its actual ground custody and both route cells are currently accessible,
/// dark and visibly linked to the scout's junction. Missing world truth does
/// not authorize a path. The route is chosen deterministically by coordinates.
pub(in crate::kobolds) fn reachable_far_stockpile(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
) -> Option<(VoxelCoord, VoxelCoord)> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        ..
    } = route;
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .flat_map(|first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| {
                    *goal != junction && *goal != den && *goal != authored_tip && *goal != first
                })
                .filter(move |goal| {
                    local_unenclosed_stockpile_target(registries, state, *goal, at).is_some()
                })
                .map(move |goal| (first, goal))
        })
        .filter(|(first, goal)| {
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
        })
        .min()
}

/// Intact cargo is cheapest to move. A hungry scout must first compare what
/// one actual pickup can yield: a complete meal from a split is preferable to
/// a tiny intact fragment. Mixed salvage retains whole-before-split handling.
fn detour_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    capacity: Mass,
    food_only: bool,
) -> (bool, bool, bool, u64, SurfaceResourceId) {
    let (inadequate_food, partial_size) = food_serving_priority(resource);
    let needs_split = !is_portable_drop(resource) || resource.remaining_mass() > capacity;
    let split_yield = resource
        .remaining_mass()
        .min(capacity)
        .min(SCOUT_BULK_SPLIT_BATCH);
    if food_only {
        (
            if needs_split {
                split_yield < SCOUT_MEAL_MASS
            } else {
                inadequate_food
            },
            needs_split,
            false,
            partial_size,
            resource.id(),
        )
    } else {
        (
            needs_split,
            resource.commodity().form() != crate::content::FORM_FOOD,
            inadequate_food,
            partial_size,
            resource.id(),
        )
    }
}

pub(super) fn unguarded_unlit_cell(
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

/// Far stockpile, loose-drop, and forage planners must share the same observed
/// threat boundary. A future waypoint cannot lend its viewpoint to the scout.
/// Actual patrol movement checks the next edge again at that waypoint.
fn far_route_is_sighted_and_safe(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    (first, goal): (VoxelCoord, VoxelCoord),
) -> bool {
    scout_visible_from(registries, state, route.junction, goal)
        && unguarded_unlit_cell(
            registries,
            state,
            route.junction,
            first,
            route.player_position,
        )
        && unguarded_unlit_cell(
            registries,
            state,
            route.junction,
            goal,
            route.player_position,
        )
}

/// Search only neighboring traversal cells, not the full history of world resources.
/// Moving a whole parcel precedes splitting a bulky pile at the same route distance.
pub(in crate::kobolds) fn reachable_side_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<VoxelCoord> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        player_position,
    } = route;
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
                .min_by_key(|resource| detour_loot_priority(resource, capacity, food_only))
                .map(|resource| (detour_loot_priority(resource, capacity, food_only), destination))
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
pub(in crate::kobolds) fn reachable_far_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<(VoxelCoord, VoxelCoord)> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        ..
    } = route;
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
                        .min_by_key(|resource| detour_loot_priority(resource, capacity, food_only))
                        .map(|resource| {
                            (
                                detour_loot_priority(resource, capacity, food_only),
                                first,
                                goal,
                            )
                        })
                })
        })
        .filter(|(_, first, goal)| {
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
        })
        .min_by_key(|(priority, _, _)| *priority)
        .map(|(_, first, goal)| (first, goal))
}

/// A hungry scout seeking loose food prefers a whole recoverable ration to
/// splitting a nearer bulky food pile, even one graph edge farther out.
/// Ordinary mixed scrap retains its distance-first patrol behavior until an
/// actual industrial report activates the separate precision salvage priority.
/// All routes use current light, terrain and player protection.
pub(in crate::kobolds) fn reachable_detour_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_only: bool,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = reachable_side_drop(registries, state, route, capacity, at, food_only);
    // Generic scrap must not gain knowledge of distant manufactured copper
    // just because that parcel is easier to lift than a nearer stone pile.
    if !food_only && let Some(first) = side {
        return Some((first, None));
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
    // A genuinely complete distant intact ration justifies one extra edge.
    // Otherwise a complete nearby split must beat a distant tiny fragment;
    // the return edge can actually recover that meal in one operation.
    if food_only && let Some(first) = side.filter(|first| whole_at(*first, SCOUT_MEAL_MASS)) {
        return Some((first, None));
    }
    let far = reachable_far_drop(registries, state, route, capacity, at, food_only);
    if food_only
        && let Some((first, goal)) = far.filter(|(_, goal)| whole_at(*goal, SCOUT_MEAL_MASS))
    {
        return Some((first, Some(goal)));
    }
    let complete_split_at = |position| {
        state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| {
                is_contestable_drop(resource, capacity)
                    && scout_wants_loose_drop(registries, state, resource, at)
                    && resource.commodity().form() == crate::content::FORM_FOOD
                    && (!is_portable_drop(resource) || resource.remaining_mass() > capacity)
                    && resource
                        .remaining_mass()
                        .min(capacity)
                        .min(SCOUT_BULK_SPLIT_BATCH)
                        >= SCOUT_MEAL_MASS
            })
    };
    if food_only && let Some(first) = side.filter(|first| complete_split_at(*first)) {
        return Some((first, None));
    }
    if food_only && let Some((first, goal)) = far.filter(|(_, goal)| complete_split_at(*goal)) {
        return Some((first, Some(goal)));
    }
    // Only partial portions remain. Prefer intact cargo and the nearer edge,
    // then a local split when there is no suitable intact parcel.
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
pub(in crate::kobolds) fn reachable_side_food(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<VoxelCoord> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        player_position,
    } = route;
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
pub(in crate::kobolds) fn reachable_far_food(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(VoxelCoord, VoxelCoord)> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        ..
    } = route;
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
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
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
pub(in crate::kobolds) fn reachable_detour_food(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
    meal_shortage: Mass,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = reachable_side_food(registries, state, route, at, maximum_mass);
    let far = reachable_far_food(registries, state, route, at, maximum_mass);
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
