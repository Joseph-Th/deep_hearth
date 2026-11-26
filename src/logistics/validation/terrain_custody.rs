//! Trusted-load reconciliation of stationary matter against generated block truth.
//!
//! Location identity is owned by logistics; each physical-occupancy predicate
//! reads its own authoritative matter owner. Empty ground allocations and
//! abstract energy fixtures remain nonphysical.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::world::BlockCollisionShape;

use super::LogisticsValidationError;

fn solid_terrain_at(registries: &Registries, state: &AppState, at: VoxelCoord) -> bool {
    state
        .world()
        .block_at(at)
        .and_then(|block| registries.world().block_for_state(block))
        .is_some_and(|block| block.collision() == BlockCollisionShape::FullCube)
}

pub(super) fn validate_ground_terrain(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LogisticsValidationError> {
    if let Some(voxel) = crate::world::first_buried_stationary_enclosure_in_world(registries, state)
    {
        return Err(LogisticsValidationError::StationaryEnclosureIntersectsSolidTerrain { voxel });
    }
    // Empty allocations have no footprint; actual stored matter and material
    // already reserved for an incoming delivery must both stay clear of rock.
    if let Some((_, voxel)) = state
        .logistics()
        .stockpile_locations()
        .find(|&(id, voxel)| {
            state.inventory().get_stockpile(id).is_some_and(|pile| {
                pile.enclosure().is_none()
                    && (!pile.stored_mass().is_zero() || !pile.reserved_inbound().is_zero())
            }) && solid_terrain_at(registries, state, voxel)
        })
    {
        return Err(LogisticsValidationError::GroundMatterIntersectsSolidTerrain { voxel });
    }
    for (stockpile, _) in state.logistics().stockpile_locations() {
        if state
            .inventory()
            .get_stockpile(stockpile)
            .is_some_and(|pile| !pile.reserved_inbound().is_zero())
            && let Some(voxel) =
                crate::world::unsupported_loose_stockpile_position(registries, state, stockpile)
        {
            return Err(
                LogisticsValidationError::ReservedGroundMatterRequiresFixedFloor {
                    stockpile,
                    voxel,
                },
            );
        }
    }
    Ok(())
}

pub(super) fn validate_equipment_terrain(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LogisticsValidationError> {
    if let Some((_, voxel)) = state
        .logistics()
        .equipment_locations()
        .find(|&(id, voxel)| {
            state
                .equipment()
                .get_equipment(id)
                .is_some_and(|record| !record.embodied_mass().is_zero())
                && solid_terrain_at(registries, state, voxel)
        })
    {
        return Err(LogisticsValidationError::LocatedEquipmentIntersectsSolidTerrain { voxel });
    }
    if let Some((equipment, voxel)) =
        state
            .logistics()
            .equipment_locations()
            .find(|&(id, voxel)| {
                crate::world::equipment_requires_fixed_footing(state, id)
                    && crate::world::generated_position_lacks_fixed_footing(
                        registries, state, voxel,
                    )
            })
    {
        return Err(
            LogisticsValidationError::LocatedEquipmentRequiresFixedFloor { equipment, voxel },
        );
    }
    Ok(())
}

pub(super) fn validate_energy_terrain(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LogisticsValidationError> {
    if let Some((_, voxel)) = state
        .logistics()
        .energy_store_locations()
        .find(|&(id, voxel)| {
            state
                .energy()
                .get_store(id)
                .is_some_and(|record| !record.embodied_material().is_empty())
                && solid_terrain_at(registries, state, voxel)
        })
    {
        return Err(LogisticsValidationError::LocatedEnergyStoreIntersectsSolidTerrain { voxel });
    }
    if let Some((store, voxel)) = state
        .logistics()
        .energy_store_locations()
        .find(|&(id, voxel)| {
            crate::world::energy_store_requires_fixed_footing(state, id)
                && crate::world::generated_position_lacks_fixed_footing(registries, state, voxel)
        })
    {
        return Err(
            LogisticsValidationError::LocatedEnergyStoreRequiresFixedFloor { store, voxel },
        );
    }
    Ok(())
}

pub(super) fn validate_fluid_terrain(
    registries: &Registries,
    state: &AppState,
) -> Result<(), LogisticsValidationError> {
    if let Some((_, voxel)) = state
        .logistics()
        .fluid_store_locations()
        .find(|&(_, voxel)| solid_terrain_at(registries, state, voxel))
    {
        return Err(LogisticsValidationError::LocatedFluidStoreIntersectsSolidTerrain { voxel });
    }
    Ok(())
}
