//! Deterministic search for exposed scout loot and bounded recoverable portions.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

use super::patrol::scout_can_enter;
use super::{DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL};

/// Food takes precedence within one physical transfer tier at the same source.
/// Stable surface identity breaks ties without introducing random state.
pub(super) fn scout_loot_priority(
    resource: &crate::surface::SurfaceResourceRecord,
) -> (bool, SurfaceResourceId) {
    (
        resource.commodity().form() != crate::content::FORM_FOOD,
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
    position: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> bool {
    scout_can_enter(registries, state, position, player_position)
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
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| {
            *destination != den
                && *destination != authored_tip
                && unguarded_unlit_cell(registries, state, *destination, player_position)
        })
        .filter_map(|destination| {
            state
                .surface()
                .available_resources_at(Some(destination))
                .filter(|resource| is_contestable_drop(resource, capacity))
                .min_by_key(|resource| detour_loot_priority(resource, capacity))
                .map(|resource| (detour_loot_priority(resource, capacity), destination))
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
) -> Option<(VoxelCoord, VoxelCoord)> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|first| {
            *first != den
                && *first != authored_tip
                && unguarded_unlit_cell(registries, state, *first, player_position)
        })
        .flat_map(|first| {
            state
                .logistics()
                .traversable_destinations(first)
                .filter(move |goal| {
                    *goal != first
                        && *goal != den
                        && *goal != junction
                        && *goal != authored_tip
                        && unguarded_unlit_cell(registries, state, *goal, player_position)
                })
                .filter_map(move |goal| {
                    state
                        .surface()
                        .available_resources_at(Some(goal))
                        .filter(|resource| is_contestable_drop(resource, capacity))
                        .min_by_key(|resource| detour_loot_priority(resource, capacity))
                        .map(|resource| (detour_loot_priority(resource, capacity), first, goal))
                })
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
) -> Option<VoxelCoord> {
    state
        .logistics()
        .traversable_destinations(junction)
        .filter(|destination| {
            *destination != den
                && *destination != authored_tip
                && unguarded_unlit_cell(registries, state, *destination, player_position)
        })
        .filter_map(|destination| {
            state
                .surface()
                .available_resources_at(Some(destination))
                .find(|resource| {
                    resource.kind() == SurfaceResourceKind::Opportunity
                        && resource.commodity().form() == crate::content::FORM_FOOD
                })
                .map(|resource| (resource.id(), destination))
        })
        .min_by_key(|(id, _)| *id)
        .map(|(_, destination)| destination)
}

#[cfg(test)]
#[path = "scavenging_tests.rs"]
mod tests;
