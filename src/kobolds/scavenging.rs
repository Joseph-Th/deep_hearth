//! Deterministic search for exposed scout loot and bounded recoverable portions.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

use super::feeding::{scout_wants_drop, scout_wants_food_opportunity};
use super::patrol::{scout_can_enter, scout_sees_guarded_cell};
use super::visibility::scout_visible_from;
use super::{DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL};

/// Survival food always precedes inert cargo, but mobilized scavengers use
/// inherited service-work knowledge to recognize shaped precision copper.
/// Stable surface identity breaks every equal-value tie.
fn scout_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    mobilized: bool,
) -> (bool, bool, SurfaceResourceId) {
    let not_food = resource.commodity().form() != crate::content::FORM_FOOD;
    (
        not_food,
        !(mobilized && is_servant_era_salvage(resource)),
        resource.id(),
    )
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
                && scout_wants_drop(registries, resource, at)
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
            (is_contestable_drop(resource, space) && scout_wants_drop(registries, resource, at))
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
            let loose =
                is_contestable_drop(resource, space) && scout_wants_drop(registries, resource, at);
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
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_portable_drop(resource)
                && resource.remaining_mass() <= space
                && scout_wants_drop(registries, resource, at)
                && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
        })
        .min_by_key(|resource| scout_loot_priority(resource, mobilized))
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
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_contestable_drop(resource, space)
                && scout_wants_drop(registries, resource, at)
                && (!food_only || resource.commodity().form() == crate::content::FORM_FOOD)
                && (!is_portable_drop(resource) || resource.remaining_mass() > space)
        })
        .min_by_key(|resource| scout_loot_priority(resource, mobilized))
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
/// Within each transfer-cost tier, edible loot wins, then stable surface identity.
fn detour_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
    capacity: Mass,
) -> (bool, bool, SurfaceResourceId) {
    (
        !is_portable_drop(resource) || resource.remaining_mass() > capacity,
        resource.commodity().form() != crate::content::FORM_FOOD,
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
                        && scout_wants_drop(registries, resource, at)
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
                                && scout_wants_drop(registries, resource, at)
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

/// When junction wild food is exhausted, investigate only adjacent traversable
/// food patches. Stable surface identity decides between qualifying patches.
pub(super) fn reachable_side_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
    at: SimulationTick,
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| *destination != den && *destination != authored_tip)
        .filter_map(|destination| {
            state
                .surface()
                .available_resources_at(Some(destination))
                .find(|resource| scout_wants_food_opportunity(registries, resource, at))
                .map(|resource| (resource.id(), destination))
        })
        .filter(|(_, destination)| {
            unguarded_unlit_cell(registries, state, junction, *destination, player_position)
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, destination)| destination)
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
                    state
                        .surface()
                        .available_resources_at(Some(goal))
                        .find(|source| scout_wants_food_opportunity(registries, source, at))
                        .map(|source| (source.id(), first, goal))
                })
        })
        .filter(|(_, first, goal)| {
            scout_visible_from(registries, state, junction, *goal)
                && unguarded_unlit_cell(registries, state, junction, *first, player_position)
                && unguarded_unlit_cell(registries, state, junction, *goal, player_position)
        })
        .min_by_key(|(id, first, goal)| (*id, *first, *goal))
        .map(|(_, first, goal)| (first, goal))
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
) -> Option<(bool, SurfaceResourceId)> {
    state
        .surface()
        .available_resources_at(Some(position))
        .filter(|resource| {
            is_servant_era_salvage(resource)
                && is_contestable_drop(resource, space)
                && scout_wants_drop(registries, resource, at)
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
    let side = state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| *first != den && *first != authored_tip)
        .filter_map(|first| {
            engineered_drop_at(registries, state, first, capacity, at)
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
                    engineered_drop_at(registries, state, goal, capacity, at)
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
