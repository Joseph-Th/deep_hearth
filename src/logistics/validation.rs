//! Trusted-load validation for logistics/inventory location ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::energy::{EnergyState, EnergyStoreId};
use crate::equipment::{EquipmentId, EquipmentRegistry, EquipmentState};
use crate::fluid::{FluidState, FluidStoreId};
use crate::inventory::{InventoryState, StockpileId, StorageDefinitionId};
use crate::spatial::VoxelCoord;
use crate::structural::{StructuralElementId, StructureState};

use super::{LogisticsState, TraversalEdge};

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
    CarriedEquipmentWithoutPlayer {
        equipment: EquipmentId,
    },
    CarriedEquipmentNotStrictlyOrdered {
        previous: EquipmentId,
        current: EquipmentId,
    },
    UnknownCarriedEquipment {
        equipment: EquipmentId,
    },
    CarriedEquipmentAlsoLocated {
        equipment: EquipmentId,
    },
    CarriedEquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    CarriedEquipmentRequiresSupport {
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
    TraversalEdgeNotCanonical {
        first: VoxelCoord,
        second: VoxelCoord,
    },
    TraversalEdgesNotStrictlyOrdered {
        previous_first: VoxelCoord,
        previous_second: VoxelCoord,
        current_first: VoxelCoord,
        current_second: VoxelCoord,
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
            Self::CarriedEquipmentWithoutPlayer { equipment } => write!(
                formatter,
                "equipment {} is marked player-carried without a logistics player",
                equipment.value()
            ),
            Self::CarriedEquipmentNotStrictlyOrdered { previous, current } => write!(
                formatter,
                "carried equipment IDs are not strictly ordered: {} then {}",
                previous.value(),
                current.value()
            ),
            Self::UnknownCarriedEquipment { equipment } => write!(
                formatter,
                "player logistics references missing carried equipment {}",
                equipment.value()
            ),
            Self::CarriedEquipmentAlsoLocated { equipment } => write!(
                formatter,
                "equipment {} is both player-carried and separately located",
                equipment.value()
            ),
            Self::CarriedEquipmentMounted { equipment, element } => write!(
                formatter,
                "player-carried equipment {} is mounted to structural element {}",
                equipment.value(),
                element.value()
            ),
            Self::CarriedEquipmentRequiresSupport { equipment } => write!(
                formatter,
                "equipment {} requires structural support and cannot be player-carried",
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
            Self::TraversalEdgeNotCanonical { first, second } => write!(
                formatter,
                "traversal edge ({},{},{}) <-> ({},{},{}) is not a canonical face-adjacent edge",
                first.x(),
                first.y(),
                first.z(),
                second.x(),
                second.y(),
                second.z()
            ),
            Self::TraversalEdgesNotStrictlyOrdered {
                previous_first,
                previous_second,
                current_first,
                current_second,
            } => write!(
                formatter,
                "traversal edges are not strictly ordered: ({},{},{})<->({},{},{}) then ({},{},{})<->({},{},{})",
                previous_first.x(),
                previous_first.y(),
                previous_first.z(),
                previous_second.x(),
                previous_second.y(),
                previous_second.z(),
                current_first.x(),
                current_first.y(),
                current_first.z(),
                current_second.x(),
                current_second.y(),
                current_second.z()
            ),
        }
    }
}

impl Error for LogisticsValidationError {}

fn validate_revision(state: &LogisticsState) -> Result<(), LogisticsValidationError> {
    let populated = state.player().is_some()
        || state.traversable_edges().next().is_some()
        || state.carried_equipment().next().is_some()
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

fn validate_traversal_edges(state: &LogisticsState) -> Result<(), LogisticsValidationError> {
    let mut previous: Option<TraversalEdge> = None;
    for edge in state.traversable_edges() {
        if !edge.is_canonical() {
            let (first, second) = edge.endpoints();
            return Err(LogisticsValidationError::TraversalEdgeNotCanonical { first, second });
        }
        if let Some(previous_edge) = previous
            && previous_edge >= edge
        {
            let (previous_first, previous_second) = previous_edge.endpoints();
            let (current_first, current_second) = edge.endpoints();
            return Err(LogisticsValidationError::TraversalEdgesNotStrictlyOrdered {
                previous_first,
                previous_second,
                current_first,
                current_second,
            });
        }
        previous = Some(edge);
    }
    Ok(())
}

fn validate_carried_equipment(
    state: &LogisticsState,
    equipment: &EquipmentState,
    registry: &EquipmentRegistry,
) -> Result<(), LogisticsValidationError> {
    let mut previous = None;
    for equipment_id in state.carried_equipment() {
        if state.player().is_none() {
            return Err(LogisticsValidationError::CarriedEquipmentWithoutPlayer {
                equipment: equipment_id,
            });
        }
        if let Some(previous_id) = previous
            && previous_id >= equipment_id
        {
            return Err(
                LogisticsValidationError::CarriedEquipmentNotStrictlyOrdered {
                    previous: previous_id,
                    current: equipment_id,
                },
            );
        }
        previous = Some(equipment_id);
        let record = equipment.get_equipment(equipment_id).ok_or(
            LogisticsValidationError::UnknownCarriedEquipment {
                equipment: equipment_id,
            },
        )?;
        if state.stationary_equipment_position(equipment_id).is_some() {
            return Err(LogisticsValidationError::CarriedEquipmentAlsoLocated {
                equipment: equipment_id,
            });
        }
        if let Some(element) = record.supported_by() {
            return Err(LogisticsValidationError::CarriedEquipmentMounted {
                equipment: equipment_id,
                element,
            });
        }
        let definition = registry
            .get_equipment(record.definition())
            .unwrap_or_else(|| {
                unreachable!(
                    "equipment owner validation proves carried equipment definition exists"
                )
            });
        if definition.requires_structural_support() {
            return Err(LogisticsValidationError::CarriedEquipmentRequiresSupport {
                equipment: equipment_id,
            });
        }
    }
    Ok(())
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
    equipment_registry: &EquipmentRegistry,
    energy: &EnergyState,
    fluid: &FluidState,
    structures: &StructureState,
) -> Result<(), LogisticsValidationError> {
    validate_revision(state)?;
    validate_traversal_edges(state)?;
    validate_player_custody(state, inventory)?;
    validate_carried_equipment(state, equipment, equipment_registry)?;
    validate_stockpile_locations(state, inventory, structures)?;
    validate_equipment_locations(state, equipment, structures)?;
    validate_energy_store_locations(state, energy)?;
    validate_fluid_store_locations(state, fluid, structures)?;
    Ok(())
}
