//! Failure vocabulary for cross-owner mining-job persistence replay.

use crate::capability::{CapabilityId, CapabilityValueKind};
use crate::core::quantity::{Mass, Pressure};
use crate::core::throughput::MassFlowDurationError;
use crate::core::time::{SimulationTick, TickSpan};
use crate::equipment::EquipmentDefinitionId;
use crate::logistics::{PlayerEquipmentAccessError, PlayerStockpileAccessError};
use crate::maintenance::{ActiveConditionDurationError, Condition};
use crate::spatial::{VoxelBounds, VoxelCoord};

use super::super::MiningJobId;

mod display;
mod source;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MiningJobValidationError {
    UnknownMethod {
        job: MiningJobId,
    },
    UnknownDeposit {
        job: MiningJobId,
    },
    UnknownDestination {
        job: MiningJobId,
    },
    WorkingEquipmentMissing {
        job: MiningJobId,
    },
    UnknownEquipmentDefinition {
        job: MiningJobId,
        definition: EquipmentDefinitionId,
    },
    WorkingEquipmentDefinitionMismatch {
        job: MiningJobId,
        expected: EquipmentDefinitionId,
        actual: EquipmentDefinitionId,
    },
    WorkingEquipmentRequiresStructuralSupport {
        job: MiningJobId,
    },
    WorkingEquipmentMounted {
        job: MiningJobId,
    },
    WorkingPlayerOutsideDeposit {
        job: MiningJobId,
        player_position: VoxelCoord,
        bounds: VoxelBounds,
    },
    WorkingEquipmentAccess {
        job: MiningJobId,
        error: PlayerEquipmentAccessError,
    },
    WorkingDestinationAccess {
        job: MiningJobId,
        error: PlayerStockpileAccessError,
    },
    EquipmentConditionMismatch {
        job: MiningJobId,
    },
    OutputProfileMismatch {
        job: MiningJobId,
    },
    ZeroRequestedMass {
        job: MiningJobId,
    },
    OutputMassMismatch {
        job: MiningJobId,
        requested: Mass,
        available: Mass,
        output: Mass,
    },
    OutputExceedsDepositTrace {
        job: MiningJobId,
        traced: Mass,
        output: Mass,
    },
    WorkingDepositMassMismatch {
        job: MiningJobId,
        expected: Mass,
        actual: Mass,
    },
    ReadyDepositMassAbovePostExtraction {
        job: MiningJobId,
        maximum: Mass,
        actual: Mass,
    },
    OverlappingRetainedWork {
        earlier: MiningJobId,
        later: MiningJobId,
        earlier_completes: SimulationTick,
        later_starts: SimulationTick,
    },
    DepositHistoryMassIncrease {
        earlier: MiningJobId,
        later: MiningJobId,
        maximum_later_mass: Mass,
        later_mass: Mass,
    },
    OutputStorageInvalid {
        job: MiningJobId,
    },
    EquipmentAlsoUsedByProduction {
        job: MiningJobId,
    },
    EquipmentAlsoUsedByManualPower {
        job: MiningJobId,
    },
    MissingCapability {
        job: MiningJobId,
        capability: CapabilityId,
    },
    CapabilityKindMismatch {
        job: MiningJobId,
        capability: CapabilityId,
        expected: CapabilityValueKind,
        found: CapabilityValueKind,
    },
    BatchTooLarge {
        job: MiningJobId,
        maximum: Mass,
        requested: Mass,
    },
    DepositTooHard {
        job: MiningJobId,
        hardness: Pressure,
        maximum: Pressure,
    },
    ZeroThroughput {
        job: MiningJobId,
    },
    Duration {
        job: MiningJobId,
        error: MassFlowDurationError,
    },
    ConditionDuration {
        job: MiningJobId,
        error: ActiveConditionDurationError,
    },
    InvalidSchedule {
        job: MiningJobId,
    },
    DurationMismatch {
        job: MiningJobId,
        stored: TickSpan,
        required: TickSpan,
    },
    ConditionOutcomeMismatch {
        job: MiningJobId,
        stored: Condition,
        required: Condition,
    },
    WorkingMiningRevisionExhausted {
        job: MiningJobId,
    },
    WorkingGeologyRevisionExhausted {
        job: MiningJobId,
    },
    WorkingEquipmentRevisionExhausted {
        job: MiningJobId,
    },
}
