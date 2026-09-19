//! Persistent production-state validation failures.

use std::error::Error;

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::{CompositionError, MaterialId};

use super::super::super::resolution::ProcessOutputStreamId;
use super::super::ProductionJobId;

mod display;

/// Persistent-state validation failure for production records or their due index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionValidationError {
    ZeroNextJobId,
    ZeroJobId,
    NextIdNotAfterExisting {
        next: u64,
        highest: ProductionJobId,
    },
    ConsumedInputCreatedAfterStart {
        job: ProductionJobId,
        latest_created_at: SimulationTick,
        started_at: SimulationTick,
    },
    JobIdMismatch {
        key: ProductionJobId,
        record: ProductionJobId,
    },
    JobStartedInFuture {
        job: ProductionJobId,
        started_at: SimulationTick,
        current: SimulationTick,
    },
    CompletionNotAfterStart {
        job: ProductionJobId,
    },
    RunningJobAlreadyDue {
        job: ProductionJobId,
        due: SimulationTick,
        current: SimulationTick,
    },
    ScheduledRevisionCapacityExhausted {
        revision: u64,
        completion_buckets: u64,
    },
    ZeroActiveDuration {
        job: ProductionJobId,
    },
    CompletionScheduleOverflow {
        job: ProductionJobId,
    },
    CompletionScheduleMismatch {
        job: ProductionJobId,
        expected_due: SimulationTick,
        actual_due: SimulationTick,
    },
    CompletedSuspensionTimeExceedsElapsed {
        job: ProductionJobId,
        completed: TickSpan,
        elapsed: TickSpan,
    },
    StorageHistoryTransitionMismatch {
        job: ProductionJobId,
        transition: SimulationTick,
        started_at: SimulationTick,
    },
    RequiredSupportWithoutEquipment {
        job: ProductionJobId,
    },
    SuspensionEquipmentSupportNotRequired {
        job: ProductionJobId,
    },
    ZeroSuspensionRemaining {
        job: ProductionJobId,
    },
    SuspensionBeforeStart {
        job: ProductionJobId,
        started_at: SimulationTick,
        suspended_at: SimulationTick,
    },
    SuspensionInFuture {
        job: ProductionJobId,
        current: SimulationTick,
        suspended_at: SimulationTick,
    },
    SuspensionRemainingExceedsActiveDuration {
        job: ProductionJobId,
        remaining: TickSpan,
        active_duration: TickSpan,
    },
    SuspensionScheduleOverflow {
        job: ProductionJobId,
    },
    SuspensionScheduleMismatch {
        job: ProductionJobId,
        expected_due: SimulationTick,
        actual_due: SimulationTick,
    },
    SuspensionEquipmentMismatch {
        job: ProductionJobId,
        expected: EquipmentId,
        reason: EquipmentId,
    },
    SuspensionOutputMismatch {
        job: ProductionJobId,
        stockpile: StockpileId,
    },
    NoOutputs {
        job: ProductionJobId,
    },
    ZeroOutputStreamId {
        job: ProductionJobId,
    },
    DuplicateOutputStreamId {
        job: ProductionJobId,
        stream: ProcessOutputStreamId,
    },
    NonCanonicalOutputStreamOrder {
        job: ProductionJobId,
    },
    EmptyOutputStream {
        job: ProductionJobId,
    },
    NoConsumedInputs {
        job: ProductionJobId,
    },
    ZeroConsumedInputMass {
        job: ProductionJobId,
    },
    InvalidConsumedInputComposition {
        job: ProductionJobId,
        error: CompositionError,
    },
    ConsumedInputCompositionMissingHost {
        job: ProductionJobId,
        host: MaterialId,
    },
    ConsumedInputMassOverflow {
        job: ProductionJobId,
    },
    ZeroConsumedEnergy {
        job: ProductionJobId,
    },
    InvalidConsumedEnergySource {
        job: ProductionJobId,
    },
    InvalidConsumedEnergyDefinition {
        job: ProductionJobId,
    },
    ZeroReleasedEnergy {
        job: ProductionJobId,
    },
    InvalidReleasedEnergyDestination {
        job: ProductionJobId,
    },
    InvalidReleasedEnergyDefinition {
        job: ProductionJobId,
    },
    MissingEquipmentConditionOutcome {
        job: ProductionJobId,
    },
    EquipmentConditionWithoutProvider {
        job: ProductionJobId,
    },
    EquipmentConditionImproved {
        job: ProductionJobId,
        before: Condition,
        after: Condition,
    },
    DuplicateOutputSpecification {
        job: ProductionJobId,
    },
    NonCanonicalOutputOrder {
        job: ProductionJobId,
        stream: ProcessOutputStreamId,
    },
    OutputMassOverflow {
        job: ProductionJobId,
    },
    OutputMassMismatch {
        job: ProductionJobId,
        output: Mass,
        consumed: Mass,
    },
    MissingDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    UnexpectedDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    SuspendedJobInDueIndex {
        job: ProductionJobId,
        due: SimulationTick,
    },
    EmptyDueIndex {
        due: SimulationTick,
    },
    FutureMaterialLotIdDemandIndexMismatch {
        indexed: u64,
        expected: u64,
    },
    EnergyOccupancyIndexMismatch {
        store: EnergyStoreId,
        indexed: Option<ProductionJobId>,
        expected: Option<ProductionJobId>,
    },
    EnergyDoubleBooked {
        store: EnergyStoreId,
    },
    EquipmentOccupancyIndexMismatch {
        equipment: EquipmentId,
        indexed: Option<ProductionJobId>,
        expected: Option<ProductionJobId>,
    },
    EquipmentDoubleBooked {
        equipment: EquipmentId,
    },
    OutputStockpileOccupancyIndexMismatch {
        stockpile: StockpileId,
    },
}

impl Error for ProductionValidationError {}
