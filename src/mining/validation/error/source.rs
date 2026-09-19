//! Nested error-source routing for persisted mining-job validation failures.

use std::error::Error;

use super::MiningJobValidationError;

impl Error for MiningJobValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Duration { error, .. } => Some(error),
            Self::ConditionDuration { error, .. } => Some(error),
            Self::UnknownMethod { .. }
            | Self::UnknownDeposit { .. }
            | Self::UnknownDestination { .. }
            | Self::WorkingEquipmentMissing { .. }
            | Self::UnknownEquipmentDefinition { .. }
            | Self::WorkingEquipmentDefinitionMismatch { .. }
            | Self::WorkingEquipmentRequiresStructuralSupport { .. }
            | Self::WorkingEquipmentMounted { .. }
            | Self::EquipmentConditionMismatch { .. }
            | Self::OutputProfileMismatch { .. }
            | Self::ZeroRequestedMass { .. }
            | Self::OutputMassMismatch { .. }
            | Self::OutputExceedsDepositTrace { .. }
            | Self::WorkingDepositMassMismatch { .. }
            | Self::ReadyDepositMassAbovePostExtraction { .. }
            | Self::OverlappingRetainedWork { .. }
            | Self::DepositHistoryMassIncrease { .. }
            | Self::OutputStorageInvalid { .. }
            | Self::EquipmentAlsoUsedByProduction { .. }
            | Self::EquipmentAlsoUsedByManualPower { .. }
            | Self::MissingCapability { .. }
            | Self::CapabilityKindMismatch { .. }
            | Self::BatchTooLarge { .. }
            | Self::DepositTooHard { .. }
            | Self::ZeroThroughput { .. }
            | Self::InvalidSchedule { .. }
            | Self::DurationMismatch { .. }
            | Self::ConditionOutcomeMismatch { .. }
            | Self::WorkingMiningRevisionExhausted { .. }
            | Self::WorkingGeologyRevisionExhausted { .. }
            | Self::WorkingEquipmentRevisionExhausted { .. } => None,
        }
    }
}
