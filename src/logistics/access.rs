//! Player access checks for stockpiles with explicit logistics-owned locations.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

/// Failure when a player action targets a stockpile known to be at another voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerStockpileAccessError {
    RemoteKnownStockpile {
        stockpile: StockpileId,
        stockpile_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerStockpileAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Failure when a player action targets detached equipment known to be at another voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEquipmentAccessError {
    RemoteKnownEquipment {
        equipment: EquipmentId,
        equipment_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerEquipmentAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Failure when a player action targets a finite-energy store known to be at another voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerEnergyStoreAccessError {
    RemoteKnownEnergyStore {
        store: EnergyStoreId,
        store_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerEnergyStoreAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Failure when a player action targets a finite-fluid store known to be at another voxel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerFluidStoreAccessError {
    RemoteKnownFluidStore {
        store: FluidStoreId,
        store_position: VoxelCoord,
        player_position: VoxelCoord,
    },
}

impl Display for PlayerFluidStoreAccessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
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

/// Rejects a stockpile only when logistics positively knows it is remote from the player.
///
/// Unlocated stockpiles remain admissible for controlled fixtures and systems that have not yet
/// migrated to world-space custody. Once a stockpile is logistics-owned on the ground, player
/// actions must occur at that exact voxel. The carried stockpile is implicitly co-located with its
/// owning player and therefore needs no separate ground entry.
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
    let Some(stockpile_position) = state.logistics().ground_stockpile_position(stockpile) else {
        return Ok(());
    };
    if stockpile_position != player.position() {
        return Err(PlayerStockpileAccessError::RemoteKnownStockpile {
            stockpile,
            stockpile_position,
            player_position: player.position(),
        });
    }
    Ok(())
}

/// Rejects a finite-fluid store only when logistics positively knows it is remote from the player.
pub fn validate_player_fluid_store_access(
    state: &AppState,
    store: FluidStoreId,
) -> Result<(), PlayerFluidStoreAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    let Some(store_position) = state.logistics().fluid_store_position(store) else {
        return Ok(());
    };
    if store_position != player.position() {
        return Err(PlayerFluidStoreAccessError::RemoteKnownFluidStore {
            store,
            store_position,
            player_position: player.position(),
        });
    }
    Ok(())
}

/// Rejects a finite-energy store only when logistics positively knows it is remote from the player.
pub fn validate_player_energy_store_access(
    state: &AppState,
    store: EnergyStoreId,
) -> Result<(), PlayerEnergyStoreAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    let Some(store_position) = state.logistics().energy_store_position(store) else {
        return Ok(());
    };
    if store_position != player.position() {
        return Err(PlayerEnergyStoreAccessError::RemoteKnownEnergyStore {
            store,
            store_position,
            player_position: player.position(),
        });
    }
    Ok(())
}

/// Rejects detached equipment only when logistics positively knows it is remote from the player.
pub fn validate_player_equipment_access(
    state: &AppState,
    equipment: EquipmentId,
) -> Result<(), PlayerEquipmentAccessError> {
    let Some(player) = state.logistics().player().copied() else {
        return Ok(());
    };
    let Some(equipment_position) = state.logistics().equipment_position(equipment) else {
        return Ok(());
    };
    if equipment_position != player.position() {
        return Err(PlayerEquipmentAccessError::RemoteKnownEquipment {
            equipment,
            equipment_position,
            player_position: player.position(),
        });
    }
    Ok(())
}
