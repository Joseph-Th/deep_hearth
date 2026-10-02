//! Player access checks for logistics-owned world locations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

/// Failure when a player action cannot prove local access to a stockpile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerStockpileAccessError {
    UnlocatedStockpile {
        stockpile: StockpileId,
    },
    RemoteKnownStockpile {
        stockpile: StockpileId,
        stockpile_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerStockpileAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnlocatedStockpile { stockpile } => write!(
                formatter,
                "stockpile {} has no logistics-owned world location",
                stockpile.value()
            ),
            Self::RemoteKnownStockpile {
                stockpile,
                stockpile_position,
                player_position,
            } => write!(
                formatter,
                "stockpile {} is at voxel ({},{},{}) but player is at ({},{},{})",
                stockpile.value(),
                stockpile_position.x(),
                stockpile_position.y(),
                stockpile_position.z(),
                player_position.x(),
                player_position.y(),
                player_position.z()
            ),
        }
    }
}

impl Error for PlayerStockpileAccessError {}

/// Failure when a player action cannot prove local access to equipment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentAccessError {
    UnlocatedEquipment {
        equipment: EquipmentId,
    },
    RemoteKnownEquipment {
        equipment: EquipmentId,
        equipment_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerEquipmentAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnlocatedEquipment { equipment } => write!(
                formatter,
                "equipment {} has no logistics-owned world location",
                equipment.value()
            ),
            Self::RemoteKnownEquipment {
                equipment,
                equipment_position,
                player_position,
            } => write!(
                formatter,
                "equipment {} is at voxel ({},{},{}) but player is at ({},{},{})",
                equipment.value(),
                equipment_position.x(),
                equipment_position.y(),
                equipment_position.z(),
                player_position.x(),
                player_position.y(),
                player_position.z()
            ),
        }
    }
}

impl Error for PlayerEquipmentAccessError {}

/// Failure when a player action cannot prove local access to a finite-energy store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEnergyStoreAccessError {
    UnlocatedEnergyStore {
        store: EnergyStoreId,
    },
    RemoteKnownEnergyStore {
        store: EnergyStoreId,
        store_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerEnergyStoreAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnlocatedEnergyStore { store } => write!(
                formatter,
                "energy store {} has no logistics-owned world location",
                store.value()
            ),
            Self::RemoteKnownEnergyStore {
                store,
                store_position,
                player_position,
            } => write!(
                formatter,
                "energy store {} is at voxel ({},{},{}) but player is at ({},{},{})",
                store.value(),
                store_position.x(),
                store_position.y(),
                store_position.z(),
                player_position.x(),
                player_position.y(),
                player_position.z()
            ),
        }
    }
}

impl Error for PlayerEnergyStoreAccessError {}

/// Failure when a player action cannot prove local access to a finite-fluid store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerFluidStoreAccessError {
    UnlocatedFluidStore {
        store: FluidStoreId,
    },
    RemoteKnownFluidStore {
        store: FluidStoreId,
        store_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerFluidStoreAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnlocatedFluidStore { store } => write!(
                formatter,
                "fluid store {} has no logistics-owned world location",
                store.value()
            ),
            Self::RemoteKnownFluidStore {
                store,
                store_position,
                player_position,
            } => write!(
                formatter,
                "fluid store {} is at voxel ({},{},{}) but player is at ({},{},{})",
                store.value(),
                store_position.x(),
                store_position.y(),
                store_position.z(),
                player_position.x(),
                player_position.y(),
                player_position.z()
            ),
        }
    }
}

impl Error for PlayerFluidStoreAccessError {}

fn validate_known_local_position<E>(
    player_position: VoxelCoord,
    asset_position: Option<VoxelCoord>,
    asset_exists: bool,
    unlocated_error: E,
    remote_error: impl FnOnce(VoxelCoord, VoxelCoord) -> E,
) -> Result<(), E> {
    let Some(asset_position) = asset_position else {
        return if asset_exists {
            Err(unlocated_error)
        } else {
            Ok(())
        };
    };
    if asset_position != player_position {
        return Err(remote_error(asset_position, player_position));
    }
    Ok(())
}

/// Requires every known non-carried stockpile targeted by a player action to have an exact
/// logistics-owned world location at the player's voxel. Controlled fixtures without a player
/// remain free to exercise capability-level behavior without world custody.
pub fn validate_player_stockpile_access(
    state: &AppState,
    stockpile: StockpileId,
) -> Result<(), PlayerStockpileAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    if player.carried_stockpile() == stockpile {
        return Ok(());
    }
    validate_known_local_position(
        player.position(),
        state.logistics().stationary_stockpile_position(stockpile),
        state.inventory().get_stockpile(stockpile).is_some(),
        PlayerStockpileAccessError::UnlocatedStockpile { stockpile },
        |stockpile_position, player_position| PlayerStockpileAccessError::RemoteKnownStockpile {
            stockpile,
            stockpile_position,
            player_position,
        },
    )
}

/// Requires every known finite-fluid store targeted by a player action to be explicitly co-located.
pub fn validate_player_fluid_store_access(
    state: &AppState,
    store: FluidStoreId,
) -> Result<(), PlayerFluidStoreAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    validate_known_local_position(
        player.position(),
        state.logistics().fluid_store_position(store),
        state.fluid().get_store(store).is_some(),
        PlayerFluidStoreAccessError::UnlocatedFluidStore { store },
        |store_position, player_position| PlayerFluidStoreAccessError::RemoteKnownFluidStore {
            store,
            store_position,
            player_position,
        },
    )
}

/// Requires every known finite-energy store targeted by a player action to be explicitly co-located.
pub fn validate_player_energy_store_access(
    state: &AppState,
    store: EnergyStoreId,
) -> Result<(), PlayerEnergyStoreAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    validate_known_local_position(
        player.position(),
        state.logistics().energy_store_position(store),
        state.energy().get_store(store).is_some(),
        PlayerEnergyStoreAccessError::UnlocatedEnergyStore { store },
        |store_position, player_position| PlayerEnergyStoreAccessError::RemoteKnownEnergyStore {
            store,
            store_position,
            player_position,
        },
    )
}

/// Requires every known equipment instance targeted by a player action to be explicitly co-located.
pub fn validate_player_equipment_access(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), PlayerEquipmentAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    validate_known_local_position(
        player.position(),
        state.logistics().equipment_position(equipment),
        state.equipment().get_equipment(equipment).is_some(),
        PlayerEquipmentAccessError::UnlocatedEquipment { equipment },
        |equipment_position, player_position| PlayerEquipmentAccessError::RemoteKnownEquipment {
            equipment,
            equipment_position,
            player_position,
        },
    )
}
