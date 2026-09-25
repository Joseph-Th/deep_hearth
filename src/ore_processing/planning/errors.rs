//! Typed failures for current-state powered-ore mass planning.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::energy::{EnergyCarrier, EnergySupplyError};
use crate::equipment::EquipmentProviderError;
use crate::production::ProcessId;

/// Failure to derive a shared powered-ore mass envelope from current observable owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredOreMassEnvelopeError {
    UnknownPoweredProcess {
        process: ProcessId,
    },
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    Energy(EnergySupplyError),
    MissingMassFlowCapability,
    MissingMaximumBatchMassCapability,
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
}

impl Display for PoweredOreMassEnvelopeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPoweredProcess { process } => write!(
                formatter,
                "process {} has no authored powered ore-processing profile",
                process.value()
            ),
            Self::Equipment(error) => {
                write!(formatter, "powered ore provider unavailable: {error}")
            }
            Self::Capability(error) => {
                write!(formatter, "powered ore provider capability failed: {error}")
            }
            Self::Energy(error) => {
                write!(formatter, "powered ore energy supply unavailable: {error}")
            }
            Self::MissingMassFlowCapability => {
                formatter.write_str("powered ore provider lacks its authored mass-flow capability")
            }
            Self::MissingMaximumBatchMassCapability => formatter
                .write_str("powered ore provider lacks its authored maximum-batch capability"),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered ore process requires {required:?} energy but supply provides {provided:?}"
            ),
        }
    }
}

impl Error for PoweredOreMassEnvelopeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::UnknownPoweredProcess { .. }
            | Self::MissingMassFlowCapability
            | Self::MissingMaximumBatchMassCapability
            | Self::WrongEnergyCarrier { .. } => None,
        }
    }
}
