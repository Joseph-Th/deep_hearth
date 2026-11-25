//! Physical den discard and finite-cache eviction for fresh food and salvaged machinery.

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::core::time::SimulationTick;
use crate::energy::assess_local_illumination;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceResourceId, SurfaceResourceKind, SurfaceResourceRecord, relocate_scavenged_drop_step,
    resource_reserved_for_player_gathering, split_bulk_scavenged_drop_step,
};
use crate::survival::{FoodFreshness, assess_surface_food_freshness};

use super::scout_wants_food_opportunity;
use crate::kobolds::junkworks::current_den_rig;
use crate::kobolds::patrol::{
    opening_resident_can_depart, scout_can_approach_visible_unlit_cell, scout_can_enter,
    scout_sees_guarded_cell,
};
use crate::kobolds::scavenging::{
    DenCacheOccupancy, ScavengeRoute, is_contestable_drop, is_portable_drop,
    is_servant_era_salvage, survey_visible_inherited_salvage, two_edge_candidates,
};
use crate::kobolds::{
    SCOUT_BULK_SPLIT_BATCH, SCOUT_DETERRENT_LIGHT_LEVEL, SCOUT_MEAL_MASS,
    SCOUT_SAFE_SCAVENGE_TEMPERATURE,
};

/// One real den-to-junction transfer. The quantity used to rank the release
/// is exactly the quantity the surface owner will move or split.
#[derive(Clone, Copy)]
struct DenEviction {
    source: SurfaceResourceId,
    released: Mass,
    split: bool,
}

impl DenEviction {
    fn from_drop(resource: &SurfaceResourceRecord) -> Self {
        let split = !is_portable_drop(resource);
        Self {
            source: resource.id(),
            released: if split {
                SCOUT_BULK_SPLIT_BATCH
            } else {
                resource.remaining_mass()
            },
            split,
        }
    }

    fn commit(
        self,
        registries: &Registries,
        state: &mut AppState,
        den: VoxelCoord,
        junction: VoxelCoord,
        at: SimulationTick,
    ) -> Option<SurfaceResourceId> {
        if self.split {
            split_bulk_scavenged_drop_step(
                registries,
                state,
                self.source,
                den,
                junction,
                at,
                self.released,
            )
            .map(|(daughter, _)| daughter)
        } else {
            relocate_scavenged_drop_step(registries, state, self.source, den, junction)
                .then_some(self.source)
        }
    }
}

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
    let release = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && !resource_reserved_for_player_gathering(state, resource.id())
                && resource.kind() == SurfaceResourceKind::Drop
                && registries.survival().get_food(resource.commodity()).is_some()
                && !matches!(
                    assess_surface_food_freshness(registries, resource, at),
                    Some(FoodFreshness::Fresh { .. })
                )
        })
        // Whole cargo requires no new identity or split, so clear it first.
        .map(DenEviction::from_drop)
        .min_by_key(|release| (release.split, release.source))?;
    release.commit(registries, state, den, junction, at)
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
    let occupancy = DenCacheOccupancy::at(state, den);
    let free = occupancy.free();
    if free >= meal_shortage || !den_exit_is_safe(registries, state, den, junction) {
        return None;
    }
    // Some space is not necessarily *enough* space: a missed five-gram meal
    // must not leave the scout endlessly foraging five-gram portions while
    // the rest of the den is inert cargo. Consider only recoverable food seen
    // from home, and only evict stock when that food exceeds free capacity.
    let colony = state.kobold_state().colony.as_ref()?;
    let forage_ready = colony.next_forage_at.is_none_or(|due| at >= due);
    let player = state.logistics().player().map(|record| record.position());
    let route = ScavengeRoute::new(colony.route, player);
    // Assess the same walking envelope as the later forage decision, but
    // only through the scout's actual view from the den. A sighted goal
    // cannot justify eviction through an obstructed or unseen first edge.
    let safe =
        |position| scout_can_approach_visible_unlit_cell(registries, state, den, position, player);
    let adjacent = state
        .logistics()
        .traversable_destinations(junction)
        .filter(|position| *position != den && safe(*position));
    let far = two_edge_candidates(state, route)
        .filter(|(first, goal)| safe(*first) && safe(*goal))
        .map(|(_, goal)| goal);
    let wanted = std::iter::once(junction)
        .filter(|position| safe(*position))
        .chain(adjacent)
        .chain(far)
        .flat_map(|position| state.surface().available_resources_at(Some(position)))
        .filter(|resource| !resource_reserved_for_player_gathering(state, resource.id()))
        .filter(|resource| match resource.kind() {
            SurfaceResourceKind::Opportunity => {
                forage_ready && scout_wants_food_opportunity(registries, resource, at)
            }
            SurfaceResourceKind::Drop => {
                is_contestable_drop(resource, SCOUT_MEAL_MASS)
                    && matches!(
                        assess_surface_food_freshness(registries, resource, at),
                        Some(FoodFreshness::Fresh { .. })
                    )
            }
        })
        .map(|resource| resource.remaining_mass().min(meal_shortage))
        .max()?;
    if wanted <= free {
        return None;
    }
    let missing = occupancy.required_release_for(wanted);
    // A working warning rig is real den infrastructure, even though its parts
    // remain loose recoverable parcels. Spend expendable cargo first, but let
    // an essential meal displace a rig component if nothing else can fit it.
    let active_rig = state
        .kobold_state()
        .colony
        .as_ref()
        .and_then(|colony| current_den_rig(state, colony));
    let release = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && !resource_reserved_for_player_gathering(state, resource.id())
                && resource.kind() == SurfaceResourceKind::Drop
                && registries.survival().get_food(resource.commodity()).is_none()
        })
        // A release sufficient for the meal wins. Protect existing infrastructure,
        // then minimize sufficient releases or maximize progress when none suffices.
        // An intact transfer wins a tie, not at the expense of a usable serving.
        .map(|resource| {
            let release = DenEviction::from_drop(resource);
            let mass = release.released.milligrams();
            let insufficient = u128::from(mass) < missing;
            let supports_alarm = active_rig.is_some_and(|rig| {
                resource.id() == rig.core() || resource.id() == rig.striker()
            });
            (
                (
                    insufficient,
                    supports_alarm,
                    is_servant_era_salvage(resource),
                    if insufficient { u64::MAX - mass } else { mass },
                    release.split,
                    release.source,
                ),
                release,
            )
        })
        .min_by_key(|(priority, _)| *priority)?
        .1;
    release.commit(registries, state, den, junction, at)
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
    // Industrial preparation must obey the same hydration limit as the
    // actual raid. Den care runs after local drinking, so a completed drink
    // can restore this option immediately without delaying food eviction.
    if crate::kobolds::hydration::prolonged_water_deprivation(colony, at) {
        return None;
    }
    let route = ScavengeRoute::new(
        colony.route,
        state.logistics().player().map(|record| record.position()),
    );
    let protected_striker = current_den_rig(state, colony).map(|rig| rig.striker());
    if !den_exit_is_safe(registries, state, den, junction) {
        return None;
    }
    let occupancy = DenCacheOccupancy::at(state, den);
    let salvage = survey_visible_inherited_salvage(registries, state, route, den, at)?;
    // One survey suffices for every candidate release. Never expel local
    // custody merely to take a larger haul if a whole component already fits.
    if salvage.whole_fits(occupancy.free()) {
        return None;
    }
    // Prefer the smallest physical release that fits an intact component.
    // If none suffices, maximize real progress, even in an overloaded den.
    let release = state
        .surface()
        .available_resources_at(Some(den))
        .filter(|resource| {
            resource.temperature() <= SCOUT_SAFE_SCAVENGE_TEMPERATURE
                && !resource_reserved_for_player_gathering(state, resource.id())
                && resource.kind() == SurfaceResourceKind::Drop
                && registries
                    .survival()
                    .get_food(resource.commodity())
                    .is_none()
                && !is_servant_era_salvage(resource)
                && Some(resource.id()) != protected_striker
        })
        .map(|resource| {
            let release = DenEviction::from_drop(resource);
            let whole = salvage.whole_fits(occupancy.free_after_release(release.released));
            let mass = release.released.milligrams();
            (
                (
                    !whole,
                    if whole { mass } else { u64::MAX - mass },
                    release.split,
                    release.source,
                ),
                release,
            )
        })
        .min_by_key(|(priority, _)| *priority)?
        .1;
    release.commit(registries, state, den, junction, at)
}

fn den_exit_is_safe(
    registries: &Registries,
    state: &AppState,
    den: VoxelCoord,
    junction: VoxelCoord,
) -> bool {
    // Den disposal and cache eviction are physical cargo moves performed by
    // the resident, not remote surface transfers. They need the same source
    // footing as an ordinary patrol edge, even if the junction is still clear.
    if !opening_resident_can_depart(registries, state, den) {
        return false;
    }
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
