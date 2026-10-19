//! Renderer-neutral local illumination derived from finite located energy stores.

use std::collections::{BTreeMap, VecDeque};

use crate::core::arithmetic::checked_mul_div_ceil;
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::{CubeFace, VoxelCoord};

use super::EnergyStoreId;

/// One currently active located local-light emitter.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalLightSourceProjection {
    store: EnergyStoreId,
    position: VoxelCoord,
    level: u8,
}

impl LocalLightSourceProjection {
    #[must_use]
    pub const fn store(self) -> EnergyStoreId {
        self.store
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }
}

/// Current block-aware local-light assessment at one world voxel.
///
/// This is a read model, not persisted lighting state. Sky light, richer attenuation, meshing, and
/// visibility remain adapter/world concerns.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalIlluminationAssessment {
    position: VoxelCoord,
    level: u8,
    strongest_source: Option<EnergyStoreId>,
}

impl LocalIlluminationAssessment {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn level(self) -> u8 {
        self.level
    }

    #[must_use]
    pub const fn strongest_source(self) -> Option<EnergyStoreId> {
        self.strongest_source
    }
}

fn active_local_light_sources<'a>(
    registries: &'a Registries,
    state: &'a AppState,
) -> impl Iterator<Item = LocalLightSourceProjection> + 'a {
    state
        .logistics()
        .energy_store_locations()
        .filter_map(move |(store, position)| {
            let record = state.energy().get_store(store).unwrap_or_else(|| {
                panic!("runtime invariant broken: located energy store disappeared")
            });
            if record.stored().is_zero() {
                return None;
            }
            let definition = registries
                .energy()
                .get_store(record.definition())
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: energy definition disappeared")
                });
            let maximum_level = definition.local_light_level();
            if maximum_level == 0 {
                return None;
            }
            // A combustion emitter fades as its finite heat charge is spent. A smaller
            // alternative fuel charge (e.g. woodworking chips) starts dimmer instead of
            // producing a full-strength fire for a shorter, invisible burn horizon.
            // The primary authored fuel charge defines full brightness; stacked fuel
            // remains capped at that brightness. Noncombustion emitters use their capacity.
            let full_brightness_energy = definition
                .combustion_profile()
                .map_or(definition.capacity(), |profile| profile.released_energy());
            let level = checked_mul_div_ceil(
                record.stored().min(full_brightness_energy).nanojoules(),
                u128::from(maximum_level),
                full_brightness_energy.nanojoules(),
            )
            .and_then(|scaled| u8::try_from(scaled).ok())
            .unwrap_or_else(|| unreachable!("bounded finite emission fits authored light level"));
            Some(LocalLightSourceProjection {
                store,
                position,
                level,
            })
        })
}

/// Projects every currently active located local-light emitter in stable store-ID order.
///
/// The projection is read-only and contains presentation facts only. It does not grant energy,
/// logistics, interaction, or visibility authority.
#[must_use]
pub fn project_local_light_sources(
    registries: &Registries,
    state: &AppState,
) -> Vec<LocalLightSourceProjection> {
    active_local_light_sources(registries, state).collect()
}

/// Returns current bounded local illumination from nonempty located authored light sources.
///
/// Burning sources dim with remaining finite energy relative to their authored primary fuel
/// charge, so smaller alternative fuel and approaching burnout have observable consequences.
/// Source levels fall by one per face-adjacent generated non-opaque voxel. Opaque blocks and missing
/// world truth stop propagation. Overlapping sources use the strongest current contribution; equal
/// contributions retain the lowest stable store ID because logistics iterates locations in
/// store-ID order.
pub fn assess_local_illumination(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> LocalIlluminationAssessment {
    let mut level = 0_u8;
    let mut strongest_source = None;
    for source in active_local_light_sources(registries, state) {
        let contribution = propagated_light_level(registries, state, source, position);
        if contribution > level {
            level = contribution;
            strongest_source = Some(source.store());
        }
    }
    LocalIlluminationAssessment {
        position,
        level,
        strongest_source,
    }
}

const LIGHT_PROPAGATION_FACES: [CubeFace; 6] = [
    CubeFace::Top,
    CubeFace::Bottom,
    CubeFace::North,
    CubeFace::South,
    CubeFace::East,
    CubeFace::West,
];

fn propagated_light_level(
    registries: &Registries,
    state: &AppState,
    source: LocalLightSourceProjection,
    target: VoxelCoord,
) -> u8 {
    if source.position() == target {
        return source.level();
    }
    if manhattan_distance(source.position(), target) >= u128::from(source.level()) {
        return 0;
    }

    let mut reached = BTreeMap::from([(source.position(), source.level())]);
    let mut frontier = VecDeque::from([(source.position(), source.level())]);
    while let Some((position, level)) = frontier.pop_front() {
        if level <= 1 {
            continue;
        }
        let next_level = level - 1;
        for face in LIGHT_PROPAGATION_FACES {
            let Some(adjacent) = face.adjacent_voxel(position) else {
                continue;
            };
            if reached
                .get(&adjacent)
                .is_some_and(|existing| *existing >= next_level)
            {
                continue;
            }
            if !voxel_transmits_light(registries, state, adjacent) {
                continue;
            }
            if adjacent == target {
                return next_level;
            }
            reached.insert(adjacent, next_level);
            frontier.push_back((adjacent, next_level));
        }
    }
    0
}

fn voxel_transmits_light(registries: &Registries, state: &AppState, position: VoxelCoord) -> bool {
    let Some(block_state) = state.world().block_at(position) else {
        return false;
    };
    !registries
        .world()
        .block_for_state(block_state)
        .unwrap_or_else(|| panic!("runtime invariant broken: world block state disappeared"))
        .is_opaque()
}

fn manhattan_distance(a: VoxelCoord, b: VoxelCoord) -> u128 {
    let axis = |left: i64, right: i64| (i128::from(left) - i128::from(right)).unsigned_abs();
    axis(a.x(), b.x())
        .checked_add(axis(a.y(), b.y()))
        .and_then(|distance| distance.checked_add(axis(a.z(), b.z())))
        .unwrap_or(u128::MAX)
}

#[cfg(test)]
#[path = "illumination_tests.rs"]
mod tests;
