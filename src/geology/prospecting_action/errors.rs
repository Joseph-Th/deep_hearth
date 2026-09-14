//! Typed admission and commit failures for field prospecting.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::equipment::{EquipmentId, EquipmentProviderError};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError, ProspectingMethodId};
use crate::maintenance::ActiveConditionDurationError;
use crate::material::MaterialId;
use crate::mining::MiningJobId;
use crate::production::ProductionJobId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldProspectingStartError {
    UnknownMethod {
        method: ProspectingMethodId,
    },
    UnknownMaterial {
        material: MaterialId,
    },
    RegionVolumeOverflow,
    RegionTooLarge {
        actual: u128,
        maximum: u128,
    },
    EquipmentRequired {
        method: ProspectingMethodId,
    },
    UnexpectedEquipment {
        method: ProspectingMethodId,
        equipment: EquipmentId,
    },
    Equipment(EquipmentProviderError),
    EquipmentMounted {
        equipment: EquipmentId,
    },
    EquipmentDefinitionNotAccepted {
        method: ProspectingMethodId,
        equipment: EquipmentId,
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
    ConditionDuration(ActiveConditionDurationError),
    ObservationIdExhausted,
    KnowledgeRevisionExhausted,
    EquipmentRevisionExhausted,
    CompletionTickOverflow,
    Work(PlayerWorkStartError),
}

impl Display for FieldProspectingStartError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { method } => {
                write!(formatter, "unknown prospecting method {}", method.value())
            }
            Self::UnknownMaterial { material } => {
                write!(
                    formatter,
                    "unknown prospecting material {}",
                    material.value()
                )
            }
            Self::RegionVolumeOverflow => {
                formatter.write_str("prospecting region voxel count overflowed")
            }
            Self::RegionTooLarge { actual, maximum } => write!(
                formatter,
                "prospecting region contains {actual} voxels but method allows at most {maximum}"
            ),
            Self::EquipmentRequired { method } => write!(
                formatter,
                "prospecting method {} requires a physical sampling instrument",
                method.value()
            ),
            Self::UnexpectedEquipment { method, equipment } => write!(
                formatter,
                "prospecting method {} does not use equipment but equipment {} was supplied",
                method.value(),
                equipment.value()
            ),
            Self::Equipment(error) => {
                write!(formatter, "prospecting equipment unavailable: {error}")
            }
            Self::EquipmentMounted { equipment } => write!(
                formatter,
                "prospecting sampling instrument {} must be portable and unmounted",
                equipment.value()
            ),
            Self::EquipmentDefinitionNotAccepted { method, equipment } => write!(
                formatter,
                "prospecting method {} does not accept equipment {}",
                method.value(),
                equipment.value()
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "prospecting equipment {} is occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "prospecting equipment {} is occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "prospecting equipment {} is occupied by direct manual power work",
                equipment.value()
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "prospecting sampling instrument cannot survive the survey: {error}"
            ),
            Self::ObservationIdExhausted => formatter.write_str(
                "prospecting cannot reserve the geological observation identities required at completion",
            ),
            Self::KnowledgeRevisionExhausted => formatter.write_str(
                "prospecting cannot reserve the geological knowledge revisions required at completion",
            ),
            Self::EquipmentRevisionExhausted => formatter.write_str(
                "prospecting cannot reserve the equipment revision required at completion",
            ),
            Self::CompletionTickOverflow => {
                formatter.write_str("prospecting completion tick overflowed")
            }
            Self::Work(error) => write!(formatter, "prospecting labor admission failed: {error}"),
        }
    }
}

impl Error for FieldProspectingStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownMaterial { .. }
            | Self::RegionVolumeOverflow
            | Self::RegionTooLarge { .. }
            | Self::EquipmentRequired { .. }
            | Self::UnexpectedEquipment { .. }
            | Self::EquipmentMounted { .. }
            | Self::EquipmentDefinitionNotAccepted { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. }
            | Self::ObservationIdExhausted
            | Self::KnowledgeRevisionExhausted
            | Self::EquipmentRevisionExhausted
            | Self::CompletionTickOverflow => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldProspectingCommitError {
    Work(PlayerWorkCommitError),
    StaleEquipmentRevision {
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
}

impl Display for FieldProspectingCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(error) => write!(formatter, "prospecting labor commit failed: {error}"),
            Self::StaleEquipmentRevision { expected, actual } => write!(
                formatter,
                "prospecting equipment expected revision {expected} but current revision is {actual}"
            ),
            Self::EquipmentBusyProduction { equipment, job } => write!(
                formatter,
                "prospecting equipment {} became occupied by production job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyMining { equipment, job } => write!(
                formatter,
                "prospecting equipment {} became occupied by mining job {}",
                equipment.value(),
                job.value()
            ),
            Self::EquipmentBusyManualPower { equipment } => write!(
                formatter,
                "prospecting equipment {} became occupied by direct manual power work",
                equipment.value()
            ),
        }
    }
}

impl Error for FieldProspectingCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Work(error) => Some(error),
            Self::StaleEquipmentRevision { .. }
            | Self::EquipmentBusyProduction { .. }
            | Self::EquipmentBusyMining { .. }
            | Self::EquipmentBusyManualPower { .. } => None,
        }
    }
}
