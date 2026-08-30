//! Persistent production-state validation failures.

use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionError, MaterialId};

use super::super::super::resolution::ProcessOutputStreamId;
use super::super::ProductionJobId;

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
    StorageHistoryOverflow {
        job: ProductionJobId,
        at: SimulationTick,
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
    InvalidConsumedInputProvenance {
        job: ProductionJobId,
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
    ZeroOutputMass {
        job: ProductionJobId,
        commodity: CommodityKey,
    },
    InvalidOutputComposition {
        job: ProductionJobId,
        commodity: CommodityKey,
        error: CompositionError,
    },
    OutputCompositionMissingHost {
        job: ProductionJobId,
        host: MaterialId,
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

impl Display for ProductionValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextJobId => formatter.write_str("next production job id must not be zero"),
            Self::ZeroJobId => formatter.write_str("production job id must not be zero"),
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next production job id {next} is not after existing id {}",
                highest.value()
            ),
            Self::JobIdMismatch { key, record } => write!(
                formatter,
                "production map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::JobStartedInFuture {
                job,
                started_at,
                current,
            } => write!(
                formatter,
                "production job {} starts at tick {} after current tick {}",
                job.value(),
                started_at.value(),
                current.value()
            ),
            Self::CompletionNotAfterStart { job } => write!(
                formatter,
                "production job {} does not complete after its start tick",
                job.value()
            ),
            Self::RunningJobAlreadyDue { job, due, current } => write!(
                formatter,
                "running production job {} was due at tick {} by current tick {}",
                job.value(),
                due.value(),
                current.value()
            ),
            Self::ZeroActiveDuration { job } => write!(
                formatter,
                "production job {} has zero required active duration",
                job.value()
            ),
            Self::CompletionScheduleOverflow { job } => write!(
                formatter,
                "production job {} active duration plus completed suspension time exceeds the simulation clock range",
                job.value()
            ),
            Self::CompletionScheduleMismatch {
                job,
                expected_due,
                actual_due,
            } => write!(
                formatter,
                "production job {} active-time history implies due tick {} but stores due tick {}",
                job.value(),
                expected_due.value(),
                actual_due.value()
            ),
            Self::CompletedSuspensionTimeExceedsElapsed {
                job,
                completed,
                elapsed,
            } => write!(
                formatter,
                "production job {} records {} completed suspension ticks after only {} elapsed wall-clock ticks",
                job.value(),
                completed.value(),
                elapsed.value()
            ),
            Self::StorageHistoryTransitionMismatch {
                job,
                transition,
                started_at,
            } => write!(
                formatter,
                "production job {} material storage history is rebased at tick {} instead of start tick {}",
                job.value(),
                transition.value(),
                started_at.value()
            ),
            Self::StorageHistoryOverflow { job, at } => write!(
                formatter,
                "production job {} material storage exposure cannot be represented at tick {}",
                job.value(),
                at.value()
            ),
            Self::RequiredSupportWithoutEquipment { job } => write!(
                formatter,
                "production job {} requires active equipment support but has no equipment provider",
                job.value()
            ),
            Self::SuspensionEquipmentSupportNotRequired { job } => write!(
                formatter,
                "production job {} is suspended for equipment support without requiring active equipment support",
                job.value()
            ),
            Self::ZeroSuspensionRemaining { job } => write!(
                formatter,
                "production job {} suspension retains zero active time",
                job.value()
            ),
            Self::SuspensionBeforeStart {
                job,
                started_at,
                suspended_at,
            } => write!(
                formatter,
                "production job {} claims suspension at tick {} before its start tick {}",
                job.value(),
                suspended_at.value(),
                started_at.value()
            ),
            Self::SuspensionInFuture {
                job,
                current,
                suspended_at,
            } => write!(
                formatter,
                "production job {} claims suspension at tick {} after current tick {}",
                job.value(),
                suspended_at.value(),
                current.value()
            ),
            Self::SuspensionRemainingExceedsActiveDuration {
                job,
                remaining,
                active_duration,
            } => write!(
                formatter,
                "production job {} suspension retains {} active ticks but the operation requires only {} active ticks total",
                job.value(),
                remaining.value(),
                active_duration.value()
            ),
            Self::SuspensionScheduleOverflow { job } => write!(
                formatter,
                "production job {} suspension schedule exceeds simulation tick range",
                job.value()
            ),
            Self::SuspensionScheduleMismatch {
                job,
                expected_due,
                actual_due,
            } => write!(
                formatter,
                "production job {} suspended schedule implies due tick {} but stores due tick {}",
                job.value(),
                expected_due.value(),
                actual_due.value()
            ),
            Self::SuspensionEquipmentMismatch {
                job,
                expected,
                reason,
            } => write!(
                formatter,
                "production job {} suspension references equipment {} but provider is {}",
                job.value(),
                reason.value(),
                expected.value()
            ),
            Self::SuspensionOutputMismatch { job, stockpile } => write!(
                formatter,
                "production job {} suspension references stockpile {} that is not one of its output destinations",
                job.value(),
                stockpile.value()
            ),
            Self::NoOutputs { job } => write!(
                formatter,
                "production job {} owns no in-process output matter",
                job.value()
            ),
            Self::ZeroOutputStreamId { job } => write!(
                formatter,
                "production job {} contains a zero output stream id",
                job.value()
            ),
            Self::DuplicateOutputStreamId { job, stream } => write!(
                formatter,
                "production job {} contains duplicate output stream id {}",
                job.value(),
                stream.value()
            ),
            Self::NonCanonicalOutputStreamOrder { job } => write!(
                formatter,
                "production job {} output streams are not in canonical stream-id order",
                job.value()
            ),
            Self::EmptyOutputStream { job } => write!(
                formatter,
                "production job {} contains an empty output stream",
                job.value()
            ),
            Self::NoConsumedInputs { job } => write!(
                formatter,
                "production job {} has no consumed input traces",
                job.value()
            ),
            Self::ZeroConsumedInputMass { job } => write!(
                formatter,
                "production job {} contains a zero-mass consumed input trace",
                job.value()
            ),
            Self::InvalidConsumedInputComposition { job, error } => write!(
                formatter,
                "production job {} contains invalid consumed input composition: {error}",
                job.value()
            ),
            Self::ConsumedInputCompositionMissingHost { job, host } => write!(
                formatter,
                "production job {} consumed input composition omits host material {}",
                job.value(),
                host.value()
            ),
            Self::InvalidConsumedInputProvenance { job } => write!(
                formatter,
                "production job {} contains an invalid consumed input provenance range",
                job.value()
            ),
            Self::ConsumedInputMassOverflow { job } => write!(
                formatter,
                "production job {} consumed input trace mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::ConsumedInputCreatedAfterStart {
                job,
                latest_created_at,
                started_at,
            } => write!(
                formatter,
                "production job {} consumed input provenance reaches tick {} after job start tick {}",
                job.value(),
                latest_created_at.value(),
                started_at.value()
            ),
            Self::ZeroConsumedEnergy { job } => write!(
                formatter,
                "production job {} traces a zero-energy operation input",
                job.value()
            ),
            Self::InvalidConsumedEnergySource { job } => write!(
                formatter,
                "production job {} traces invalid zero energy-store identity",
                job.value()
            ),
            Self::InvalidConsumedEnergyDefinition { job } => write!(
                formatter,
                "production job {} traces invalid zero energy-store definition identity",
                job.value()
            ),
            Self::ZeroReleasedEnergy { job } => write!(
                formatter,
                "production job {} traces zero released energy",
                job.value()
            ),
            Self::InvalidReleasedEnergyDestination { job } => write!(
                formatter,
                "production job {} traces invalid zero released-energy destination identity",
                job.value()
            ),
            Self::InvalidReleasedEnergyDefinition { job } => write!(
                formatter,
                "production job {} traces invalid zero released-energy definition identity",
                job.value()
            ),
            Self::MissingEquipmentConditionOutcome { job } => write!(
                formatter,
                "production job {} occupies equipment without a post-operation condition outcome",
                job.value()
            ),
            Self::EquipmentConditionWithoutProvider { job } => write!(
                formatter,
                "production job {} stores an equipment condition outcome without a provider",
                job.value()
            ),
            Self::EquipmentConditionImproved { job, before, after } => write!(
                formatter,
                "production job {} improves equipment condition from {} ppm to {} ppm",
                job.value(),
                before.parts_per_million(),
                after.parts_per_million()
            ),
            Self::ZeroOutputMass { job, commodity } => write!(
                formatter,
                "production job {} promises zero mass for material {} form {}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::InvalidOutputComposition {
                job,
                commodity,
                error,
            } => write!(
                formatter,
                "production job {} output material {} form {} has invalid composition: {error}",
                job.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::OutputCompositionMissingHost { job, host } => write!(
                formatter,
                "production job {} output composition omits host material {}",
                job.value(),
                host.value()
            ),
            Self::DuplicateOutputSpecification { job } => write!(
                formatter,
                "production job {} contains duplicate resolved output lot specifications",
                job.value()
            ),
            Self::NonCanonicalOutputOrder { job, stream } => write!(
                formatter,
                "production job {} output stream {} lot specifications are not in canonical order",
                job.value(),
                stream.value()
            ),
            Self::OutputMassMismatch {
                job,
                output,
                consumed,
            } => write!(
                formatter,
                "production job {} owns {} mg output but records {} mg consumed",
                job.value(),
                output.milligrams(),
                consumed.milligrams()
            ),
            Self::OutputMassOverflow { job } => write!(
                formatter,
                "production job {} output mass overflows authoritative quantity storage",
                job.value()
            ),
            Self::MissingDueIndex { job, due } => write!(
                formatter,
                "production job {} is missing from due index tick {}",
                job.value(),
                due.value()
            ),
            Self::UnexpectedDueIndex { job, due } => write!(
                formatter,
                "due index tick {} references inconsistent production job {}",
                due.value(),
                job.value()
            ),
            Self::SuspendedJobInDueIndex { job, due } => write!(
                formatter,
                "suspended production job {} remains indexed for completion at tick {}",
                job.value(),
                due.value()
            ),
            Self::EmptyDueIndex { due } => write!(
                formatter,
                "production due index contains an empty bucket at tick {}",
                due.value()
            ),
            Self::EnergyOccupancyIndexMismatch {
                store,
                indexed,
                expected,
            } => write!(
                formatter,
                "energy occupancy index for store {} records job {:?} but active jobs require {:?}",
                store.value(),
                indexed.map(ProductionJobId::value),
                expected.map(ProductionJobId::value)
            ),
            Self::EnergyDoubleBooked { store } => write!(
                formatter,
                "multiple production jobs exclusively reserve energy store {}",
                store.value()
            ),
            Self::EquipmentOccupancyIndexMismatch {
                equipment,
                indexed,
                expected,
            } => write!(
                formatter,
                "equipment occupancy index for equipment {} records job {:?} but active jobs require {:?}",
                equipment.value(),
                indexed.map(ProductionJobId::value),
                expected.map(ProductionJobId::value)
            ),
            Self::EquipmentDoubleBooked { equipment } => write!(
                formatter,
                "multiple production jobs exclusively occupy equipment {}",
                equipment.value()
            ),
            Self::OutputStockpileOccupancyIndexMismatch { stockpile } => write!(
                formatter,
                "output-stockpile occupancy index for stockpile {} disagrees with active production jobs",
                stockpile.value()
            ),
        }
    }
}

impl Error for ProductionValidationError {}
