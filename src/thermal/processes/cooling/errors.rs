//! Typed runtime and persisted sensible-cooling errors.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId};
use crate::core::quantity::{Energy, Mass, Temperature};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergySinkError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError, ProductionJobId};

use super::SensibleCoolingBatchError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleCoolingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Batch(SensibleCoolingBatchError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingCoolingPower {
        capability: CapabilityId,
    },
    MissingMaximumTemperature {
        capability: CapabilityId,
    },
    MissingMaximumBatchMass {
        capability: CapabilityId,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    InputTemperatureExceedsEquipmentMaximum {
        input: Temperature,
        maximum: Temperature,
    },
    EnergySink(EnergySinkError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Resolution(ProcessResolutionError),
}

impl Display for SensibleCoolingResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                f,
                "process {} has no solid-cooling definition",
                process.value()
            ),
            Self::Input(error) => write!(f, "solid-cooling input invalid: {error}"),
            Self::Batch(error) => write!(f, "solid-cooling batch invalid: {error}"),
            Self::Equipment(error) => write!(f, "cooling equipment unavailable: {error}"),
            Self::Capability(error) => {
                write!(f, "cooling equipment capabilities insufficient: {error}")
            }
            Self::MissingCoolingPower { capability } => write!(
                f,
                "equipment lacks cooling-power capability {}",
                capability.value()
            ),
            Self::MissingMaximumTemperature { capability } => write!(
                f,
                "equipment lacks maximum-temperature capability {}",
                capability.value()
            ),
            Self::MissingMaximumBatchMass { capability } => write!(
                f,
                "equipment lacks batch-capacity capability {}",
                capability.value()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                f,
                "cooling batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::InputTemperatureExceedsEquipmentMaximum { input, maximum } => write!(
                f,
                "input {} mK exceeds cooling equipment maximum {} mK",
                input.millikelvin(),
                maximum.millikelvin()
            ),
            Self::EnergySink(error) => write!(f, "cooling heat receiver unavailable: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                f,
                "cooling requires {required:?} heat sink, found {provided:?}"
            ),
            Self::Duration(error) => write!(f, "cooling timing failed: {error}"),
            Self::ConditionDuration(error) => {
                write!(f, "cooling exceeds equipment condition lifetime: {error}")
            }
            Self::Resolution(error) => write!(f, "cooling resolution failed: {error}"),
        }
    }
}

impl Error for SensibleCoolingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Batch(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::EnergySink(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { .. }
            | Self::MissingCoolingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::InputTemperatureExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleCoolingJobValidationError {
    UnexpectedConsumedEnergy {
        job: ProductionJobId,
    },
    MissingReleasedEnergy {
        job: ProductionJobId,
    },
    MissingEquipmentProvider {
        job: ProductionJobId,
    },
    UnknownEquipmentDefinition {
        job: ProductionJobId,
    },
    UnknownEnergyDefinition {
        job: ProductionJobId,
    },
    Capability {
        job: ProductionJobId,
        error: CapabilityEvaluationError,
    },
    MissingCoolingPower {
        job: ProductionJobId,
    },
    MissingMaximumTemperature {
        job: ProductionJobId,
    },
    MissingMaximumBatchMass {
        job: ProductionJobId,
    },
    BatchMassExceedsEquipmentCapacity {
        job: ProductionJobId,
        selected: Mass,
        maximum: Mass,
    },
    InputTemperatureExceedsEquipmentMaximum {
        job: ProductionJobId,
        input: Temperature,
        maximum: Temperature,
    },
    WrongEnergyCarrier {
        job: ProductionJobId,
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Batch {
        job: ProductionJobId,
        error: SensibleCoolingBatchError,
    },
    EnergyMismatch {
        job: ProductionJobId,
        traced: Energy,
        required: Energy,
    },
    Duration {
        job: ProductionJobId,
        error: PowerDurationError,
    },
    ConditionDuration {
        job: ProductionJobId,
        error: ActiveConditionDurationError,
    },
    DurationMismatch {
        job: ProductionJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionMismatch {
        job: ProductionJobId,
        stored: Condition,
        required: Condition,
    },
    OutputMismatch {
        job: ProductionJobId,
    },
}

impl Display for SensibleCoolingJobValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "persisted solid cooling violates thermal contract: {self:?}"
        )
    }
}

impl Error for SensibleCoolingJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Capability { error, .. } => Some(error),
            Self::Duration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::Batch { error, .. } => Some(error),
            Self::UnexpectedConsumedEnergy { .. }
            | Self::MissingReleasedEnergy { .. }
            | Self::MissingEquipmentProvider { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::UnknownEnergyDefinition { .. }
            | Self::MissingCoolingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::InputTemperatureExceedsEquipmentMaximum { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingEquipmentConditionOutcome { .. }
            | Self::EquipmentConditionMismatch { .. }
            | Self::OutputMismatch { .. } => None,
        }
    }
}
