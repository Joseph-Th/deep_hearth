//! Nested error-source chaining for public manual-crafting failures.

use std::error::Error;

use super::{
    ManualCraftCommitError, ManualCraftEquipmentProjectionError, ManualCraftError,
    ManualCraftHandProjectionError, StartManualCraftError,
};

impl Error for ManualCraftHandProjectionError {}

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
            | Self::UnknownManualProcess { .. }
            | Self::InputCommodityMismatch { .. }
            | Self::InputCompositionMismatch { .. }
            | Self::MixedInputTemperature
            | Self::InputMassNotWholeBatches { .. }
            | Self::DurationOverflow { .. }
            | Self::RequiredEquipmentMissing { .. }
            | Self::EquipmentNotSupported { .. }
            | Self::MissingEquipmentCapability { .. }
            | Self::EquipmentCapabilityKindMismatch { .. }
            | Self::OutputMassOverflow { .. } => None,
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

impl Error for ManualCraftCommitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Work(error) => Some(error),
        }
    }
}
