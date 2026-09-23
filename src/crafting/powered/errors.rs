//! Typed failures for powered crafting resolution and admission.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::capability::CapabilityEvaluationError;
use crate::core::quantity::Mass;
use crate::core::throughput::MassFlowDurationError;
use crate::energy::{EnergyCarrier, EnergyStoreId, EnergySupplyError, PowerDurationError};
use crate::equipment::EquipmentProviderError;
use crate::maintenance::ActiveConditionDurationError;
use crate::material::MaterialLotSpecError;
use crate::production::{ProcessId, ProcessInputError, ProcessResolutionError, StartProcessError};

use crate::crafting::batch::ManualCraftBatchError;

/// Failure while resolving an unattended machine crafting operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoweredCraftError {
    UnknownProcess {
        process: ProcessId,
    },
    MissingTransform {
        process: ProcessId,
    },
    Input(ProcessInputError),
    EmptyInput,
    InputCommodityMismatch,
    InputCompositionMismatch,
    MixedInputTemperature,
    InputMassNotWholeBatches {
        consumed: Mass,
        batch_mass: Mass,
    },
    Equipment(EquipmentProviderError),
    Capability(CapabilityEvaluationError),
    Energy(EnergySupplyError),
    EnergyCapacityExceeded {
        store: EnergyStoreId,
        capacity: crate::core::quantity::Energy,
        requested: crate::core::quantity::Energy,
    },
    WrongEnergyCarrier {
        required: EnergyCarrier,
        provided: EnergyCarrier,
    },
    ThroughputDuration(MassFlowDurationError),
    EnergyDuration(PowerDurationError),
    EquipmentCondition(ActiveConditionDurationError),
    OutputMassOverflow,
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for PoweredCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProcess { process } => write!(
                formatter,
                "unknown powered craft process {}",
                process.value()
            ),
            Self::MissingTransform { process } => write!(
                formatter,
                "powered craft process {} lost its material transform",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "powered craft input is invalid: {error}"),
            Self::EmptyInput => formatter.write_str("powered craft selection is empty"),
            Self::InputCommodityMismatch => formatter
                .write_str("powered craft input commodity does not match its authored transform"),
            Self::InputCompositionMismatch => formatter
                .write_str("powered craft input composition does not match its authored transform"),
            Self::MixedInputTemperature => {
                formatter.write_str("powered craft cannot combine mixed input temperatures")
            }
            Self::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "powered craft selected {} mg, not a whole number of {} mg transform batches",
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::Equipment(error) => {
                write!(formatter, "powered craft equipment is unavailable: {error}")
            }
            Self::Capability(error) => {
                write!(
                    formatter,
                    "powered craft equipment capability failed: {error}"
                )
            }
            Self::Energy(error) => write!(
                formatter,
                "powered craft energy supply is unavailable: {error}"
            ),
            Self::EnergyCapacityExceeded {
                store,
                capacity,
                requested,
            } => write!(
                formatter,
                "powered craft requires {} nJ but energy store {} can hold at most {} nJ",
                requested.nanojoules(),
                store.value(),
                capacity.nanojoules()
            ),
            Self::WrongEnergyCarrier { required, provided } => write!(
                formatter,
                "powered craft requires {required:?} energy but selected store supplies {provided:?}"
            ),
            Self::ThroughputDuration(error) => write!(
                formatter,
                "powered craft throughput cannot schedule work: {error}"
            ),
            Self::EnergyDuration(error) => write!(
                formatter,
                "powered craft energy delivery cannot schedule work: {error}"
            ),
            Self::EquipmentCondition(error) => write!(
                formatter,
                "powered craft equipment cannot remain productive: {error}"
            ),
            Self::OutputMassOverflow => formatter.write_str("powered craft output mass overflowed"),
            Self::Output(error) => write!(formatter, "powered craft output is invalid: {error}"),
            Self::Resolution(error) => {
                write!(formatter, "powered craft resolution is invalid: {error}")
            }
        }
    }
}

impl Error for PoweredCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::Capability(error) => Some(error),
            Self::Energy(error) => Some(error),
            Self::ThroughputDuration(error) => Some(error),
            Self::EnergyDuration(error) => Some(error),
            Self::EquipmentCondition(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::UnknownProcess { .. }
            | Self::MissingTransform { .. }
            | Self::EmptyInput
            | Self::InputCommodityMismatch
            | Self::InputCompositionMismatch
            | Self::MixedInputTemperature
            | Self::InputMassNotWholeBatches { .. }
            | Self::EnergyCapacityExceeded { .. }
            | Self::WrongEnergyCarrier { .. }
            | Self::OutputMassOverflow => None,
        }
    }
}

pub(super) fn batch_error(error: ManualCraftBatchError) -> PoweredCraftError {
    match error {
        ManualCraftBatchError::EmptyInput => PoweredCraftError::EmptyInput,
        ManualCraftBatchError::InputCommodityMismatch => PoweredCraftError::InputCommodityMismatch,
        ManualCraftBatchError::InputCompositionMismatch => {
            PoweredCraftError::InputCompositionMismatch
        }
        ManualCraftBatchError::MixedInputTemperature => PoweredCraftError::MixedInputTemperature,
        ManualCraftBatchError::InputMassNotWholeBatches {
            consumed,
            batch_mass,
        } => PoweredCraftError::InputMassNotWholeBatches {
            consumed,
            batch_mass,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartPoweredCraftError {
    Resolution(PoweredCraftError),
    Process(StartProcessError),
}

impl Display for StartPoweredCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => {
                write!(formatter, "powered craft resolution failed: {error}")
            }
            Self::Process(error) => write!(formatter, "powered craft start failed: {error}"),
        }
    }
}

impl Error for StartPoweredCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::Process(error) => Some(error),
        }
    }
}
