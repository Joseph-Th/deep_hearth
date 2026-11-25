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

/// The bounded two-edge envelope includes a continuation through the ordinary
/// patrol tip. The den and directly reachable patrol cells are not far goals.
pub(in crate::kobolds) fn two_edge_candidates(
    state: &AppState,
    route: ScavengeRoute,
) -> impl Iterator<Item = (VoxelCoord, VoxelCoord)> + '_ {
    state
        .logistics()
        .traversable_destinations(route.junction)
        .filter(move |first| *first != route.den)
        .flat_map(move |first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| {
                    *goal != route.den && *goal != route.junction && *goal != route.authored_tip
                })
                .map(move |goal| (first, goal))
        })
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
    two_edge_candidates(state, route)
        .filter(|(_, goal)| {
            local_unenclosed_stockpile_target(registries, state, *goal, at).is_some()
        })
        .filter(|(first, goal)| {
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
        })
        .min()
}

/// Ordering differs for hunger and ordinary salvage. Name the ranking fields
/// so route-length comparison cannot accidentally discard the serving size.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DropLootClass {
    Hungry {
        inadequate: bool,
        needs_split: bool,
        partial_size: u64,
    },
    Mixed {
        needs_split: bool,
        not_food: bool,
        inadequate: bool,
        partial_size: u64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct DropLootPriority {
    class: DropLootClass,
    resource: SurfaceResourceId,
}

fn detour_loot_priority(
    registries: &Registries,
    resource: &crate::surface::SurfaceResourceRecord,
    capacity: Mass,
    food_shortage: Option<Mass>,
) -> DropLootPriority {
    let needs_split = !is_portable_drop(resource) || resource.remaining_mass() > capacity;
    let class = if let Some(shortage) = food_shortage {
        let recoverable = if needs_split {
            resource
                .remaining_mass()
                .min(capacity)
                .min(SCOUT_BULK_SPLIT_BATCH)
        } else {
            resource.remaining_mass()
        };
        let inadequate = recoverable < shortage;
        DropLootClass::Hungry {
            inadequate,
            needs_split,
            partial_size: if inadequate {
                u64::MAX - recoverable.milligrams()
            } else {
                0
            },
        }
    } else {
        let (inadequate, partial_size) =
            food_serving_priority(registries, resource, SCOUT_MEAL_MASS);
        DropLootClass::Mixed {
            needs_split,
            not_food: registries
                .survival()
                .get_food(resource.commodity())
                .is_none(),
            inadequate,
            partial_size,
        }
    };
    DropLootPriority {
        class,
        resource: resource.id(),
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
                    || registries
                        .survival()
                        .get_food(resource.commodity())
                        .is_some())
        })
        .map(|resource| detour_loot_priority(registries, resource, capacity, food_shortage))
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

/// Rank the neighboring drop candidates once. Hungry scouts also consider the
/// ordinary patrol tip; mixed salvage still uses the side-route search.
fn best_adjacent_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_shortage: Option<Mass>,
) -> Option<(DropLootPriority, VoxelCoord)> {
    let ScavengeRoute {
        junction,
        den,
        authored_tip,
        player_position,
    } = route;
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| {
            *destination != den && (food_shortage.is_some() || *destination != authored_tip)
        })
        .filter_map(|destination| {
            best_local_drop_priority(registries, state, destination, capacity, at, food_shortage)
                .map(|priority| (priority, destination))
        })
        // Lighting propagation is more expensive than an indexed local parcel lookup.
        .filter(|(_, destination)| {
            unguarded_unlit_cell(registries, state, junction, *destination, player_position)
        })
        .min_by_key(|(priority, _)| *priority)
}

/// Rank safe two-edge candidates without discarding their recoverable-food class.
fn best_far_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_shortage: Option<Mass>,
) -> Option<(DropLootPriority, VoxelCoord, VoxelCoord)> {
    two_edge_candidates(state, route)
        .filter_map(|(first, goal)| {
            best_local_drop_priority(registries, state, goal, capacity, at, food_shortage)
                .map(|priority| (priority, first, goal))
        })
        .filter(|(_, first, goal)| {
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
        })
        .min_by_key(|(priority, _, _)| *priority)
}

/// A hungry scout seeks a complete ration before partial food, preferring
/// intact cargo to a split within each class and larger partial servings.
/// Shorter routes win only after these food priorities are equal.
/// Mixed salvage retains its ordinary distance-first side-route behavior.
pub(in crate::kobolds) fn reachable_detour_drop(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    capacity: Mass,
    at: SimulationTick,
    food_shortage: Option<Mass>,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let adjacent = best_adjacent_drop(registries, state, route, capacity, at, food_shortage);
    // A nearby intact full ration is already optimal; generic salvage always
    // prefers the nearest route. No farther scan is necessary in either case.
    if let Some((priority, first)) = adjacent
        && (food_shortage.is_none()
            || matches!(
                priority.class,
                DropLootClass::Hungry {
                    inadequate: false,
                    needs_split: false,
                    ..
                }
            ))
    {
        return Some((first, None));
    }
    let far = best_far_drop(registries, state, route, capacity, at, food_shortage);
    match (adjacent, far) {
        (Some((near_priority, _)), Some((far_priority, first, goal)))
            if far_priority.class < near_priority.class =>
        {
            Some((first, Some(goal)))
        }
        (Some((_, first)), _) => Some((first, None)),
        (None, far) => far.map(|(_, first, goal)| (first, Some(goal))),
    }
}

/// When junction wild food is exhausted, prefer the largest real forage serving
/// among adjacent traversable patches, with stable identity resolving equal yields.
fn best_adjacent_forage(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(Mass, VoxelCoord)> {
    let ScavengeRoute {
        junction,
        den,
        player_position,
        ..
    } = route;
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| *destination != den)
        .filter_map(|destination| {
            best_local_forage(registries, state, destination, at, maximum_mass)
                .map(|(mass, id)| (mass, id, destination))
        })
        .filter(|(_, _, destination)| {
            unguarded_unlit_cell(registries, state, junction, *destination, player_position)
        })
        .max_by_key(|(mass, id, _)| (*mass, Reverse(*id)))
        .map(|(mass, _, destination)| (mass, destination))
}

/// Seek finite natural food beyond one real intermediate route junction.
/// Candidate resources are position-indexed and checked before expensive light
/// and terrain resolution; each candidate uses only two adjacent graph edges.
fn best_far_forage(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
) -> Option<(Mass, VoxelCoord, VoxelCoord)> {
    two_edge_candidates(state, route)
        .filter_map(|(first, goal)| {
            best_local_forage(registries, state, goal, at, maximum_mass)
                .map(|(mass, id)| (mass, id, first, goal))
        })
        .filter(|(_, _, first, goal)| {
            far_route_is_sighted_and_safe(registries, state, route, (*first, *goal))
        })
        .max_by_key(|(mass, id, first, goal)| {
            (*mass, Reverse(*id), Reverse(*first), Reverse(*goal))
        })
        .map(|(mass, _, first, goal)| (mass, first, goal))
}

/// Nearby patches save travel, but a distant complete meal is more useful
/// than a local crumb when a single forage triggers the same recovery clock.
/// When both patches are insufficient, prefer the larger recoverable serving.
/// Retain the ranked yields instead of re-reading either selected source.
pub(in crate::kobolds) fn reachable_forage_route(
    registries: &Registries,
    state: &AppState,
    route: ScavengeRoute,
    at: SimulationTick,
    maximum_mass: Mass,
    meal_shortage: Mass,
) -> Option<(VoxelCoord, Option<VoxelCoord>)> {
    let side = best_adjacent_forage(registries, state, route, at, maximum_mass);
    if let Some((mass, first)) = side
        && mass >= meal_shortage
    {
        return Some((first, None));
    }
    let far = best_far_forage(registries, state, route, at, maximum_mass);
    match (side, far) {
        (Some((side_mass, _)), Some((far_mass, first, goal))) if far_mass > side_mass => {
            Some((first, Some(goal)))
        }
        (Some((_, first)), _) => Some((first, None)),
        (None, far) => far.map(|(_, first, goal)| (first, Some(goal))),
    }
}
