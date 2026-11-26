//! Terrain, footing, vegetation, and resident clearance for enclosures.

use crate::core::state::AppState;
use crate::registry::Registries;
use crate::spatial::VoxelCoord;
use crate::surface::{OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM, exposed_residue_compact_depth_um};

use super::super::StockpileId;
use super::StorageEnclosureConstructionError;

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
    validate_enclosure_site(registries, state, target_position)
}

/// Both new and in-place storage must satisfy the same physical site rules.
/// A local assembly also needs actual body contact with fixed ground: a
/// jumping actor can still share the supported destination's discrete voxel.
/// Pointed placement separately authorizes a distinct target cell.
pub(super) fn validate_enclosure_site(
    registries: &Registries,
    state: &AppState,
    position: VoxelCoord,
) -> Result<(), StorageEnclosureConstructionError> {
    if state
        .world()
        .full_cell_collision_at(registries.world(), position)
        == Some(true)
    {
        return Err(StorageEnclosureConstructionError::GeneratedSolidTerrain { position });
    }
    // Harvesting the fruit does not remove the plant's living roots.
    if state.surface().rooted_biomass_occupies_voxel(position) {
        return Err(StorageEnclosureConstructionError::RootedVegetationOccupiesSite { position });
    }
    if crate::kobolds::opening_kobold_occupies_voxel(state, position)
        || crate::kobolds::temple_caste_occupies_voxel(registries, state, position)
    {
        return Err(StorageEnclosureConstructionError::ResidentOccupiesSite { position });
    }
    let compact_depth_um = exposed_residue_compact_depth_um(registries, state, position);
    if compact_depth_um >= OBSTRUCTING_RESIDUE_COMPACT_DEPTH_UM {
        return Err(StorageEnclosureConstructionError::LooseResidueFooting {
            position,
            compact_depth_um,
        });
    }
    if crate::world::generated_position_lacks_fixed_footing(registries, state, position) {
        return Err(StorageEnclosureConstructionError::UnsupportedFloor { position });
    }
    if state
        .logistics()
        .player()
        .is_some_and(|player| player.position() == position)
        && !crate::logistics::player_stationary_placement_supported(registries, state)
    {
        return Err(StorageEnclosureConstructionError::PlayerAirborne);
    }
    Ok(())
}
