//! Terrain, footing, vegetation, and resident clearance for enclosures.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM, exposed_residue_compact_depth_um};

use super::super::StockpileId;
use super::StorageEnclosureConstructionError;

/// Ordinary construction in generated world truth requires a collidable floor. Controlled
/// ungenerated-space stockpiles have no physical terrain authority yet.
pub(super) fn generated_enclosure_has_ground_support(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    if state.world().block_at(position).is_none() {
        return true;
    }
    position
        .y()
        .checked_sub(1)
        .and_then(|y| {
            state
                .world()
                .block_at(VoxelCoord::new(position.x(), y, position.z()))
        })
        .and_then(|block| registries.world().block_for_state(block))
        .is_some_and(|block| block.collision() == crate::world::BlockCollisionShape::FullCube)
}

pub(super) fn validate_world_location(
    registries: &Registries,
    state: &AppState,
    target: StockpileId,
    source: StockpileId,
) -> Result<(), StorageEnclosureConstructionError> {
    let Some(target_position) = state.logistics().stationary_stockpile_position(target) else {
        return Ok(());
    };
    let Some(source_position) = state.logistics().stockpile_position(source) else {
        return Err(
            StorageEnclosureConstructionError::LocatedTargetSourceUnlocated {
                target,
                target_position,
                source,
            },
        );
    };
    if source_position != target_position {
        return Err(
            StorageEnclosureConstructionError::LocatedTargetSourceRemote {
                target,
                target_position,
                source,
                source_position,
            },
        );
    }
    if enclosure_position_is_solid(registries, state, target_position) {
        return Err(StorageEnclosureConstructionError::GeneratedSolidTerrain {
            position: target_position,
        });
    }
    if rooted_foraging_at(state, target_position) {
        return Err(
            StorageEnclosureConstructionError::RootedVegetationOccupiesSite {
                position: target_position,
            },
        );
    }
    // In-place construction must have the same exposed-residue foundation
    // clearance as new enclosure placement at this exact voxel.
    let compact_depth_um = exposed_residue_compact_depth_um(registries, state, target_position);
    if compact_depth_um >= OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM {
        return Err(StorageEnclosureConstructionError::LooseResidueFooting {
            position: target_position,
            compact_depth_um,
        });
    }
    if !generated_enclosure_has_ground_support(registries, state, target_position) {
        return Err(StorageEnclosureConstructionError::UnsupportedFloor {
            position: target_position,
        });
    }
    Ok(())
}

/// Harvested patches with live root reserves still occupy their sites; a
/// rootless and fruitless provenance receipt does not. Query the surface
/// owner's exact position index rather than all perennial sites in the world.
pub(super) fn rooted_foraging_at(state: &AppState, position: VoxelCoord) -> bool {
    state.surface().rooted_biomass_occupies_voxel(position)
}

pub(super) fn enclosure_position_is_solid(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> bool {
    state
        .world()
        .full_cell_collision_at(registries.world(), position)
        == Some(true)
}
