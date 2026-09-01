//! Public failure types for manual crafting resolution, admission, and commit.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use crate::core::quantity::Mass;
use crate::labor::{PlayerWorkCommitError, PlayerWorkStartError};
use crate::material::{CommodityKey, MaterialLotSpecError};
use crate::production::{
    ProcessId, ProcessInputError, ProcessResolutionError, StartProcessCommitError,
    StartProcessError,
};

use super::batch::ManualCraftBatchError;

/// Failure while resolving one exact manual shaping operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftError {
    SurvivalNotInitialized,
    PlayerDead,
    UnknownManualProcess {
        process: ProcessId,
    },
    Input(ProcessInputError),
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
            Self::SurvivalNotInitialized
            | Self::PlayerDead
            | Self::UnknownManualProcess { process: _ }
            | Self::InputCommodityMismatch { .. }
            | Self::InputCompositionMismatch { .. }
            | Self::MixedInputTemperature
            | Self::InputMassNotWholeBatches { .. }
            | Self::DurationOverflow { batches: _ }
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
