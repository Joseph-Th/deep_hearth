//! Diagnostics for shared powered ore equipment, timing, and trusted-load replay.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Energy, Mass};
use crate::energy::{EnergyCarrier, PowerDurationError};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::ore_processing::MassFlowDurationError;

/// Failure while resolving condition-adjusted equipment limits for one powered ore batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreEquipmentError {
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded { selected: Mass, maximum: Mass },
}

/// Corruption or authored-physics drift shared by every persisted powered ore-processing job.
///
/// Process-specific output replay remains in the owning process module. This error owns only the
/// common finite-energy equipment, throughput, timing, and wear contract so those process families
/// cannot silently diverge during trusted-load validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreJobValidationError {
    MissingEnergy,
    UnexpectedReleasedEnergy,
    MissingEquipmentProvider,
    UnknownEquipmentDefinition,
    UnknownEnergyDefinition,
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    BatchMassExceeded {
        selected: Mass,
        maximum: Mass,
    },
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    EnergyMismatch {
        traced: Energy,
        required: Energy,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    ConditionDuration(ActiveConditionDurationError),
    DurationMismatch {
        stored_ticks: u64,
        required_ticks: u64,
    },
    MissingConditionOutcome,
    ConditionOutcomeMismatch {
        stored: Condition,
        required: Condition,
    },
}

impl Display for PoweredOreJobValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingEnergy => formatter.write_str("missing consumed work-energy trace"),
            Self::UnexpectedReleasedEnergy => formatter
                .write_str("contains released energy not authorized by powered ore processing"),
            Self::MissingEquipmentProvider => {
                formatter.write_str("missing occupied equipment provider")
            }
            Self::UnknownEquipmentDefinition => {
                formatter.write_str("references an unknown equipment definition")
            }
            Self::UnknownEnergyDefinition => {
                formatter.write_str("references an unknown energy-store definition")
            }
            Self::MissingMassFlowCapability => {
                formatter.write_str("equipment lacks the authored mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => {
                formatter.write_str("equipment lacks the authored maximum-batch capability")
            }
            Self::BatchMassExceeded { selected, maximum } => write!(
                formatter,
                "selected {} mg above the traced equipment maximum {} mg",
                selected.milligrams(),
                maximum.milligrams()
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "requires {required:?} energy but traces {provided:?}"
            ),
            Self::EnergyMismatch { traced, required } => write!(
                formatter,
                "traces {} nJ but mass-specific work requires {} nJ",
                traced.nanojoules(),
                required.nanojoules()
            ),
            Self::ThroughputDuration(error) => {
                write!(formatter, "cannot recompute throughput duration: {error}")
            }
            Self::EnergyDuration(error) => write!(
                formatter,
                "cannot recompute energy-delivery duration: {error}"
            ),
            Self::ConditionDuration(error) => {
                write!(formatter, "exceeds equipment condition lifetime: {error}")
            }
            Self::DurationMismatch {
                stored_ticks,
                required_ticks,
            } => write!(
                formatter,
                "stores duration {stored_ticks} ticks but physics require {required_ticks}"
            ),
            Self::MissingConditionOutcome => {
                formatter.write_str("has no persisted equipment-condition outcome")
            }
            Self::ConditionOutcomeMismatch { stored, required } => write!(
                formatter,
                "stores equipment condition {} ppm but physics require {} ppm",
                stored.parts_per_million(),
                required.parts_per_million()
            ),
        }
    }
}

impl Error for PoweredOreJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::ConditionDuration(error) => Some(error),
            Self::MissingEnergy
            | Self::UnexpectedReleasedEnergy
            | Self::MissingEquipmentProvider
            | Self::UnknownEquipmentDefinition
            | Self::UnknownEnergyDefinition
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::BatchMassExceeded { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::EnergyMismatch { .. }
            | Self::DurationMismatch { .. }
            | Self::MissingConditionOutcome
            | Self::ConditionOutcomeMismatch { .. } => None,
        }
    }
}

/// Failure while resolving common active-time and wear physics after equipment admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ore_processing) enum PoweredOreTimingError {
    Throughput(MassFlowDurationError),
    Energy(PowerDurationError),
    Condition(ActiveConditionDurationError),
}
