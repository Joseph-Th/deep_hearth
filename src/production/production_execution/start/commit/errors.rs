//! Typed failures for committing a previously validated production start.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::mining::MiningJobId;
use crate::structural::StructuralCommitError;

/// Failure when a validated process start is committed after an owning state has changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartProcessCommitError {
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
            Self::StaleProductionRevision { .. }
            | Self::StaleInventoryRevision { .. }
            | Self::StaleEnergyRevision { .. }
            | Self::StaleEquipmentRevision { .. }
            | Self::StaleStructureRevision { .. }
            | Self::EnergyStoreBusyManualPower { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. } => None,
        }
    }
}
