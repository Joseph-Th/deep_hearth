//! Deterministic search for exposed scout loot and bounded recoverable portions.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceId, SurfaceResourceKind, SurfaceResourceLifecycle};

use super::patrol::scout_can_enter;
use super::{DEN_LOOSE_CACHE_LIMIT, SCOUT_CARRY_LIMIT, SCOUT_DETERRENT_LIGHT_LEVEL};

/// Food takes precedence over inert loose cargo at equal route distance.
/// The stable surface ID breaks ties without introducing random state.
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

/// From the junction, exposed loose matter on a traversable neighboring branch
/// can displace the routine patrol stop. Portable parcels move whole; bulky piles
/// are split on retreat, bounded by finite den capacity. Candidate order is
/// stable surface ID. Detours still follow actual edges and pass physical access.
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
        .surface()
        .resources()
        .filter(|resource| {
            let destination = resource.position();
            is_contestable_drop(resource, capacity)
                && destination != den
                && destination != junction
                && destination != authored_tip
                && state.logistics().has_traversal_edge(junction, destination)
                && player_position != Some(destination)
                && assess_local_illumination(registries, state, destination).level()
                    < SCOUT_DETERRENT_LIGHT_LEVEL
                && (state.world().generation_profile().is_none()
                    || state
                        .world()
                        .has_standing_clearance(registries.world(), destination))
        })
        .min_by_key(|resource| scout_loot_priority(resource))
        .map(|resource| resource.position())
}

/// Search one further authored traversal edge only after no directly adjacent
/// unattended loose pile was found. The same stable resource order picks
/// the goal; the first physically clear, unlit intermediate path wins.
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
        .surface()
        .resources()
        .filter_map(|resource| {
            let goal = resource.position();
            if !is_contestable_drop(resource, capacity)
                || goal == den
                || goal == junction
                || goal == authored_tip
                || player_position == Some(goal)
                || assess_local_illumination(registries, state, goal).level()
                    >= SCOUT_DETERRENT_LIGHT_LEVEL
                || (state.world().generation_profile().is_some()
                    && !state
                        .world()
                        .has_standing_clearance(registries.world(), goal))
            {
                return None;
            }
            state
                .logistics()
                .traversable_destinations(junction)
                .find(|first| {
                    *first != den
                        && *first != authored_tip
                        && *first != goal
                        && player_position != Some(*first)
                        && state.logistics().has_traversal_edge(*first, goal)
                        && assess_local_illumination(registries, state, *first).level()
                            < SCOUT_DETERRENT_LIGHT_LEVEL
                        && (state.world().generation_profile().is_none()
                            || state
                                .world()
                                .has_standing_clearance(registries.world(), *first))
                })
                .map(|first| (scout_loot_priority(resource), first, goal))
        })
        .min_by_key(|(priority, _, _)| *priority)
        .map(|(_, first, goal)| (first, goal))
}

/// When the junction's own wild food is exhausted, investigate one reachable
/// neighboring food patch. Foraging stays source-owned and physically adjacent:
/// the scout creates its catch only while retreating from the patch.
pub(super) fn reachable_side_food(
    registries: &Registries,
    state: &AppState,
    junction: VoxelCoord,
    den: VoxelCoord,
    authored_tip: VoxelCoord,
    player_position: Option<VoxelCoord>,
) -> Option<VoxelCoord> {
    state.surface().resources().find_map(|resource| {
        let destination = resource.position();
        (resource.kind() == SurfaceResourceKind::Opportunity
            && resource.lifecycle() == SurfaceResourceLifecycle::Available
            && resource.commodity().form() == crate::content::FORM_FOOD
            && !resource.remaining_mass().is_zero()
            && destination != den
            && destination != junction
            && destination != authored_tip
            && state.logistics().has_traversal_edge(junction, destination)
            && scout_can_enter(registries, state, destination, player_position)
            && assess_local_illumination(registries, state, destination).level()
                < SCOUT_DETERRENT_LIGHT_LEVEL)
            .then_some(destination)
    })
}
