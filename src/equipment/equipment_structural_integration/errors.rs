//! Public validation and commit errors for equipment structural support transactions.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Force;
use crate::core::time::SimulationTick;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;
use crate::structural::{
    StructuralCommitError, StructuralElementId, StructuralLifecycle, StructuralMutationError,
};

use crate::equipment::EquipmentId;

/// Failure while resolving one equipment support assignment before any owner mutates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSupportError {
    UnknownEquipment {
        equipment: EquipmentId,
    },
    AlreadyMounted {
        equipment: EquipmentId,
        element: StructuralElementId,
    },
    NotMounted {
        equipment: EquipmentId,
    },
    TargetNotActive {
        element: StructuralElementId,
        lifecycle: StructuralLifecycle,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
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
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingEquipmentLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
    EquipmentRevisionExhausted,
    Structure(StructuralMutationError),
}

impl Display for EquipmentSupportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownEquipment { equipment } => {
                write!(formatter, "unknown equipment id {}", equipment.value())
            }
            Self::AlreadyMounted { equipment, element } => write!(
                formatter,
                "equipment {} is already supported by structural element {}",
                equipment.value(),
                element.value()
            ),
            Self::NotMounted { equipment } => write!(
                formatter,
                "equipment {} has no structural support assignment to remove",
                equipment.value()
            ),
            Self::TargetNotActive { element, lifecycle } => write!(
                formatter,
                "structural element {} is {lifecycle:?} and cannot receive mounted equipment",
                element.value()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by production job {} until tick {} and cannot be moved",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} is occupied by mining job {} and cannot be moved",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} is occupied by direct player-powered generation and cannot be moved",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is occupied by geological sampling until tick {} and cannot be moved",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} is under maintenance until tick {} and cannot be moved",
                equipment.value(),
                completes_at.value()
            ),
            Self::AggregateMassOverflow { element } => write!(
                formatter,
                "mounted equipment mass overflows aggregate accounting on structural element {}",
                element.value()
            ),
            Self::WeightForceOverflow { element } => write!(
                formatter,
                "mounted equipment weight exceeds structural force range on element {}",
                element.value()
            ),
            Self::ExistingEquipmentLoadMismatch {
                element,
                stored,
                expected,
            } => write!(
                formatter,
                "structural element {} stores {} mN equipment load but equipment ownership requires {} mN",
                element.value(),
                stored.millinewtons(),
                expected.millinewtons()
            ),
            Self::EquipmentRevisionExhausted => {
                formatter.write_str("equipment revision space is exhausted")
            }
            Self::Structure(error) => {
                write!(formatter, "structural support change failed: {error}")
            }
        }
    }
}

impl Error for EquipmentSupportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::UnknownEquipment { .. }
            | Self::AlreadyMounted { .. }
            | Self::NotMounted { .. }
            | Self::TargetNotActive { .. }
            | Self::EquipmentBusy { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. }
            | Self::AggregateMassOverflow { .. }
            | Self::WeightForceOverflow { .. }
            | Self::ExistingEquipmentLoadMismatch { .. }
            | Self::EquipmentRevisionExhausted => None,
        }
    }
}

/// Failure to commit a revision-bound equipment/support transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EquipmentSupportCommitError {
    StaleEquipmentRevision {
        expected: u64,
        actual: u64,
    },
    UnknownEquipment {
        equipment: EquipmentId,
    },
    SupportChanged {
        equipment: EquipmentId,
        expected: Option<StructuralElementId>,
        actual: Option<StructuralElementId>,
    },
    EquipmentBusy {
        equipment: EquipmentId,
        job: ProductionJobId,
        completes_at: SimulationTick,
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

impl Display for EquipmentSupportCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "validated equipment support change expected equipment revision {expected} but current revision is {actual}"
            ),
            Self::UnknownEquipment { equipment } => write!(
                formatter,
                "equipment {} disappeared before support commit",
                equipment.value()
            ),
            Self::SupportChanged {
                equipment,
                expected,
                actual,
            } => write!(
                formatter,
                "equipment {} support changed from expected {expected:?} to {actual:?} before commit",
                equipment.value()
            ),
            Self::EquipmentBusy {
                equipment,
                job,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by production job {} until tick {} before support commit",
                equipment.value(),
                job.value(),
                completes_at.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "equipment {} became occupied by mining job {} before support commit",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "equipment {} became occupied by direct player-powered generation before support commit",
                equipment.value()
            ),
            Self::EquipmentBusyProspecting {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} became occupied by geological sampling until tick {} before support commit",
                equipment.value(),
                completes_at.value()
            ),
            Self::EquipmentUnderMaintenance {
                equipment,
                completes_at,
            } => write!(
                formatter,
                "equipment {} entered maintenance until tick {} before support commit",
                equipment.value(),
                completes_at.value()
            ),
            Self::Structure(error) => {
                write!(formatter, "structural support commit failed: {error}")
            }
        }
    }
}

impl Error for EquipmentSupportCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Structure(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::UnknownEquipment { .. }
            | Self::SupportChanged { .. }
            | Self::EquipmentBusy { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::EquipmentBusyProspecting { .. }
            | Self::EquipmentUnderMaintenance { .. } => None,
        }
    }
}
