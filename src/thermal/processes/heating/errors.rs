//! Diagnostics for sensible-heating process resolution.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::{CapabilityEvaluationError, CapabilityId};
use crate::core::quantity::{Mass, Temperature};
use crate::energy::{EnergyCarrier, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::MaterialLotSpecError;
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError};
use crate::thermal::PhaseSensibleHeatError;

/// Failure while resolving exact material heating into a startable production outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SensibleHeatingResolutionError {
    UnknownThermalProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    MissingHeatingPower {
        capability: CapabilityId,
    },
    MissingMaximumTemperature {
        capability: CapabilityId,
    },
    MissingMaximumBatchMass {
        capability: CapabilityId,
    },
    TargetExceedsEquipmentMaximum {
        target: Temperature,
        maximum: Temperature,
    },
    BatchMassExceedsEquipmentCapacity {
        selected: Mass,
        maximum: Mass,
    },
    TargetBelowInputTemperature {
        current: Temperature,
        target: Temperature,
    },
    Heat(PhaseSensibleHeatError),
    RequiredEnergyOverflow,
    NoHeatingRequired,
    Energy(EnergySupplyError),
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    Duration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for SensibleHeatingResolutionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownThermalProcess { process } => write!(
                formatter,
                "process {} has no sensible-heating resolver definition",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "process input binding failed: {error}"),
            Self::Equipment(error) => write!(formatter, "equipment resolution failed: {error}"),
            Self::Capability(error) => {
                write!(formatter, "equipment capability check failed: {error}")
            }
            Self::MissingHeatingPower { capability } => write!(
                formatter,
                "equipment does not expose configured heating-power capability {}",
                capability.value()
            ),
            Self::MissingMaximumTemperature { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-temperature capability {}",
                capability.value()
            ),
            Self::MissingMaximumBatchMass { capability } => write!(
                formatter,
                "equipment does not expose configured maximum-batch-mass capability {}",
                capability.value()
            ),
            Self::TargetExceedsEquipmentMaximum { target, maximum } => write!(
                formatter,
                "target {} mK exceeds equipment maximum {} mK",
                target.millikelvin(),
                maximum.millikelvin()
            ),
            Self::BatchMassExceedsEquipmentCapacity { selected, maximum } => write!(
                formatter,
                "selected batch {} mg exceeds equipment capacity {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::TargetBelowInputTemperature { current, target } => write!(
                formatter,
                "sensible-heating target {} mK is below selected input temperature {} mK",
                target.millikelvin(),
                current.millikelvin()
            ),
            Self::Heat(error) => write!(formatter, "sensible-heat calculation failed: {error}"),
            Self::RequiredEnergyOverflow => {
                formatter.write_str("required sensible heat overflowed")
            }
            Self::NoHeatingRequired => {
                formatter.write_str("selected matter is already at target temperature")
            }
            Self::Energy(error) => write!(formatter, "finite energy supply failed: {error}"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "sensible-heating process requires {required:?} energy but store provides {provided:?}"
            ),
            Self::Duration(error) => {
                write!(formatter, "heating duration calculation failed: {error}")
            }
            Self::ConditionDuration(error) => write!(
                formatter,
                "heating exceeds equipment condition lifetime: {error}"
            ),
            Self::Output(error) => write!(formatter, "heated output construction failed: {error}"),
            Self::Resolution(error) => write!(formatter, "process resolution failed: {error}"),
        }
    }
}

impl Error for SensibleHeatingResolutionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Heat(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::Duration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownThermalProcess { .. }
            | Self::MissingHeatingPower { .. }
            | Self::MissingMaximumTemperature { .. }
            | Self::MissingMaximumBatchMass { .. }
            | Self::TargetExceedsEquipmentMaximum { .. }
            | Self::BatchMassExceedsEquipmentCapacity { .. }
            | Self::TargetBelowInputTemperature { .. }
            | Self::RequiredEnergyOverflow
            | Self::NoHeatingRequired
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}
