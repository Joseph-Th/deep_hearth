//! Failure types for screening resolution and persisted-job replay.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::{Length, Mass};
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::{
    FormId, MaterialLotSpecError, ParticleSizeDistributionError, ParticleSizeRange,
};
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError, ProductionJobId};

use super::super::{MassFlowDurationError, powered_physics::PoweredOreJobValidationError};

/// Failure while partitioning selected material into exact screen products.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningBatchError {
    EmptyInput,
    NoParticleSizePartition {
        aperture: Length,
    },
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    MissingParticleSize,
    UnresolvedParticleClass {
        aperture: Length,
        class: ParticleSizeRange,
    },
    UnrepresentableClassMass {
        mass: Mass,
        undersize_weight: u64,
        total_weight: u64,
    },
    MassOverflow,
    Distribution(ParticleSizeDistributionError),
    Output(MaterialLotSpecError),
}

impl Display for ScreeningBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("screening batch contains no material"),
            Self::NoParticleSizePartition { aperture } => write!(
                formatter,
                "screening batch is already wholly classified to one side of the {} um aperture",
                aperture.micrometers()
            ),
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "screening batch requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::MissingParticleSize => {
                formatter.write_str("screening input has no particulate size distribution")
            }
            Self::UnresolvedParticleClass { aperture, class } => write!(
                formatter,
                "screen aperture {} um intersects unresolved particle class {}..={} um",
                aperture.micrometers(),
                class.minimum_diameter().micrometers(),
                class.maximum_diameter().micrometers()
            ),
            Self::UnrepresentableClassMass {
                mass,
                undersize_weight,
                total_weight,
            } => write!(
                formatter,
                "screening {} mg with undersize weight {undersize_weight}/{total_weight} cannot be represented exactly at whole-milligram mass resolution",
                mass.milligrams()
            ),
            Self::MassOverflow => formatter.write_str("screening output mass overflowed"),
            Self::Distribution(error) => write!(
                formatter,
                "screening could not preserve a classified particle distribution: {error}"
            ),
            Self::Output(error) => write!(
                formatter,
                "screening output specification could not preserve its material profile: {error}"
            ),
        }
    }
}

impl Error for ScreeningBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Distribution(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::InputFormMismatch { .. }
            | Self::NoParticleSizePartition { .. }
            | Self::UnresolvedParticleClass { .. }
            | Self::UnrepresentableClassMass { .. }
            | Self::EmptyInput
            | Self::MissingParticleSize
            | Self::MassOverflow => None,
        }
    }
}

/// Failure while resolving one exact screening operation before authoritative mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningResolutionError {
    UnknownScreeningProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    Batch(ScreeningBatchError),
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for ScreeningResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownScreeningProcess { process } => write!(
                formatter,
                "process {} has no authored screening semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "screening input selection failed: {error}"),
            Self::Equipment(error) => write!(formatter, "screening equipment failed: {error}"),
            Self::Capability(error) => write!(
                formatter,
                "screening capability requirement failed: {error}"
            ),
            Self::MissingMassFlowCapability => {
                formatter.write_str("screening equipment has no usable mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("screening equipment has no usable maximum-batch-mass capability"),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected screening batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "screening batch resolution failed: {error}"),
            Self::Energy(error) => write!(formatter, "screening energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "screening requires {required:?} energy but selected source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "screening throughput duration failed: {error}")
            }
            Self::EnergyDuration(error) => {
                write!(
                    formatter,
                    "screening energy delivery duration failed: {error}"
                )
            }
            Self::ConditionDuration(error) => {
                write!(
                    formatter,
                    "screening exceeds equipment condition lifetime: {error}"
                )
            }
            Self::Resolution(error) => {
                write!(formatter, "screening process resolution failed: {error}")
            }
        }
    }
}

impl Error for ScreeningResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownScreeningProcess { process: _process } => None,
            Self::BatchMassExceeded {
                selected: _selected,
                maximum: _maximum,
            } => None,
            Self::WrongEnergyCarrier {
                required: _required,
                provided: _provided,
            } => None,
            Self::MissingMassFlowCapability | Self::MissingMaximumBatchMassCapability => None,
        }
    }
}

/// Persistent-state failure found while recomputing an in-flight screening job from its traces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreeningJobValidationError {
    Powered {
        job: ProductionJobId,
        error: PoweredOreJobValidationError,
    },
    Batch {
        job: ProductionJobId,
        error: ScreeningBatchError,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for ScreeningJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Powered { job, error } => write!(
                formatter,
                "screening job {} powered-physics replay failed: {error}",
                job.value()
            ),
            Self::Batch { job, error } => write!(
                formatter,
                "screening job {} has invalid batch physics: {error}",
                job.value()
            ),
            Self::OutputMismatch { job } => write!(
                formatter,
                "screening job {} output snapshot no longer matches its consumed material traces",
                job.value()
            ),
        }
    }
}

impl Error for ScreeningJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Powered { error, .. } => Some(error),
            Self::Batch { job: _job, error } => Some(error),
            Self::OutputMismatch { job: _job } => None,
        }
    }
}
