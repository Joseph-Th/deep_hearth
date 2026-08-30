//! Failure vocabulary for cross-owner mining-job persistence replay.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::ore_processing::MassFlowDurationError;

use super::super::MiningJobId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningJobValidationError {
    UnknownMethod {
        job: MiningJobId,
    },
    UnknownDeposit {
        job: MiningJobId,
    },
    UnknownDestination {
        job: MiningJobId,
    },
    WorkingEquipmentMissing {
        job: MiningJobId,
    },
    UnknownEquipmentDefinition {
        job: MiningJobId,
        definition: EquipmentDefinitionId,
    },
    WorkingEquipmentDefinitionMismatch {
        job: MiningJobId,
        expected: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    WorkingEquipmentMounted {
        job: MiningJobId,
    },
    EquipmentConditionMismatch {
        job: MiningJobId,
    },
    OutputProfileMismatch {
        job: MiningJobId,
    },
    OutputExceedsDepositTrace {
        job: MiningJobId,
        traced: Mass,
        output: Mass,
    },
    DepositMassStateMismatch {
        job: MiningJobId,
        expected: Mass,
        actual: Mass,
    },
    OutputStorageInvalid {
        job: MiningJobId,
    },
    EquipmentAlsoUsedByProduction {
        job: MiningJobId,
    },
    MissingCapability {
        job: MiningJobId,
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        job: MiningJobId,
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    BatchTooLarge {
        job: MiningJobId,
        maximum: Mass,
        requested: Mass,
    },
    DepositTooHard {
        job: MiningJobId,
        hardness: Pressure,
        maximum: Pressure,
    },
    ZeroThroughput {
        job: MiningJobId,
    },
    Duration {
        job: MiningJobId,
        error: MassFlowDurationError,
    },
    ConditionDuration {
        job: MiningJobId,
        error: ActiveConditionDurationError,
    },
    InvalidSchedule {
        job: MiningJobId,
    },
    DurationMismatch {
        job: MiningJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    ConditionOutcomeMismatch {
        job: MiningJobId,
        stored: Condition,
        required: Condition,
    },
}

impl Display for MiningJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod { job } => {
                write!(
                    formatter,
                    "mining job {} references an unknown method",
                    job.value()
                )
            }
            Self::UnknownDeposit { job } => {
                write!(
                    formatter,
                    "mining job {} references an unknown deposit",
                    job.value()
                )
            }
            Self::UnknownDestination { job } => write!(
                formatter,
                "mining job {} references an unknown destination stockpile",
                job.value()
            ),
            Self::WorkingEquipmentMissing { job } => {
                write!(
                    formatter,
                    "active mining job {} equipment is missing",
                    job.value()
                )
            }
            Self::UnknownEquipmentDefinition { job, definition } => write!(
                formatter,
                "mining job {} equipment references unknown definition {}",
                job.value(),
                definition.value()
            ),
            Self::WorkingEquipmentDefinitionMismatch {
                job,
                expected,
                actual,
            } => write!(
                formatter,
                "mining job {} equipment definition {} does not match traced definition {}",
                job.value(),
                actual.value(),
                expected.value()
            ),
            Self::WorkingEquipmentMounted { job } => write!(
                formatter,
                "active mining job {} uses equipment that is mounted to a structure",
                job.value()
            ),
            Self::EquipmentConditionMismatch { job } => write!(
                formatter,
                "mining job {} equipment condition differs from its start trace",
                job.value()
            ),
            Self::OutputProfileMismatch { job } => write!(
                formatter,
                "mining job {} output no longer matches its geological deposit",
                job.value()
            ),
            Self::OutputExceedsDepositTrace {
                job,
                traced,
                output,
            } => write!(
                formatter,
                "mining job {} output {} mg exceeds traced pre-extraction deposit mass {} mg",
                job.value(),
                output.milligrams(),
                traced.milligrams()
            ),
            Self::DepositMassStateMismatch {
                job,
                expected,
                actual,
            } => write!(
                formatter,
                "mining job {} expects geological source mass {} mg in its current phase but found {} mg",
                job.value(),
                expected.milligrams(),
                actual.milligrams()
            ),
            Self::OutputStorageInvalid { job } => write!(
                formatter,
                "mining job {} output is incompatible with its destination storage",
                job.value()
            ),
            Self::EquipmentAlsoUsedByProduction { job } => write!(
                formatter,
                "mining job {} equipment is also occupied by production",
                job.value()
            ),
            Self::MissingCapability { job, capability } => write!(
                formatter,
                "mining job {} equipment lacks required capability {}",
                job.value(),
                capability.value()
            ),
            Self::CapabilityKindMismatch {
                job,
                capability,
                expected,
                found,
            } => write!(
                formatter,
                "mining job {} capability {} has {found:?} value kind instead of {expected:?}",
                job.value(),
                capability.value()
            ),
            Self::BatchTooLarge {
                job,
                maximum,
                requested,
            } => write!(
                formatter,
                "mining job {} batch {} mg exceeds equipment maximum {} mg",
                job.value(),
                requested.milligrams(),
                maximum.milligrams()
            ),
            Self::DepositTooHard {
                job,
                hardness,
                maximum,
            } => write!(
                formatter,
                "mining job {} deposit hardness {} Pa exceeds equipment maximum {} Pa",
                job.value(),
                hardness.pascals(),
                maximum.pascals()
            ),
            Self::ZeroThroughput { job } => {
                write!(
                    formatter,
                    "mining job {} resolves zero throughput",
                    job.value()
                )
            }
            Self::Duration { job, error } => {
                write!(
                    formatter,
                    "mining job {} duration is invalid: {error}",
                    job.value()
                )
            }
            Self::ConditionDuration { job, error } => write!(
                formatter,
                "mining job {} exceeds equipment condition lifetime: {error}",
                job.value()
            ),
            Self::InvalidSchedule { job } => {
                write!(
                    formatter,
                    "mining job {} has an invalid work schedule",
                    job.value()
                )
            }
            Self::DurationMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "mining job {} stores {} active ticks but current physics requires {}",
                job.value(),
                stored.value(),
                required.value()
            ),
            Self::ConditionOutcomeMismatch {
                job,
                stored,
                required,
            } => write!(
                formatter,
                "mining job {} stores post-work condition {} ppm but current physics requires {} ppm",
                job.value(),
                stored.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
}

impl Error for MiningJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Duration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownDeposit { .. }
            | Self::UnknownDestination { .. }
            | Self::WorkingEquipmentMissing { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::WorkingEquipmentDefinitionMismatch { .. }
            | Self::WorkingEquipmentMounted { .. }
            | Self::EquipmentConditionMismatch { .. }
            | Self::OutputProfileMismatch { .. }
            | Self::OutputExceedsDepositTrace { .. }
            | Self::DepositMassStateMismatch { .. }
            | Self::OutputStorageInvalid { .. }
            | Self::EquipmentAlsoUsedByProduction { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::DepositTooHard { .. }
            | Self::ZeroThroughput { .. }
            | Self::InvalidSchedule { .. }
            | Self::DurationMismatch { .. }
            | Self::ConditionOutcomeMismatch { .. } => None,
        }
    }
}
