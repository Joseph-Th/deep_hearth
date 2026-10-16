//! Renderer-neutral local illumination derived from finite located energy stores.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;

use super::EnergyStoreId;

/// Current unoccluded local block-light assessment at one world voxel.
///
/// This is a read model, not persisted lighting state. General propagation, occlusion, sky light,
/// meshing, and visibility remain adapter/world concerns.
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

/// Returns current unoccluded local illumination from nonempty located authored light sources.
///
/// Source levels fall by one per Manhattan voxel. Overlapping sources use the strongest current
/// contribution; equal contributions retain the lowest stable store ID because logistics iterates
/// locations in store-ID order.
pub fn assess_local_illumination(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> LocalIlluminationAssessment {
    let mut level = 0_u8;
    let mut strongest_source = None;
    for (store, source_position) in state.logistics().energy_store_locations() {
        let record = state.energy().get_store(store).unwrap_or_else(|| {
            panic!("runtime invariant broken: located energy store disappeared")
        });
        if record.stored().is_zero() {
            continue;
        }
        let definition = registries
            .energy()
            .get_store(record.definition())
            .unwrap_or_else(|| panic!("runtime invariant broken: energy definition disappeared"));
        let source_level = definition.local_light_level();
        if source_level == 0 {
            continue;
        }
        let distance = manhattan_distance(source_position, position);
        if distance >= u128::from(source_level) {
            continue;
        }
        let contribution = source_level
            .checked_sub(u8::try_from(distance).unwrap_or_else(|_| {
                unreachable!("contributing light distance is bounded by a u8 source level")
            }))
            .unwrap_or_else(|| unreachable!("light contribution distance is below source level"));
        if contribution > level {
            level = contribution;
            strongest_source = Some(store);
        }
    }
    LocalIlluminationAssessment {
        position,
        level,
        strongest_source,
    }
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
