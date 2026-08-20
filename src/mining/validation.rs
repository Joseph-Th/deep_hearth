//! Cross-owner persistence validation for mining job semantics and resource references.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::core::state::AppState;
use crate::core::time::TickSpan;
use crate::equipment::EquipmentDefinitionId;
use crate::inventory::validate_stockpile_storage;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::ore_processing::MassFlowDurationError;
use crate::registry::Registries;

use super::MiningJobId;
use super::physics::{MiningPhysicsError, resolve_mining_physics};

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

fn map_physics_error(job: MiningJobId, error: MiningPhysicsError) -> MiningJobValidationError {
    match error {
        MiningPhysicsError::MissingCapability { capability } => {
            MiningJobValidationError::MissingCapability { job, capability }
        }
        MiningPhysicsError::CapabilityKindMismatch {
            capability,
            expected,
            found,
        } => MiningJobValidationError::CapabilityKindMismatch {
            job,
            capability,
            expected,
            found,
        },
        MiningPhysicsError::BatchTooLarge { maximum, requested } => {
            MiningJobValidationError::BatchTooLarge {
                job,
                maximum,
                requested,
            }
        }
        MiningPhysicsError::DepositTooHard { hardness, maximum } => {
            MiningJobValidationError::DepositTooHard {
                job,
                hardness,
                maximum,
            }
        }
        MiningPhysicsError::ZeroThroughput => MiningJobValidationError::ZeroThroughput { job },
        MiningPhysicsError::Duration(error) => MiningJobValidationError::Duration { job, error },
        MiningPhysicsError::ConditionDuration(error) => {
            MiningJobValidationError::ConditionDuration { job, error }
        }
    }
}

pub(crate) fn validate_loaded_mining_jobs(
    registries: &Registries,
    state: &AppState,
) -> Result<(), MiningJobValidationError> {
    for job in state.mining().jobs() {
        let method = registries
            .mining()
            .get_method(job.method())
            .ok_or(MiningJobValidationError::UnknownMethod { job: job.id() })?;
        let deposit = state
            .geology()
            .get_deposit(job.deposit())
            .ok_or(MiningJobValidationError::UnknownDeposit { job: job.id() })?;
        let destination = state
            .inventory()
            .get_stockpile(job.destination())
            .ok_or(MiningJobValidationError::UnknownDestination { job: job.id() })?;
        let equipment_definition = registries
            .equipment()
            .get_equipment(job.equipment_definition())
            .ok_or(MiningJobValidationError::UnknownEquipmentDefinition {
                job: job.id(),
                definition: job.equipment_definition(),
            })?;

        if job.is_working() {
            let equipment = state
                .equipment()
                .get_equipment(job.equipment())
                .ok_or(MiningJobValidationError::WorkingEquipmentMissing { job: job.id() })?;
            if equipment.definition() != job.equipment_definition() {
                return Err(
                    MiningJobValidationError::WorkingEquipmentDefinitionMismatch {
                        job: job.id(),
                        expected: job.equipment_definition(),
                        actual: equipment.definition(),
                    },
                );
            }
            if equipment.supported_by().is_some() {
                return Err(MiningJobValidationError::WorkingEquipmentMounted { job: job.id() });
            }
            if equipment.condition() != job.equipment_condition_before() {
                return Err(MiningJobValidationError::EquipmentConditionMismatch { job: job.id() });
            }
        }

        let output = job.output();
        if output.commodity() != deposit.commodity()
            || output.temperature() != deposit.temperature()
            || output.composition() != deposit.composition()
            || output.particle_size_distribution().is_some()
        {
            return Err(MiningJobValidationError::OutputProfileMismatch { job: job.id() });
        }
        if validate_stockpile_storage(
            registries,
            destination,
            job.destination(),
            output.commodity(),
            output.composition(),
            output.temperature(),
            output.particle_size_distribution(),
        )
        .is_err()
        {
            return Err(MiningJobValidationError::OutputStorageInvalid { job: job.id() });
        }
        if job.is_working()
            && state
                .production()
                .get_equipment_occupant(job.equipment())
                .is_some()
        {
            return Err(MiningJobValidationError::EquipmentAlsoUsedByProduction { job: job.id() });
        }

        let physics = resolve_mining_physics(
            registries,
            method,
            equipment_definition,
            job.equipment_condition_before(),
            deposit.excavation_hardness(),
            output.mass(),
        )
        .map_err(|error| map_physics_error(job.id(), error))?;
        let stored_duration = TickSpan::new(
            job.completes_at()
                .value()
                .checked_sub(job.started_at().value())
                .filter(|duration| *duration > 0)
                .ok_or(MiningJobValidationError::InvalidSchedule { job: job.id() })?,
        );
        if stored_duration != physics.duration() {
            return Err(MiningJobValidationError::DurationMismatch {
                job: job.id(),
                stored: stored_duration,
                required: physics.duration(),
            });
        }
        if job.equipment_condition_after() != physics.condition_after() {
            return Err(MiningJobValidationError::ConditionOutcomeMismatch {
                job: job.id(),
                stored: job.equipment_condition_after(),
                required: physics.condition_after(),
            });
        }
    }
    Ok(())
}
