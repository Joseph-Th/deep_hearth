//! Equipment-upgrade validation and commit failures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::SimulationTick;
use crate::inventory::{StockpileId, StockpileStructuralLoadError};
use crate::mining::MiningJobId;
use crate::production::{ProductionJobId, ProductionOccupancyRelease};
use crate::structural::{StructuralCommitError, StructuralElementId};

use super::super::{EquipmentDefinitionId, EquipmentId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentUpgradeError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    UnknownTargetDefinition {
        target: EquipmentDefinitionId,
    },
    NoUpgradeProfile {
        target: EquipmentDefinitionId,
    },
    WrongBaseDefinition {
        equipment: EquipmentId,
        required: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
        release: ProductionOccupancyRelease,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    EquipmentUnderMaintenance {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    UnknownSource {
        stockpile: StockpileId,
    },
    InsufficientMaterial {
        stockpile: StockpileId,
        available: Mass,
        required: Mass,
    },
    SourceMassOverflow {
        stockpile: StockpileId,
    },
    StaleInventorySelection {
        expected: u64,
        actual: u64,
    },
    InventoryRevisionExhausted,
    EquipmentRevisionExhausted,
    StructuralLoad(StockpileStructuralLoadError),
}

impl Display for EquipmentUpgradeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::UnknownTargetDefinition { target } => write!(
                formatter,
                "unknown target equipment definition {}",
                target.value()
            ),
            Self::NoUpgradeProfile { target } => write!(
                formatter,
                "equipment definition {} has no authored additive upgrade path",
                target.value()
            ),
            Self::WrongBaseDefinition {
                equipment,
                required,
                actual,
            } => write!(
                formatter,
                "equipment {} uses definition {} but upgrade requires base definition {}",
                equipment.value(),
                actual.value(),
                required.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} is mounted on structural element {} and must be unmounted before its mass changes",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction {
                equipment,
                job,
                release,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} {release} and cannot be upgraded",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be upgraded",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be upgraded",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by geological sampling until tick {} and cannot be upgraded",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is under maintenance until tick {} and cannot be upgraded",
                equipment.value(),
                completes_at.value()
            ),
            Self::UnknownSource { stockpile } => write!(
                formatter,
                "unknown equipment-upgrade material stockpile {}",
                stockpile.value()
            ),
            Self::InsufficientMaterial {
                stockpile,
                available,
                required,
            } => write!(
                formatter,
                "equipment-upgrade stockpile {} contains {} mg but {} mg of authored addition material is required",
                stockpile.value(),
                available.milligrams(),
                required.milligrams()
            ),
            Self::SourceMassOverflow { stockpile } => write!(
                formatter,
                "equipment-upgrade source {} mass accounting overflowed",
                stockpile.value()
            ),
            Self::StaleInventorySelection { expected, actual } => write!(
                formatter,
                "equipment-upgrade material selection expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::InventoryRevisionExhausted => {
                formatter.write_str("inventory revision space is exhausted")
            }
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::StructuralLoad(error) => {
                write!(formatter, "equipment-upgrade source load failed: {error}")
            }
        }
    }
}

impl Error for EquipmentUpgradeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StructuralLoad(error) => Some(error),
            Self::UnknownEquipment { .. }
            | Self::UnknownTargetDefinition { .. }
            | Self::NoUpgradeProfile { .. }
            | Self::WrongBaseDefinition { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. }
            | Self::UnknownSource { .. }
            | Self::InsufficientMaterial { .. }
            | Self::SourceMassOverflow { .. }
            | Self::StaleInventorySelection { .. }
            | Self::InventoryRevisionExhausted
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentUpgradeCommitError {
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    DefinitionChanged {
        equipment: EquipmentId,
        expected: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    EquipmentMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    EquipmentBusyProduction {
        equipment: EquipmentId,
        job: ProductionJobId,
    },
    EquipmentBusyMining {
        equipment: EquipmentId,
        job: MiningJobId,
    },
    EquipmentBusyManualPower {
        equipment: EquipmentId,
    },
    EquipmentBusyProspecting {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    EquipmentUnderMaintenance {
        equipment: EquipmentId,
        completes_at: SimulationTick,
    },
    Structure(StructuralCommitError),
}

impl Display for EquipmentUpgradeCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "equipment upgrade expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "equipment upgrade expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before upgrade commit",
                equipment.value()
            ),
            Self::DefinitionChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} changed definition from expected {} to {} before upgrade commit",
                equipment.value(),
                expected.value(),
                actual.value()
            ),
            Self::EquipmentMounted { equipment, element } => write!(
                formatter,
                "equipment {} became mounted on structural element {} before upgrade commit",
                equipment.value(),
                element.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by production job {} before upgrade commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before upgrade commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before upgrade commit",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by geological sampling until tick {} before upgrade commit",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} entered maintenance until tick {} before upgrade commit",
                equipment.value(),
                completes_at.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "equipment upgrade structure failed: {error}")
            }
        }
    }
}

impl Error for EquipmentUpgradeCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::UnknownEquipment { .. }
            | Self::DefinitionChanged { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. } => None,
        }
    }
}
