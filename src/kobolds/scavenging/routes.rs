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
use super::super::{SCOUT_BULK_SPLIT_BATCH, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_MASS};
use super::{
    best_local_forage, food_serving_priority, is_contestable_drop, is_portable_drop,
    local_unenclosed_stockpile_target, scout_wants_loose_drop,
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
/// one actual pickup can yield against the outstanding meal, not the original
/// serving size. Mixed salvage retains whole-before-split handling.
type DropLootPriority = (bool, bool, bool, u64, SurfaceResourceId);

fn detour_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    capacity: Mass,
    food_shortage: Option<Mass>,
) -> DropLootPriority {
    let needs_split = !is_portable_drop(resource) || resource.remaining_mass() > capacity;
    let split_yield = resource
        .remaining_mass()
        .min(capacity)
        .min(SCOUT_BULK_SPLIT_BATCH);
    let (inadequate_food, partial_size) = match food_shortage {
        Some(shortage) => {
            let recoverable = if needs_split {
                split_yield
            } else {
                resource.remaining_mass()
            };
            let inadequate = recoverable < shortage;
            (
                inadequate,
                if inadequate {
                    u64::MAX - recoverable.milligrams()
                } else {
                    0
                },
            )
        }
        None => food_serving_priority(resource, SCOUT_MEAL_MASS),
    };
    if food_shortage.is_some() {
        (
            inadequate_food,
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

/// Select the priority from one indexed physical cell once. Neighboring and
/// two-edge routes share identical freshness, capacity, and hunger filtering;
/// only observation and path safety differ.
fn best_local_drop_priority(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    capacity: Mass,
    at: SimulationTick,
    food_shortage: Option<Mass>,
) -> Option<DropLootPriority> {
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_contestable_drop(resource, capacity)
                && scout_wants_loose_drop(registries, state, resource, at)
                && (food_shortage.is_none()
                    || resource.commodity().form() == crate::content::FORM_FOOD)
        })
        .map(|resource| detour_loot_priority(resource, capacity, food_shortage))
        .min()
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
    food_shortage: Option<Mass>,
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
            best_local_drop_priority(registries, state, destination, capacity, at, food_shortage)
                .map(|priority| (priority, destination))
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
    food_shortage: Option<Mass>,
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
                    best_local_drop_priority(registries, state, goal, capacity, at, food_shortage)
                        .map(|priority| (priority, first, goal))
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
/// Some(shortage) seeks food sufficient for the *remaining* meal; None seeks scrap.
/// All routes use current light, terrain and player protection.
pub(in crate::kobolds) fn reachable_detour_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_shortage: Option<Mass>,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = reachable_side_drop(registries, state, route, capacity, at, food_shortage);
    // Generic scrap remains distance-first, even if a farther manufactured
    // part is easier to lift. Avoid food-specific rescans entirely.
    let Some(shortage) = food_shortage else {
        return side.map(|first| (first, None)).or_else(|| {
            reachable_far_drop(registries, state, route, capacity, at, None)
                .map(|(first, goal)| (first, Some(goal)))
        });
    };
    let whole_at = |position, minimum_mass| {
        state
            .surface()
            .available_resources_at(Some(position))
            .any(|resource| {
                is_portable_drop(resource)
                    && resource.remaining_mass() <= capacity
                    && resource.remaining_mass() >= minimum_mass
                    && scout_wants_loose_drop(registries, state, resource, at)
                    && resource.commodity().form() == crate::content::FORM_FOOD
            })
    };
    // A genuinely complete distant intact ration justifies one extra edge.
    // Otherwise a complete nearby split must beat a distant tiny fragment;
    // the return edge can actually recover that meal in one operation.
    if let Some(first) = side.filter(|first| whole_at(*first, shortage)) {
        return Some((first, None));
    }
    let far = reachable_far_drop(registries, state, route, capacity, at, Some(shortage));
    if let Some((first, goal)) = far.filter(|(_, goal)| whole_at(*goal, shortage)) {
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
                        >= shortage
            })
    };
    if let Some(first) = side.filter(|first| complete_split_at(*first)) {
        return Some((first, None));
    }
    if let Some((first, goal)) = far.filter(|(_, goal)| complete_split_at(*goal)) {
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
