//! Public failure types for manual crafting resolution, admission, and commit.

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

mod display;
mod source;

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
    UnknownManualProcess {
        process: ProcessId,
    },
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
    Access(crate::logistics::PlayerStockpileAccessError),
    Resolution(ManualCraftError),
    Process(StartProcessError),
    Work(PlayerWorkStartError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCraftCommitError {
    StaleLogisticsRevision { expected: u64, actual: u64 },
    Process(StartProcessCommitError),
    Work(PlayerWorkCommitError),
}
