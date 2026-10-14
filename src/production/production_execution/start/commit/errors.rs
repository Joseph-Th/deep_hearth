//! Typed failures for committing a validated production start.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::{ControlTick, SimulationTick};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::mining::MiningJobId;
use crate::structural::StructuralCommitError;

/// Failure when a validated process start is committed after an owning state has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessCommitError {
    StaleControlTick {
        expected: ControlTick,
        actual: ControlTick,
    },
    StaleSimulationTick {
        expected: SimulationTick,
        actual: SimulationTick,
    },
    StaleLogisticsRevision {
        expected: u64,
        actual: u64,
    },
    StaleSurvivalRevision {
        expected: u64,
        actual: u64,
    },
    StaleProductionRevision {
        expected: u64,
        actual: u64,
    },
    StaleInventoryRevision {
        expected: u64,
        actual: u64,
    },
    StaleEnergyRevision {
        expected: u64,
        actual: u64,
    },
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    StaleStructureRevision {
        expected: u64,
        actual: u64,
    },
    OutputDestinationInTransit {
        stockpile: StockpileId,
    },
    EnergyStoreBusyManualPower {
        store: EnergyStoreId,
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

impl Display for StartProcessCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleControlTick { expected, actual } => write!(
                formatter,
                "validated process start expected control tick {} but current control tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleSimulationTick { expected, actual } => write!(
                formatter,
                "validated process start was resolved at simulation tick {} but current simulation tick is {}",
                expected.value(),
                actual.value()
            ),
            Self::StaleLogisticsRevision { expected, actual } => write!(
                formatter,
                "validated process start expected logistics revision {expected} but current revision is {actual}"
            ),
            Self::StaleSurvivalRevision { expected, actual } => write!(
                formatter,
                "player-authorized process start expected survival revision {expected} but current revision is {actual}"
            ),
            Self::OutputDestinationInTransit { stockpile } => write!(
                formatter,
                "validated process output destination {} entered an active player step before commit",
                stockpile.value()
            ),
            Self::StaleProductionRevision { expected, actual } => write!(
                formatter,
                "validated process start expected production revision {expected} but current revision is {actual}"
            ),
            Self::StaleInventoryRevision { expected, actual } => write!(
                formatter,
                "validated process start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEnergyRevision { expected, actual } => write!(
                formatter,
                "validated process start expected energy revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated process start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructureRevision { expected, actual } => write!(
                formatter,
                "validated process start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EnergyStoreBusyManualPower { store } => write!(
                formatter,
                "validated process start energy store {} is occupied by direct player-powered generation",
                store.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "validated process start equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "validated process start equipment {} is occupied by direct player-powered generation",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "validated process start equipment {} is occupied by geological sampling until tick {}",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "validated process start equipment {} is under maintenance until tick {}",
                equipment.value(),
                completes_at.value()
            ),
            Self::Structure(error) => write!(
                formatter,
                "validated process start could not commit stored-matter structural load: {error}"
            ),
        }
    }
}

impl Error for StartProcessCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleControlTick { .. }
            | Self::StaleSimulationTick { .. }
            | Self::StaleLogisticsRevision { .. }
            | Self::StaleSurvivalRevision { .. }
            | Self::StaleProductionRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::OutputDestinationInTransit { .. }
            | Self::EnergyStoreBusyManualPower { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. } => None,
        }
    }
}
