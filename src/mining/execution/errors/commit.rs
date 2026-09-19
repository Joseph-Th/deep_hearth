//! Late commit failures after a validated mining start becomes stale.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::equipment::EquipmentId;
use crate::labor::PlayerWorkCommitError;
use crate::production::ProductionJobId;

use super::super::super::MiningJobId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningStartCommitError {
    TargetNoLongerResolved,
    /// Hidden source state changed after validation. Exact reserve values stay non-oracular.
    TargetChanged,
    StaleInventory {
        expected: u64,
        actual: u64,
    },
    StaleEquipment {
        expected: u64,
        actual: u64,
    },
    StaleMining {
        expected: u64,
        actual: u64,
    },
    StaleStructure {
        expected: u64,
        actual: u64,
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
    Work(PlayerWorkCommitError),
}

impl Display for MiningStartCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetNoLongerResolved => formatter.write_str(
                "validated mining target is no longer uniquely supported by current local evidence and geology",
            ),
            Self::TargetChanged => formatter.write_str(
                "validated mining target changed after validation; resolve the target again",
            ),
            Self::StaleInventory { expected, actual } => write!(
                formatter,
                "validated mining start expected inventory revision {expected} but current revision is {actual}"
            ),
            Self::StaleEquipment { expected, actual } => write!(
                formatter,
                "validated mining start expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::StaleMining { expected, actual } => write!(
                formatter,
                "validated mining start expected mining revision {expected} but current revision is {actual}"
            ),
            Self::StaleStructure { expected, actual } => write!(
                formatter,
                "validated mining start expected structural revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "validated mining start equipment {} became occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "validated mining start equipment {} became occupied by manual power generation",
                equipment.value()
            ),
            Self::Work(error) => write!(
                formatter,
                "validated mining start player-work state changed: {error}"
            ),
        }
    }
}

impl Error for MiningStartCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::TargetNoLongerResolved
            | Self::TargetChanged
            | Self::StaleInventory { .. }
            | Self::StaleEquipment { .. }
            | Self::StaleMining { .. }
            | Self::StaleStructure { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}
