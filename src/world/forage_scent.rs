//! Windborne food cues through bounded, physically open generated terrain.
//!
//! Unlike short-range sight, a scent can follow an air detour around a trunk.
//! It never supplies remote coordinates, exact material custody, or freshness.

use std::collections::{BTreeSet, VecDeque};

use crate::content::{MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::surface::SurfaceResourceKind;
use crate::survival::{FoodFreshness, Vitality, assess_surface_food_freshness};

use super::weather::{REGION_SIDE_VOXELS, local_airflow_at};
use super::{BlockCollisionShape, WindCondition, WindDirection, assess_player_weather};

const MAX_AIR_STEPS: u8 = 8;
const CALM_AIR_STEPS: u8 = 3;
/// Odor immediately around a source disperses through open air even upwind.
/// Beyond this local envelope, only the directional wind plume is detectable.
const LOCAL_DIFFUSION_STEPS: u8 = 2;
const BREEZE_AIR_STEPS: u8 = 6;
const MAX_VERTICAL_DEVIATION: u64 = 2;
const STRONG_SCENT_STEPS: u8 = 4;
const DISTINCT_SOURCE_MASS_MG: u64 = 250_000;

fn scent_reach(wind: WindCondition) -> u8 {
    match wind {
        WindCondition::Calm => CALM_AIR_STEPS,
        WindCondition::Breeze => BREEZE_AIR_STEPS,
        WindCondition::Gusting => MAX_AIR_STEPS,
    }
}

/// General character of a plant smell. No source identity or position is disclosed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForageScentKind {
    SweetFruit,
    WildNuts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForageScentIntensity {
    Faint,
    Distinct,
}

/// One player's ambiguous sensory clue. Several physical patches of the
/// same kind contribute to a single cue; it is never a remote resource handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForageScent {
    kind: ForageScentKind,
    intensity: ForageScentIntensity,
}

impl ForageScent {
    #[must_use]
    pub const fn kind(self) -> ForageScentKind {
        self.kind
    }

    #[must_use]
    pub const fn intensity(self) -> ForageScentIntensity {
        self.intensity
    }
}

fn passable_air(registries: &Registries, state: &AppState, at: VoxelCoord) -> bool {
    !state.structures().blocks_full_cell(at)
        && state.world().tree_trunk_at(at).is_none()
        && state
            .world()
            .block_at(at)
            .and_then(|state_id| registries.world().block_for_state(state_id))
            .is_some_and(|block| block.collision() == BlockCollisionShape::Empty)
}

/// Avoid searching air corridors when no edible, fresh indexed source lies
/// within the maximum eight-step smell envelope. This is the same freshness
/// gate as actual scent emission, not a second classification policy.
/// Coordinate-bound failures must not conceal a legitimate scent near the edge.
fn potentially_scented_forage_nearby(
    registries: &Registries,
    state: &AppState,
    observer: VoxelCoord,
) -> bool {
    let radius = i64::from(MAX_AIR_STEPS);
    let vertical = MAX_VERTICAL_DEVIATION as i64;
    let limits = (
        observer.x().checked_sub(radius),
        observer.y().checked_sub(vertical),
        observer.z().checked_sub(radius),
        observer.x().checked_add(radius + 1),
        observer.y().checked_add(vertical + 1),
        observer.z().checked_add(radius + 1),
    );
    let (Some(min_x), Some(min_y), Some(min_z), Some(max_x), Some(max_y), Some(max_z)) = limits
    else {
        return true;
    };
    let Ok(bounds) = VoxelBounds::new(
        VoxelCoord::new(min_x, min_y, min_z),
        VoxelCoord::new(max_x, max_y, max_z),
    ) else {
        return true;
    };
    state
        .surface()
        .available_resources_in_bounds(bounds)
        .any(|resource| {
            resource.kind() == SurfaceResourceKind::Opportunity
                && matches!(
                    resource.commodity().material(),
                    MATERIAL_BERRIES | MATERIAL_WILD_NUTS
                )
                && matches!(
                    assess_surface_food_freshness(registries, resource, state.tick()),
                    Some(FoodFreshness::Fresh { .. })
                )
        })
}

fn scent_reaches_player(
    source: VoxelCoord,
    player: VoxelCoord,
    wind_direction: Option<WindDirection>,
    air_steps: u8,
) -> bool {
    // Local diffusion is not a long-distance wind clue. Never make food
    // one or two open-air steps away inexplicably odorless on its upwind side.
    if air_steps <= LOCAL_DIFFUSION_STEPS {
        return true;
    }
    let Some(direction) = wind_direction else {
        return air_steps <= CALM_AIR_STEPS;
    };
    let (wx, wz) = direction.delta();
    // Relative axes are differences of signed voxel coordinates. Use i128 to
    // prevent extreme world positions from overflowing a weather query.
    let dx = i128::from(player.x()) - i128::from(source.x());
    let dz = i128::from(player.z()) - i128::from(source.z());
    let along = dx * i128::from(wx) + dz * i128::from(wz);
    let across = (dx * i128::from(wz) - dz * i128::from(wx)).abs();
    // The downwind fan widens by one cell for every two along-wind steps.
    // A calm day has a short local scent, but an upwind long-distance player
    // never receives a source's hidden coordinates by searching every chunk.
    along >= 0 && across <= (along / 2 + 1)
}

fn live_food_scent(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
    distance: u8,
    observer: VoxelCoord,
) -> impl Iterator<Item = ForageScent> {
    state
        .surface()
        .available_resources_at(Some(position))
        .filter_map(move |resource| {
            if resource.kind() != SurfaceResourceKind::Opportunity {
                return None;
            }
            let kind = match resource.commodity().material() {
                MATERIAL_BERRIES => ForageScentKind::SweetFruit,
                MATERIAL_WILD_NUTS => ForageScentKind::WildNuts,
                _ => return None,
            };
            if !matches!(
                assess_surface_food_freshness(registries, resource, state.tick()),
                Some(FoodFreshness::Fresh { .. })
            ) {
                return None;
            }
            // Real sky exposure or an unobstructed upwind inlet can carry
            // the regional plume. A sealed source retains only local diffusion;
            // a one-cell awning alone does not make horizontal wind disappear.
            let (wind, direction) = local_airflow_at(registries, state, position, state.tick())
                .map_or((WindCondition::Calm, None), |weather| {
                    (weather.wind(), weather.wind_direction())
                });
            if distance > scent_reach(wind)
                || !scent_reaches_player(position, observer, direction, distance)
            {
                return None;
            }
            Some(ForageScent {
                kind,
                intensity: if distance <= STRONG_SCENT_STEPS
                    && resource.remaining_mass().milligrams() >= DISTINCT_SOURCE_MASS_MG
                {
                    ForageScentIntensity::Distinct
                } else {
                    ForageScentIntensity::Faint
                },
            })
        })
}

/// Current sensory cues from living, finite, unspoiled food patches.
///
/// A bounded search follows at most eight face-connected open voxels in
/// generated terrain, including around full-cell walls and trees. Unknown
/// chunks are opaque; no global forage lookup, terrain creation, or retained
/// discovery flag is involved. Wind direction gates distant scent and its
/// strength changes with distance and depletion. Returns at most two
/// category-level clues, never source positions or IDs.
#[must_use]
pub fn assess_player_forage_scents(registries: &Registries, state: &AppState) -> Vec<ForageScent> {
    let Some(player) = state.logistics().player() else {
        return Vec::new();
    };
    if state
        .survival()
        .player()
        .is_none_or(|survival| survival.vitality() == Vitality::ZERO)
    {
        return Vec::new();
    }
    let start = player.position();
    // Empty clearings need neither a weather lookup nor bounded air traversal.
    if !potentially_scented_forage_nearby(registries, state, start) {
        return Vec::new();
    }
    let Some(weather) = assess_player_weather(registries, state) else {
        return Vec::new();
    };
    if !passable_air(registries, state, start) {
        return Vec::new();
    }
    // A source up to eight air steps away can have a different front near a
    // regional border. Elsewhere the observer's shorter range remains exact.
    let near_frontier = [start.x(), start.z()].into_iter().any(|axis| {
        let within_region = axis.rem_euclid(REGION_SIDE_VOXELS);
        within_region < i64::from(MAX_AIR_STEPS)
            || within_region >= REGION_SIDE_VOXELS - i64::from(MAX_AIR_STEPS)
    });
    let reach = if near_frontier {
        MAX_AIR_STEPS
    } else {
        scent_reach(weather.wind())
    };
    let mut visited = BTreeSet::from([start]);
    let mut queue = VecDeque::from([(start, 0u8)]);
    let mut best: [Option<ForageScentIntensity>; 2] = [None, None];
    while let Some((voxel, steps)) = queue.pop_front() {
        for scent in live_food_scent(registries, state, voxel, steps, start) {
            let index = match scent.kind {
                ForageScentKind::SweetFruit => 0,
                ForageScentKind::WildNuts => 1,
            };
            best[index] = Some(best[index].map_or(scent.intensity, |old| old.max(scent.intensity)));
        }
        // Both source categories already have the strongest possible cue.
        // Further air traversal cannot improve either player-facing observation.
        if best == [Some(ForageScentIntensity::Distinct); 2] {
            break;
        }
        if steps == reach {
            continue;
        }
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 0, 1),
            (0, 0, -1),
            (0, 1, 0),
            (0, -1, 0),
        ] {
            let (Some(x), Some(y), Some(z)) = (
                voxel.x().checked_add(dx),
                voxel.y().checked_add(dy),
                voxel.z().checked_add(dz),
            ) else {
                continue;
            };
            let neighbor = VoxelCoord::new(x, y, z);
            if start.y().abs_diff(y) > MAX_VERTICAL_DEVIATION
                || visited.contains(&neighbor)
                || !passable_air(registries, state, neighbor)
            {
                continue;
            }
            visited.insert(neighbor);
            queue.push_back((neighbor, steps + 1));
        }
    }
    [
        (ForageScentKind::SweetFruit, best[0]),
        (ForageScentKind::WildNuts, best[1]),
    ]
    .into_iter()
    .filter_map(|(kind, intensity)| intensity.map(|intensity| ForageScent { kind, intensity }))
    .collect()
}

#[cfg(test)]
#[path = "forage_scent_tests.rs"]
mod tests;
