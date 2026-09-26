//! Trusted-load validation for logistics/inventory location ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::energy::{EnergyState, EnergyStoreId};
use crate::equipment::{EquipmentId, EquipmentState};
use crate::fluid::{FluidState, FluidStoreId};
use crate::inventory::{InventoryState, StockpileId, StorageDefinitionId};
use crate::spatial::VoxelCoord;
use crate::structural::{StructuralElementId, StructureState};

use super::LogisticsState;

/// Persistent-state validation failure for player logistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogisticsValidationError {
    UninitializedRevisionNonzero {
        revision: u64,
    },
    InitializedRevisionZero,
    UnknownCarriedStockpile {
        stockpile: StockpileId,
    },
    CarriedStockpileMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    CarriedStockpileEnclosed {
        stockpile: StockpileId,
        definition: StorageDefinitionId,
    },
    UnknownLocatedStockpile {
        stockpile: StockpileId,
    },
    LocatedStockpileAlsoCarried {
        stockpile: StockpileId,
    },
    StockpileOutsideSupport {
        stockpile: StockpileId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    UnknownLocatedEquipment {
        equipment: EquipmentId,
    },
    EquipmentOutsideSupport {
        equipment: EquipmentId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
    UnknownLocatedEnergyStore {
        store: EnergyStoreId,
    },
    UnknownLocatedFluidStore {
        store: FluidStoreId,
    },
    FluidStoreOutsideSupport {
        store: FluidStoreId,
        position: VoxelCoord,
        element: StructuralElementId,
    },
}

impl Display for LogisticsValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UninitializedRevisionNonzero { revision } => write!(
                formatter,
                "uninitialized player logistics has nonzero revision {revision}"
            ),
            Self::InitializedRevisionZero => {
                formatter.write_str("initialized player logistics has zero revision")
            }
            Self::UnknownCarriedStockpile { stockpile } => write!(
                formatter,
                "player logistics references missing carried stockpile {}",
                stockpile.value()
            ),
            Self::CarriedStockpileMounted { stockpile, element } => write!(
                formatter,
                "player-carried stockpile {} is also mounted to structural element {}",
                stockpile.value(),
                element.value()
            ),
            Self::UnknownLocatedEnergyStore { store } => write!(
                formatter,
                "energy-store location references missing store {}",
                store.value()
            ),
            Self::UnknownLocatedFluidStore { store } => write!(
                formatter,
                "fluid-store location references missing store {}",
                store.value()
            ),
            Self::FluidStoreOutsideSupport {
                store,
                position,
                element,
            } => write!(
                formatter,
                "fluid store {} at voxel ({},{},{}) lies outside structural support {} bounds",
                store.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::UnknownLocatedEquipment { equipment } => write!(
                formatter,
                "equipment location references missing equipment {}",
                equipment.value()
            ),
            Self::EquipmentOutsideSupport {
                equipment,
                position,
                element,
            } => write!(
                formatter,
                "equipment {} at voxel ({},{},{}) lies outside structural support {} bounds",
                equipment.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
            Self::CarriedStockpileEnclosed {
                stockpile,
                definition,
            } => write!(
                formatter,
                "player-carried stockpile {} also owns stationary storage enclosure {}",
                stockpile.value(),
                definition.value()
            ),
            Self::UnknownLocatedStockpile { stockpile } => write!(
                formatter,
                "stockpile location references missing stockpile {}",
                stockpile.value()
            ),
            Self::LocatedStockpileAlsoCarried { stockpile } => write!(
                formatter,
                "stockpile {} is both player-carried and separately located",
                stockpile.value()
            ),
            Self::StockpileOutsideSupport {
                stockpile,
                position,
                element,
            } => write!(
                formatter,
                "stockpile {} at voxel ({},{},{}) lies outside structural support {} bounds",
                stockpile.value(),
                position.x(),
                position.y(),
                position.z(),
                element.value()
            ),
        }
    }
}

impl Error for LogisticsValidationError {}

fn validate_revision(state: &LogisticsState) -> Result<(), LogisticsValidationError> {
    let populated = state.player().is_some()
        || state.stockpile_locations().next().is_some()
        || state.equipment_locations().next().is_some()
        || state.energy_store_locations().next().is_some()
        || state.fluid_store_locations().next().is_some();
    match (populated, state.revision()) {
        (false, revision) if revision != 0 => {
            Err(LogisticsValidationError::UninitializedRevisionNonzero { revision })
        }
        (true, 0) => Err(LogisticsValidationError::InitializedRevisionZero),
        _ => Ok(()),
    }
}

fn validate_player_custody(
    state: &LogisticsState,
    inventory: &InventoryState,
) -> Result<(), LogisticsValidationError> {
    let Some(player) = state.player().copied() else {
        return Ok(());
    };
    let stockpile = inventory.get_stockpile(player.carried_stockpile()).ok_or(
        LogisticsValidationError::UnknownCarriedStockpile {
            stockpile: player.carried_stockpile(),
        },
    )?;
    if let Some(element) = stockpile.supported_by() {
        return Err(LogisticsValidationError::CarriedStockpileMounted {
            stockpile: player.carried_stockpile(),
            element,
        });
    }
    if let Some(enclosure) = stockpile.enclosure() {
        return Err(LogisticsValidationError::CarriedStockpileEnclosed {
            stockpile: player.carried_stockpile(),
            definition: enclosure.definition(),
        });
    }
    Ok(())
}

fn validate_stockpile_locations(
    state: &LogisticsState,
    inventory: &InventoryState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    for (stockpile, position) in state.stockpile_locations() {
        if state
            .player()
            .is_some_and(|player| player.carried_stockpile() == stockpile)
        {
            return Err(LogisticsValidationError::LocatedStockpileAlsoCarried { stockpile });
        }
        let record = inventory
            .get_stockpile(stockpile)
            .ok_or(LogisticsValidationError::UnknownLocatedStockpile { stockpile })?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::StockpileOutsideSupport {
                stockpile,
                position,
                element,
            });
        }
    }
    Ok(())
}

fn validate_equipment_locations(
    state: &LogisticsState,
    equipment: &EquipmentState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    for (equipment_id, position) in state.equipment_locations() {
        let record = equipment.get_equipment(equipment_id).ok_or(
            LogisticsValidationError::UnknownLocatedEquipment {
                equipment: equipment_id,
            },
        )?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::EquipmentOutsideSupport {
                equipment: equipment_id,
                position,
                element,
            });
        }
    }
    Ok(())
}

fn validate_energy_store_locations(
    state: &LogisticsState,
    energy: &EnergyState,
) -> Result<(), LogisticsValidationError> {
    for (store, _) in state.energy_store_locations() {
        if energy.get_store(store).is_none() {
            return Err(LogisticsValidationError::UnknownLocatedEnergyStore { store });
        }
    }
    Ok(())
}

fn validate_fluid_store_locations(
    state: &LogisticsState,
    fluid: &FluidState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    for (store, position) in state.fluid_store_locations() {
        let record = fluid
            .get_store(store)
            .ok_or(LogisticsValidationError::UnknownLocatedFluidStore { store })?;
        if let Some(element) = record.supported_by()
            && let Some(support) = structures.get_element(element)
            && !support.bounds().has_voxel(position)
        {
            return Err(LogisticsValidationError::FluidStoreOutsideSupport {
                store,
                position,
                element,
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_loaded_logistics(
    state: &LogisticsState,
    inventory: &InventoryState,
    equipment: &EquipmentState,
    energy: &EnergyState,
    fluid: &FluidState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    validate_revision(state)?;
    validate_player_custody(state, inventory)?;
    validate_stockpile_locations(state, inventory, structures)?;
    validate_equipment_locations(state, equipment, structures)?;
    validate_energy_store_locations(state, energy)?;
    validate_fluid_store_locations(state, fluid, structures)?;
    Ok(())
}
