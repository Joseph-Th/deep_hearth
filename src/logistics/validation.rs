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
    UnknownGroundStockpile {
        stockpile: StockpileId,
    },
    GroundStockpileAlsoCarried {
        stockpile: StockpileId,
    },
    GroundStockpileMounted {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    UnknownDetachedEquipment {
        equipment: EquipmentId,
    },
    DetachedEquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    UnknownDetachedEnergyStore {
        store: EnergyStoreId,
    },
    UnknownFluidStore {
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
            Self::UnknownDetachedEnergyStore { store } => write!(
                formatter,
                "detached energy-store location references missing store {}",
                store.value()
            ),
            Self::UnknownFluidStore { store } => write!(
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
            Self::UnknownDetachedEquipment { equipment } => write!(
                formatter,
                "detached equipment location references missing equipment {}",
                equipment.value()
            ),
            Self::DetachedEquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} is both detached in logistics and mounted to structural element {}",
                equipment.value(),
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
            Self::UnknownGroundStockpile { stockpile } => write!(
                formatter,
                "ground location references missing stockpile {}",
                stockpile.value()
            ),
            Self::GroundStockpileAlsoCarried { stockpile } => write!(
                formatter,
                "stockpile {} is both player-carried and ground-located",
                stockpile.value()
            ),
            Self::GroundStockpileMounted { stockpile, element } => write!(
                formatter,
                "ground stockpile {} is also mounted to structural element {}",
                stockpile.value(),
                element.value()
            ),
        }
    }
}

impl Error for LogisticsValidationError {}

pub(crate) fn validate_loaded_logistics(
    state: &LogisticsState,
    inventory: &InventoryState,
    equipment: &EquipmentState,
    energy: &EnergyState,
    fluid: &FluidState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    let populated = state.player().is_some()
        || state.ground_stockpiles().next().is_some()
        || state.detached_equipment().next().is_some()
        || state.detached_energy_stores().next().is_some()
        || state.fluid_stores().next().is_some();
    match (populated, state.revision()) {
        (false, revision) if revision != 0 => {
            return Err(LogisticsValidationError::UninitializedRevisionNonzero {
                revision: state.revision(),
            });
        }
        (true, 0) => {
            return Err(LogisticsValidationError::InitializedRevisionZero);
        }
        _ => {}
    }
    if let Some(player) = state.player().copied() {
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
    }
    for (stockpile, _) in state.ground_stockpiles() {
        if state
            .player()
            .is_some_and(|player| player.carried_stockpile() == stockpile)
        {
            return Err(LogisticsValidationError::GroundStockpileAlsoCarried { stockpile });
        }
        let record = inventory
            .get_stockpile(stockpile)
            .ok_or(LogisticsValidationError::UnknownGroundStockpile { stockpile })?;
        if let Some(element) = record.supported_by() {
            return Err(LogisticsValidationError::GroundStockpileMounted { stockpile, element });
        }
    }
    for (equipment_id, _) in state.detached_equipment() {
        let record = equipment.get_equipment(equipment_id).ok_or(
            LogisticsValidationError::UnknownDetachedEquipment {
                equipment: equipment_id,
            },
        )?;
        if let Some(element) = record.supported_by() {
            return Err(LogisticsValidationError::DetachedEquipmentMounted {
                equipment: equipment_id,
                element,
            });
        }
    }
    for (store, _) in state.detached_energy_stores() {
        if energy.get_store(store).is_none() {
            return Err(LogisticsValidationError::UnknownDetachedEnergyStore { store });
        }
    }
    for (store, position) in state.fluid_stores() {
        let record = fluid
            .get_store(store)
            .ok_or(LogisticsValidationError::UnknownFluidStore { store })?;
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
