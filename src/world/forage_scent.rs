//! Windborne food cues through bounded, physically open generated terrain.
//!
//! Unlike short-range sight, a scent can follow an air detour around a trunk.
//! It never supplies remote coordinates, exact material custody, or freshness.

use std::collections::{BTreeSet, VecDeque};

use crate::content::{MATERIAL_BERRIES, MATERIAL_WILD_NUTS};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::SurfaceResourceKind;
use crate::survival::{FoodFreshness, Vitality, freshness_from_history};

use super::weather::{REGION_SIDE_VOXELS, weather_at};
use super::{BlockCollisionShape, WindCondition, WindDirection, assess_player_weather};

const MAX_AIR_STEPS: u8 = 8;
const CALM_AIR_STEPS: u8 = 3;
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

fn scent_reaches_player(
    source: VoxelCoord,
    player: VoxelCoord,
    wind_direction: Option<WindDirection>,
    air_steps: u8,
) -> bool {
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
    world_seed: u64,
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
            let food = registries.survival().get_food(resource.commodity())?;
            let history = resource.ambient_storage_history_at(state.tick())?;
            if !matches!(
                freshness_from_history(
                    history,
                    crate::inventory::AMBIENT_PRESERVATION_MULTIPLIER_PPM,
                    state.tick(),
                    food.shelf_life(),
                ),
                Ok(FoodFreshness::Fresh { .. })
            ) {
                return None;
            }
            // Source-local wind controls emission, even when the observer is
            // across a regional weather boundary with an opposing front.
            let source_weather = weather_at(registries, world_seed, position, state.tick());
            if distance > scent_reach(source_weather.wind())
                || !scent_reaches_player(
                    position,
                    observer,
                    source_weather.wind_direction(),
                    distance,
                )
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
    let Some(weather) = assess_player_weather(registries, state) else {
        return Vec::new();
    };
    let world_seed = state
        .world()
        .generation_profile()
        .expect("observable generated weather requires a generation profile")
        .seed()
        .value();
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
        for scent in live_food_scent(registries, state, voxel, steps, world_seed, start) {
            let index = match scent.kind {
                ForageScentKind::SweetFruit => 0,
                ForageScentKind::WildNuts => 1,
            };
            best[index] = Some(best[index].map_or(scent.intensity, |old| old.max(scent.intensity)));
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
