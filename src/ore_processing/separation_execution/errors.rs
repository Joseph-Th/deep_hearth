//! Failure types for constituent-separation batch physics and powered resolution.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::Mass;
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::{FormId, MaterialId, MaterialLotSpecError, ParticleSizeRange};
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError};

use super::super::MassFlowDurationError;

/// Failure while deriving physically conservative constituent streams from selected feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationBatchError {
    EmptyInput,
    InputFormMismatch {
        expected: FormId,
        found: FormId,
    },
    InputParticleSizeOutsideOperatingRange {
        required: ParticleSizeRange,
        found: ParticleSizeRange,
    },
    SortingInputHostMaterialMismatch {
        expected: MaterialId,
        found: MaterialId,
    },
    UnsupportedResidueForm {
        material: MaterialId,
        form: FormId,
    },
    MissingTargetConstituent {
        material: MaterialId,
    },
    MissingNonTargetConstituent,
    TargetBelowMassResolution {
        material: MaterialId,
        selected: Mass,
    },
    MassOverflow,
    Output(MaterialLotSpecError),
}

impl Display for ConstituentSeparationBatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => {
                formatter.write_str("constituent-separation batch contains no material")
            }
            Self::InputFormMismatch { expected, found } => write!(
                formatter,
                "constituent separation requires input form {} but selected form {}",
                expected.value(),
                found.value()
            ),
            Self::InputParticleSizeOutsideOperatingRange { required, found } => write!(
                formatter,
                "constituent separation requires feed inside {}..={} um but selected feed spans {}..={} um",
                required.minimum_diameter().micrometers(),
                required.maximum_diameter().micrometers(),
                found.minimum_diameter().micrometers(),
                found.maximum_diameter().micrometers()
            ),
            Self::SortingInputHostMaterialMismatch { expected, found } => write!(
                formatter,
                "constituent sorting requires target host material {} but selected commodity uses {}",
                expected.value(),
                found.value()
            ),
            Self::UnsupportedResidueForm { material, form } => write!(
                formatter,
                "constituent separation cannot preserve material {} in residue form {}",
                material.value(),
                form.value()
            ),
            Self::MissingTargetConstituent { material } => write!(
                formatter,
                "constituent separation feed contains no authored target material {}",
                material.value()
            ),
            Self::MissingNonTargetConstituent => formatter
                .write_str("constituent separation requires at least one non-target constituent"),
            Self::TargetBelowMassResolution { material, selected } => write!(
                formatter,
                "selected {} mg contains less than one authoritative milligram of recoverable target material {}",
                selected.milligrams(),
                material.value()
            ),
            Self::MassOverflow => {
                formatter.write_str("constituent-separation output mass overflowed")
            }
            Self::Output(error) => write!(
                formatter,
                "constituent-separation output specification is invalid: {error}"
            ),
        }
    }
}

impl Error for ConstituentSeparationBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),
            Self::EmptyInput
            | Self::InputFormMismatch { .. }
            | Self::InputParticleSizeOutsideOperatingRange { .. }
            | Self::SortingInputHostMaterialMismatch { .. }
            | Self::UnsupportedResidueForm { .. }
            | Self::MissingTargetConstituent { .. }
            | Self::MissingNonTargetConstituent
            | Self::TargetBelowMassResolution { .. }
            | Self::MassOverflow => None,
        }
    }
}

/// Failure while resolving one exact constituent-separation operation before mutation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstituentSeparationResolutionError {
    UnknownProcess {
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
    Batch(ConstituentSeparationBatchError),
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

impl Display for ConstituentSeparationResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "process {} has no authored constituent-separation semantics",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "constituent-separation input failed: {error}"),
            Self::Equipment(error) => write!(
                formatter,
                "constituent-separation equipment failed: {error}"
            ),
            Self::Capability(error) => write!(
                formatter,
                "constituent-separation capability failed: {error}"
            ),
            Self::MissingMassFlowCapability => formatter
                .write_str("constituent-separation equipment has no usable mass-flow capability"),
            Self::MissingMaximumBatchMassCapability => formatter.write_str(
                "constituent-separation equipment has no usable maximum-batch capability",
            ),
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected constituent-separation batch {} mg exceeds equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::Batch(error) => write!(formatter, "constituent-separation batch failed: {error}"),
            Self::Energy(error) => write!(
                formatter,
                "constituent-separation energy supply failed: {error}"
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "constituent separation requires {required:?} energy but source provides {provided:?}"
            ),
            Self::ThroughputDuration(error) => write!(
                formatter,
                "constituent-separation throughput duration failed: {error}"
            ),
            Self::EnergyDuration(error) => write!(
                formatter,
                "constituent-separation energy duration failed: {error}"
            ),
            Self::ConditionDuration(error) => write!(
                formatter,
                "constituent separation exceeds equipment condition lifetime: {error}"
            ),
            Self::Resolution(error) => write!(
                formatter,
                "constituent-separation process resolution failed: {error}"
            ),
        }
    }
}

impl Error for ConstituentSeparationResolutionError {
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
            Self::UnknownProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}
