//! Public failure types for manual crafting resolution, admission, and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::Mass;
use crate::core::throughput::MassFlowDurationError;
use crate::equipment::{EquipmentDefinitionId, EquipmentId, EquipmentProviderError};
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::maintenance::ActiveConditionDurationError;
use crate::material::{CommodityKey, MaterialLotSpecError};
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolutionError, StartProcessCommitError,
    StartProcessError,
};

use super::batch::ManualCraftBatchError;

/// Failure while projecting authored equipment-assisted manual work without a runtime provider.
///
/// This projection answers physical schedule questions only. It does not prove that an equipment
/// instance exists, is supported or idle, that material is available, or that player labor can
/// start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftEquipmentProjectionError {
    UnknownManualProcess {
        process: ProcessId,
    },
    EquipmentNotSupported {
        process: ProcessId,
    },
    UnknownEquipmentDefinition {
        equipment: EquipmentDefinitionId,
    },
    MissingEquipmentCapability {
        equipment: EquipmentDefinitionId,
        capability: CapabilityId,
    },
    EquipmentCapabilityKindMismatch {
        equipment: EquipmentDefinitionId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    InputMassOverflow {
        process: ProcessId,
        batches: NonZeroU64,
    },
    EquipmentDuration(MassFlowDurationError),
    EquipmentCondition(ActiveConditionDurationError),
}

/// Failure while projecting equipment-free manual work from immutable authored definitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualCraftHandProjectionError {
    EquipmentRequired {
        process: ProcessId,
    },
    DurationOverflow {
        process: ProcessId,
        batches: NonZeroU64,
    },
    ResourceBudgetOverflow {
        process: ProcessId,
        batches: NonZeroU64,
    },
}

impl Display for ManualCraftHandProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EquipmentRequired { process } => write!(
                formatter,
                "manual craft process {} has no equipment-free hand-work route",
                process.value()
            ),
            Self::DurationOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} hand-work duration overflows for {} batches",
                process.value(),
                batches.get()
            ),
            Self::ResourceBudgetOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} physiological hand-work budget overflows for {} batches",
                process.value(),
                batches.get()
            ),
        }
    }
}

impl Error for ManualCraftHandProjectionError {}

impl Display for ManualCraftEquipmentProjectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownManualProcess { process } => {
                write!(
                    formatter,
                    "process {} is not authored as a manual craft",
                    process.value()
                )
            }
            Self::EquipmentNotSupported { process } => write!(
                formatter,
                "manual craft process {} has no authored equipment-assisted path",
                process.value()
            ),
            Self::UnknownEquipmentDefinition { equipment } => write!(
                formatter,
                "manual craft projection references unknown equipment definition {}",
                equipment.value()
            ),
            Self::MissingEquipmentCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "equipment definition {} does not provide required shaping capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "equipment definition {} capability {} has {found:?} value instead of mass throughput",
                equipment.value(),
                capability.value()
            ),
            Self::InputMassOverflow { process, batches } => write!(
                formatter,
                "manual craft process {} input mass overflows when projected for {} batches",
                process.value(),
                batches.get()
            ),
            Self::EquipmentDuration(error) => {
                write!(
                    formatter,
                    "manual craft projection cannot schedule work: {error}"
                )
            }
            Self::EquipmentCondition(error) => {
                write!(
                    formatter,
                    "manual craft projection cannot remain productive: {error}"
                )
            }
        }
    }
}

impl Error for ManualCraftEquipmentProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EquipmentDuration(error) => Some(error),
            Self::EquipmentCondition(error) => Some(error),
            Self::UnknownManualProcess { .. }
            | Self::EquipmentNotSupported { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::InputMassOverflow { .. } => None,
        }
    }
}

/// Failure while resolving one exact manual shaping operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftError {
    SurvivalNotInitialized,
    PlayerDead,
    UnknownManualProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
    EmptyInput,
    InputCommodityMismatch {
        expected: CommodityKey,
    },
    InputCompositionMismatch {
        expected: CommodityKey,
    },
    MixedInputTemperature,
    InputMassNotWholeBatches {
        consumed: Mass,
        batch_mass: Mass,
    },
    DurationOverflow {
        batches: NonZeroU64,
    },
    RequiredEquipmentMissing {
        process: ProcessId,
    },
    EquipmentNotSupported {
        process: ProcessId,
        equipment: EquipmentId,
    },
    Equipment(EquipmentProviderError),
    MissingEquipmentCapability {
        equipment: EquipmentId,
        capability: CapabilityId,
    },
    EquipmentCapabilityKindMismatch {
        equipment: EquipmentId,
        capability: CapabilityId,
        found: CapabilityValueKind,
    },
    EquipmentDuration(MassFlowDurationError),
    EquipmentCondition(ActiveConditionDurationError),
    OutputMassOverflow {
        commodity: CommodityKey,
        batches: NonZeroU64,
    },
    Output(MaterialLotSpecError),
    Resolution(ProcessResolutionError),
}

impl Display for ManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurvivalNotInitialized => {
                formatter.write_str("manual crafting requires initialized player survival")
            }
            Self::PlayerDead => formatter.write_str("dead player cannot perform manual crafting"),
            Self::UnknownManualProcess { process } => write!(
                formatter,
                "process {} is not authored as a manual craft",
                process.value()
            ),
            Self::Input(error) => write!(formatter, "manual craft input is invalid: {error}"),
            Self::EmptyInput => formatter.write_str("manual craft selection is empty"),
            Self::InputCommodityMismatch { expected } => write!(
                formatter,
                "manual craft selection contains matter other than authored material {} form {}",
                expected.material().value(),
                expected.form().value()
            ),
            Self::InputCompositionMismatch { expected } => write!(
                formatter,
                "manual craft selection for material {} form {} must be pure host material",
                expected.material().value(),
                expected.form().value()
            ),
            Self::MixedInputTemperature => formatter.write_str(
                "manual shaping cannot combine different input temperatures without thermal physics",
            ),
            Self::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => write!(
                formatter,
                "manual craft selection contains {} mg, which is not a whole number of {} mg authored batches",
                consumed.milligrams(),
                batch_mass.milligrams()
            ),
            Self::DurationOverflow { batches } => write!(
                formatter,
                "manual shaping duration overflows when repeated {} times",
                batches.get()
            ),
            Self::RequiredEquipmentMissing { process } => write!(
                formatter,
                "manual craft process {} requires compatible physical equipment",
                process.value()
            ),
            Self::EquipmentNotSupported { process, equipment } => write!(
                formatter,
                "manual craft process {} has no authored equipment-assisted path for equipment {}",
                process.value(),
                equipment.value()
            ),
            Self::Equipment(error) => write!(formatter, "manual craft equipment is unavailable: {error}"),
            Self::MissingEquipmentCapability {
                equipment,
                capability,
            } => write!(
                formatter,
                "manual craft equipment {} does not provide required shaping capability {}",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentCapabilityKindMismatch {
                equipment,
                capability,
                found,
            } => write!(
                formatter,
                "manual craft equipment {} capability {} has {found:?} value instead of mass throughput",
                equipment.value(),
                capability.value()
            ),
            Self::EquipmentDuration(error) => {
                write!(formatter, "manual craft equipment throughput cannot schedule work: {error}")
            }
            Self::EquipmentCondition(error) => {
                write!(formatter, "manual craft equipment cannot remain productive: {error}")
            }
            Self::OutputMassOverflow {
                commodity,
                batches,
            } => write!(
                formatter,
                "manual shaping output material {} form {} overflows when repeated {} times",
                commodity.material().value(),
                commodity.form().value(),
                batches.get()
            ),
            Self::Output(error) => write!(formatter, "manual craft output is invalid: {error}"),
            Self::Resolution(error) => write!(formatter, "manual craft resolution is invalid: {error}"),
        }
    }
}

impl Error for ManualCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Output(error) => Some(error),
            Self::Resolution(error) => Some(error),
            Self::Equipment(error) => Some(error),
            Self::EquipmentDuration(error) => Some(error),
            Self::EquipmentCondition(error) => Some(error),
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::EmptyInput
            | Self::UnknownManualProcess { process: _ }
            | Self::InputCommodityMismatch { .. }
            | Self::InputCompositionMismatch { .. }
            | Self::MixedInputTemperature
            | Self::InputMassNotWholeBatches { .. }
            | Self::DurationOverflow { batches: _ }
            | Self::RequiredEquipmentMissing { .. }
            | Self::EquipmentNotSupported { .. }
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::OutputMassOverflow { .. } => None,
        }
    }
}

impl ManualCraftError {
    pub(super) fn from_batch_error(
        error: ManualCraftBatchError,
        definition: &super::ManualCraftDefinition,
    ) -> Self {
        match error {
            ManualCraftBatchError::EmptyInput => Self::EmptyInput,
            ManualCraftBatchError::InputCommodityMismatch => Self::InputCommodityMismatch {
                expected: definition.input(),
            },
            ManualCraftBatchError::InputCompositionMismatch => Self::InputCompositionMismatch {
                expected: definition.input(),
            },
            ManualCraftBatchError::MixedInputTemperature => Self::MixedInputTemperature,
            ManualCraftBatchError::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            } => Self::InputMassNotWholeBatches {
                consumed,
                batch_mass,
            },
        }
    }
}

/// Failure while admitting manual shaping into production and exclusive player labor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartManualCraftError {
    Resolution(ManualCraftError),
    Process(StartProcessError),
    Work(PlayerWorkStartError),
}

impl Display for StartManualCraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolution(error) => write!(formatter, "manual craft resolution failed: {error}"),
            Self::Process(error) => write!(formatter, "manual craft start failed: {error}"),
            Self::Work(error) => write!(formatter, "manual craft labor is unavailable: {error}"),
        }
    }
}

impl Error for StartManualCraftError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resolution(error) => Some(error),
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftCommitError {
    Process(StartProcessCommitError),
    Work(PlayerWorkCommitError),
}

impl Display for ManualCraftCommitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => {
                write!(formatter, "manual craft process commit failed: {error}")
            }
            Self::Work(error) => write!(formatter, "manual craft labor commit failed: {error}"),
        }
    }
}

impl Error for ManualCraftCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}
