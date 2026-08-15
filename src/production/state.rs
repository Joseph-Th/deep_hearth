//! Durable production jobs and due-tick index; sibling execution code owns every mutation.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ConsumedEnergyTrace, EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{ConsumedMaterialTrace, StockpileId};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionError, MaterialId, MaterialLotSpec};

use super::definitions::ProcessId;
use super::resolution::ProcessOutputStreamId;

/// Durable routing for one physically inseparable resolved output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionOutputStream {
    pub(super) id: ProcessOutputStreamId,
    pub(super) destination: StockpileId,
    pub(super) outputs: Vec<MaterialLotSpec>,
}

/// Why an in-flight production job is currently unable to accumulate active process time.
///
/// Suspension never manufactures a failure product. The production job remains the authoritative
/// owner of its consumed matter and energy until its physical provider becomes usable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductionSuspensionReason {
    EquipmentSupportUnavailable { equipment: EquipmentId },
}

/// When an occupied resource can become available to unrelated work.
///
/// Running jobs have a scheduled wall-clock release. Suspended jobs deliberately do not expose their
/// stale pre-suspension completion tick as a promise: release depends on physical recovery and resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductionOccupancyRelease {
    Scheduled(SimulationTick),
    AwaitingResume,
}

impl Display for ProductionOccupancyRelease {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheduled(tick) => write!(formatter, "until tick {}", tick.value()),
            Self::AwaitingResume => {
                formatter.write_str("while its production job is suspended awaiting recovery")
            }
        }
    }
}

/// Durable pause state for one production job whose active-time clock is not currently advancing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionSuspension {
    suspended_at: SimulationTick,
    remaining_active_time: TickSpan,
    reason: ProductionSuspensionReason,
}

impl ProductionSuspension {
    pub(super) const fn new(
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) -> Self {
        Self {
            suspended_at,
            remaining_active_time,
            reason,
        }
    }

    #[must_use]
    pub const fn suspended_at(self) -> SimulationTick {
        self.suspended_at
    }

    #[must_use]
    pub const fn remaining_active_time(self) -> TickSpan {
        self.remaining_active_time
    }

    #[must_use]
    pub const fn reason(self) -> ProductionSuspensionReason {
        self.reason
    }
}

impl ProductionOutputStream {
    #[must_use]
    pub const fn id(&self) -> ProcessOutputStreamId {
        self.id
    }

    #[must_use]
    pub const fn destination(&self) -> StockpileId {
        self.destination
    }

    #[must_use]
    pub fn outputs(&self) -> &[MaterialLotSpec] {
        &self.outputs
    }
}

/// Persistent monotonically allocated production job identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProductionJobId(u64);

impl ProductionJobId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "production job id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Durable running material transformation with capacity reserved until completion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionJobRecord {
    pub(super) id: ProductionJobId,
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) active_duration: TickSpan,
    pub(super) suspension: Option<ProductionSuspension>,
    pub(super) consumed_mass: Mass,
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) consumed_energy: Option<ConsumedEnergyTrace>,
    pub(super) released_energy: Option<ReleasedEnergyTrace>,
    pub(super) equipment_provider: Option<EquipmentOperationTrace>,
    pub(super) equipment_requires_active_support: bool,
    pub(super) equipment_condition_after: Option<Condition>,
    pub(super) output_streams: Vec<ProductionOutputStream>,
}

impl ProductionJobRecord {
    #[must_use]
    pub const fn id(&self) -> ProductionJobId {
        self.id
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.source
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.completes_at
    }

    /// Returns the authored/resolved amount of active process time required by this operation.
    /// Wall-clock suspension never changes this physics contract.
    #[must_use]
    pub const fn active_duration(&self) -> TickSpan {
        self.active_duration
    }

    /// Returns the current suspension state, if this job is retaining work-in-process while paused.
    #[must_use]
    pub const fn suspension(&self) -> Option<ProductionSuspension> {
        self.suspension
    }

    #[must_use]
    pub const fn is_suspended(&self) -> bool {
        self.suspension.is_some()
    }

    /// Returns the externally meaningful release horizon for resources exclusively owned by this
    /// job. A suspended operation has no scheduled release until it resumes.
    #[must_use]
    pub const fn occupancy_release(&self) -> ProductionOccupancyRelease {
        if self.suspension.is_some() {
            ProductionOccupancyRelease::AwaitingResume
        } else {
            ProductionOccupancyRelease::Scheduled(self.completes_at)
        }
    }

    #[must_use]
    pub const fn consumed_mass(&self) -> Mass {
        self.consumed_mass
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.consumed_inputs
    }

    /// Returns the finite energy moved into this in-flight operation at start, if any.
    #[must_use]
    pub const fn consumed_energy(&self) -> Option<ConsumedEnergyTrace> {
        self.consumed_energy
    }

    /// Returns exact energy released from process inputs and awaiting sink commit, if any.
    #[must_use]
    pub const fn released_energy(&self) -> Option<ReleasedEnergyTrace> {
        self.released_energy
    }

    /// Returns the equipment provider exclusively occupied by this operation, if any.
    #[must_use]
    pub const fn equipment_provider(&self) -> Option<EquipmentOperationTrace> {
        self.equipment_provider
    }

    /// Whether this operation was authorized only while its equipment had an active structural
    /// support. Unsupported/free-standing providers do not acquire this requirement implicitly.
    #[must_use]
    pub const fn equipment_requires_active_support(&self) -> bool {
        self.equipment_requires_active_support
    }

    /// Returns the persisted post-operation condition for the occupied equipment provider.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment_condition_after
    }

    /// Returns exact material streams and their committed destinations.
    #[must_use]
    pub fn output_streams(&self) -> &[ProductionOutputStream] {
        &self.output_streams
    }

    /// Returns the sole durable stream for process families that require single-stream output.
    #[must_use]
    pub fn single_output_stream(&self) -> Option<&ProductionOutputStream> {
        let [stream] = self.output_streams.as_slice() else {
            return None;
        };
        Some(stream)
    }

    pub(crate) fn involves_stockpile(&self, stockpile: StockpileId) -> bool {
        self.source == stockpile
            || self
                .output_streams
                .iter()
                .any(|stream| stream.destination == stockpile)
    }
}

/// Runtime owner for active process jobs and the deterministic due-tick index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionState {
    pub(super) revision: u64,
    pub(super) next_job_id: u64,
    pub(super) jobs: BTreeMap<ProductionJobId, ProductionJobRecord>,
    pub(super) due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
    energy_occupancy: BTreeMap<EnergyStoreId, ProductionJobId>,
}

impl ProductionState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            due_jobs: BTreeMap::new(),
            energy_occupancy: BTreeMap::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) const fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
    }

    pub(crate) fn has_valid_equipment_condition_outcomes(&self) -> bool {
        self.jobs.values().all(|job| {
            match (job.equipment_provider, job.equipment_condition_after) {
                (Some(provider), Some(after)) => after <= provider.condition(),
                (None, None) => true,
                (Some(_), None) | (None, Some(_)) => false,
            }
        })
    }

    pub(crate) fn has_valid_schedule_index(&self) -> bool {
        if self.due_jobs.values().any(BTreeSet::is_empty) {
            return false;
        }
        for (id, job) in &self.jobs {
            if job.completes_at <= job.started_at || job.active_duration.value() == 0 {
                return false;
            }
            if job.equipment_requires_active_support && job.equipment_provider.is_none() {
                return false;
            }
            match job.suspension {
                Some(suspension) => {
                    if !job.equipment_requires_active_support
                        || suspension.remaining_active_time().value() == 0
                        || suspension.remaining_active_time().value() > job.active_duration.value()
                        || suspension.suspended_at() < job.started_at
                        || suspension
                            .suspended_at()
                            .checked_add_span(suspension.remaining_active_time())
                            != Some(job.completes_at)
                    {
                        return false;
                    }
                    match (suspension.reason(), job.equipment_provider) {
                        (
                            ProductionSuspensionReason::EquipmentSupportUnavailable { equipment },
                            Some(provider),
                        ) if equipment == provider.equipment() => {}
                        (
                            ProductionSuspensionReason::EquipmentSupportUnavailable { .. },
                            Some(_) | None,
                        ) => return false,
                    }
                }
                None => {
                    if !self
                        .due_jobs
                        .get(&job.completes_at)
                        .is_some_and(|ids| ids.contains(id))
                    {
                        return false;
                    }
                }
            }
        }
        self.due_jobs.iter().all(|(due, ids)| {
            ids.iter().all(|id| {
                self.jobs
                    .get(id)
                    .is_some_and(|job| job.suspension.is_none() && job.completes_at == *due)
            })
        })
    }

    pub(crate) fn has_unique_energy_reservations(&self) -> bool {
        self.expected_energy_occupancy().is_some()
    }

    pub(crate) fn has_valid_energy_occupancy_index(&self) -> bool {
        self.expected_energy_occupancy()
            .is_some_and(|expected| expected == self.energy_occupancy)
    }

    fn expected_energy_occupancy(&self) -> Option<BTreeMap<EnergyStoreId, ProductionJobId>> {
        let mut occupied = BTreeMap::new();
        for job in self.jobs.values() {
            if let Some(trace) = job.consumed_energy
                && occupied.insert(trace.source(), job.id).is_some()
            {
                return None;
            }
            if let Some(trace) = job.released_energy
                && occupied.insert(trace.destination(), job.id).is_some()
            {
                return None;
            }
        }
        Some(occupied)
    }

    fn energy_occupancy_mismatch(
        &self,
    ) -> Option<(
        EnergyStoreId,
        Option<ProductionJobId>,
        Option<ProductionJobId>,
    )> {
        let expected = self.expected_energy_occupancy()?;
        let stores = self
            .energy_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for store in stores {
            let indexed = self.energy_occupancy.get(&store).copied();
            let expected = expected.get(&store).copied();
            if indexed != expected {
                return Some((store, indexed, expected));
            }
        }
        None
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    /// Returns one active process job by stable runtime ID.
    #[must_use]
    pub fn get_job(&self, id: ProductionJobId) -> Option<&ProductionJobRecord> {
        self.jobs.get(&id)
    }

    /// Iterates active jobs deterministically by stable runtime ID.
    pub fn jobs(&self) -> impl Iterator<Item = &ProductionJobRecord> {
        self.jobs.values()
    }

    /// Returns the active production job that exclusively reserves one finite energy store.
    #[must_use]
    pub(crate) fn get_energy_occupant(&self, store: EnergyStoreId) -> Option<ProductionJobId> {
        self.energy_occupancy.get(&store).copied()
    }

    pub(super) fn insert_job(
        &mut self,
        job: ProductionJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        let id = job.id;
        let completes_at = job.completes_at;
        let consumed_energy_store = job.consumed_energy.map(|trace| trace.source());
        let released_energy_store = job.released_energy.map(|trace| trace.destination());
        if let (Some(consumed), Some(released)) = (consumed_energy_store, released_energy_store) {
            assert_ne!(
                consumed, released,
                "validated production job cannot reserve one energy store as both source and sink"
            );
        }
        for store in consumed_energy_store
            .into_iter()
            .chain(released_energy_store)
        {
            assert!(
                !self.energy_occupancy.contains_key(&store),
                "validated production job cannot replace an existing energy-store reservation"
            );
        }
        let replaced = self.jobs.insert(id, job);
        assert!(
            replaced.is_none(),
            "validated production job ID must be unique"
        );
        let inserted = self.due_jobs.entry(completes_at).or_default().insert(id);
        assert!(
            inserted,
            "production due index must not contain duplicate job IDs"
        );
        for store in consumed_energy_store
            .into_iter()
            .chain(released_energy_store)
        {
            let previous = self.energy_occupancy.insert(store, id);
            debug_assert!(previous.is_none());
        }
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(super) fn remove_job(&mut self, id: ProductionJobId) -> ProductionJobRecord {
        let job = match self.jobs.remove(&id) {
            Some(job) => job,
            None => panic!(
                "runtime invariant broken: missing production job {}",
                id.value()
            ),
        };
        let due_set = match self.due_jobs.get_mut(&job.completes_at) {
            Some(due_set) => due_set,
            None => panic!(
                "runtime invariant broken: missing due index for production job {}",
                id.value()
            ),
        };
        assert!(
            due_set.remove(&id),
            "runtime invariant broken: due index missing production job {}",
            id.value()
        );
        if due_set.is_empty() {
            self.due_jobs.remove(&job.completes_at);
        }
        for store in job
            .consumed_energy
            .map(|trace| trace.source())
            .into_iter()
            .chain(job.released_energy.map(|trace| trace.destination()))
        {
            let removed = self.energy_occupancy.remove(&store);
            assert_eq!(
                removed,
                Some(id),
                "runtime invariant broken: energy occupancy index disagrees with production job {}",
                id.value()
            );
        }
        job
    }
}

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
    CompletionNotAfterStart {
        job: ProductionJobId,
    },
    ZeroActiveDuration {
        job: ProductionJobId,
    },
    RequiredSupportWithoutEquipment {
        job: ProductionJobId,
    },
    SuspensionWithoutRequiredSupport {
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
    ConsumedInputMassMismatch {
        job: ProductionJobId,
        traced: Mass,
        consumed: Mass,
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
            Self::CompletionNotAfterStart { job } => write!(
                formatter,
                "production job {} does not complete after its start tick",
                job.value()
            ),
            Self::ZeroActiveDuration { job } => write!(
                formatter,
                "production job {} has zero required active duration",
                job.value()
            ),
            Self::RequiredSupportWithoutEquipment { job } => write!(
                formatter,
                "production job {} requires active equipment support but has no equipment provider",
                job.value()
            ),
            Self::SuspensionWithoutRequiredSupport { job } => write!(
                formatter,
                "production job {} is suspended for equipment support without an active-support requirement",
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
            Self::ConsumedInputMassMismatch {
                job,
                traced,
                consumed,
            } => write!(
                formatter,
                "production job {} traces {} mg of consumed input but records {} mg consumed",
                job.value(),
                traced.milligrams(),
                consumed.milligrams()
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
        }
    }
}

impl Error for ProductionValidationError {}

pub(crate) fn validate_loaded_production(
    state: &ProductionState,
) -> Result<(), ProductionValidationError> {
    if state.next_job_id == 0 {
        return Err(ProductionValidationError::ZeroNextJobId);
    }
    if let Some(highest) = state.jobs.keys().next_back().copied()
        && state.next_job_id <= highest.value()
    {
        return Err(ProductionValidationError::NextIdNotAfterExisting {
            next: state.next_job_id,
            highest,
        });
    }

    for (id, job) in &state.jobs {
        if id.value() == 0 || job.id.value() == 0 {
            return Err(ProductionValidationError::ZeroJobId);
        }
        if *id != job.id {
            return Err(ProductionValidationError::JobIdMismatch {
                key: *id,
                record: job.id,
            });
        }
        if job.completes_at <= job.started_at {
            return Err(ProductionValidationError::CompletionNotAfterStart { job: *id });
        }
        if job.active_duration.value() == 0 {
            return Err(ProductionValidationError::ZeroActiveDuration { job: *id });
        }
        if job.equipment_requires_active_support && job.equipment_provider.is_none() {
            return Err(ProductionValidationError::RequiredSupportWithoutEquipment { job: *id });
        }
        if let Some(suspension) = job.suspension {
            if !job.equipment_requires_active_support {
                return Err(
                    ProductionValidationError::SuspensionWithoutRequiredSupport { job: *id },
                );
            }
            if suspension.remaining_active_time().value() == 0 {
                return Err(ProductionValidationError::ZeroSuspensionRemaining { job: *id });
            }
            if suspension.suspended_at() < job.started_at {
                return Err(ProductionValidationError::SuspensionBeforeStart {
                    job: *id,
                    started_at: job.started_at,
                    suspended_at: suspension.suspended_at(),
                });
            }
            if suspension.remaining_active_time().value() > job.active_duration.value() {
                return Err(
                    ProductionValidationError::SuspensionRemainingExceedsActiveDuration {
                        job: *id,
                        remaining: suspension.remaining_active_time(),
                        active_duration: job.active_duration,
                    },
                );
            }
            let expected_due = suspension
                .suspended_at()
                .checked_add_span(suspension.remaining_active_time())
                .ok_or(ProductionValidationError::SuspensionScheduleOverflow { job: *id })?;
            if expected_due != job.completes_at {
                return Err(ProductionValidationError::SuspensionScheduleMismatch {
                    job: *id,
                    expected_due,
                    actual_due: job.completes_at,
                });
            }
            match suspension.reason() {
                ProductionSuspensionReason::EquipmentSupportUnavailable { equipment } => {
                    let expected = match job.equipment_provider {
                        Some(provider) => provider.equipment(),
                        None => {
                            return Err(
                                ProductionValidationError::RequiredSupportWithoutEquipment {
                                    job: *id,
                                },
                            );
                        }
                    };
                    if equipment != expected {
                        return Err(ProductionValidationError::SuspensionEquipmentMismatch {
                            job: *id,
                            expected,
                            reason: equipment,
                        });
                    }
                }
            }
        }
        if job.output_streams.is_empty() {
            return Err(ProductionValidationError::NoOutputs { job: *id });
        }
        if job.consumed_inputs.is_empty() {
            return Err(ProductionValidationError::NoConsumedInputs { job: *id });
        }
        let mut traced_input_mass = Mass::ZERO;
        for trace in &job.consumed_inputs {
            if trace.mass().is_zero() {
                return Err(ProductionValidationError::ZeroConsumedInputMass { job: *id });
            }
            trace.profile().composition().validate().map_err(|error| {
                ProductionValidationError::InvalidConsumedInputComposition { job: *id, error }
            })?;
            let host = trace.profile().commodity().material();
            if trace.profile().composition().parts_per_million(host) == 0 {
                return Err(
                    ProductionValidationError::ConsumedInputCompositionMissingHost {
                        job: *id,
                        host,
                    },
                );
            }
            if trace.provenance().latest_created_at() < trace.provenance().earliest_created_at() {
                return Err(ProductionValidationError::InvalidConsumedInputProvenance { job: *id });
            }
            if trace.provenance().latest_created_at() > job.started_at {
                return Err(ProductionValidationError::ConsumedInputCreatedAfterStart {
                    job: *id,
                    latest_created_at: trace.provenance().latest_created_at(),
                    started_at: job.started_at,
                });
            }
            traced_input_mass = traced_input_mass
                .checked_add(trace.mass())
                .ok_or(ProductionValidationError::ConsumedInputMassOverflow { job: *id })?;
        }
        if traced_input_mass != job.consumed_mass {
            return Err(ProductionValidationError::ConsumedInputMassMismatch {
                job: *id,
                traced: traced_input_mass,
                consumed: job.consumed_mass,
            });
        }
        if let Some(trace) = job.consumed_energy {
            if trace.energy().is_zero() {
                return Err(ProductionValidationError::ZeroConsumedEnergy { job: *id });
            }
            if trace.source().value() == 0 {
                return Err(ProductionValidationError::InvalidConsumedEnergySource { job: *id });
            }
            if trace.definition().value() == 0 {
                return Err(ProductionValidationError::InvalidConsumedEnergyDefinition {
                    job: *id,
                });
            }
        }
        if let Some(trace) = job.released_energy {
            if trace.energy().is_zero() {
                return Err(ProductionValidationError::ZeroReleasedEnergy { job: *id });
            }
            if trace.destination().value() == 0 {
                return Err(
                    ProductionValidationError::InvalidReleasedEnergyDestination { job: *id },
                );
            }
            if trace.definition().value() == 0 {
                return Err(ProductionValidationError::InvalidReleasedEnergyDefinition {
                    job: *id,
                });
            }
        }
        match (job.equipment_provider, job.equipment_condition_after) {
            (Some(provider), Some(after)) => {
                if after > provider.condition() {
                    return Err(ProductionValidationError::EquipmentConditionImproved {
                        job: *id,
                        before: provider.condition(),
                        after,
                    });
                }
            }
            (Some(_), None) => {
                return Err(
                    ProductionValidationError::MissingEquipmentConditionOutcome { job: *id },
                );
            }
            (None, Some(_)) => {
                return Err(
                    ProductionValidationError::EquipmentConditionWithoutProvider { job: *id },
                );
            }
            (None, None) => {}
        }
        let mut output_mass = Mass::ZERO;
        let mut output_stream_ids = BTreeSet::new();
        let mut previous_stream_id = None;
        for stream in &job.output_streams {
            if stream.id.value() == 0 {
                return Err(ProductionValidationError::ZeroOutputStreamId { job: *id });
            }
            if !output_stream_ids.insert(stream.id) {
                return Err(ProductionValidationError::DuplicateOutputStreamId {
                    job: *id,
                    stream: stream.id,
                });
            }
            if previous_stream_id.is_some_and(|previous| previous > stream.id) {
                return Err(ProductionValidationError::NonCanonicalOutputStreamOrder { job: *id });
            }
            previous_stream_id = Some(stream.id);
            if stream.outputs.is_empty() {
                return Err(ProductionValidationError::EmptyOutputStream { job: *id });
            }
            let mut seen_outputs = BTreeSet::new();
            let mut previous_output = None;
            for output in &stream.outputs {
                if output.mass().is_zero() {
                    return Err(ProductionValidationError::ZeroOutputMass {
                        job: *id,
                        commodity: output.commodity(),
                    });
                }
                output.composition().validate().map_err(|error| {
                    ProductionValidationError::InvalidOutputComposition {
                        job: *id,
                        commodity: output.commodity(),
                        error,
                    }
                })?;
                if output
                    .composition()
                    .parts_per_million(output.commodity().material())
                    == 0
                {
                    return Err(ProductionValidationError::OutputCompositionMissingHost {
                        job: *id,
                        host: output.commodity().material(),
                    });
                }
                if !seen_outputs.insert(output.clone()) {
                    return Err(ProductionValidationError::DuplicateOutputSpecification {
                        job: *id,
                    });
                }
                if previous_output.is_some_and(|previous: &MaterialLotSpec| previous > output) {
                    return Err(ProductionValidationError::NonCanonicalOutputOrder {
                        job: *id,
                        stream: stream.id,
                    });
                }
                previous_output = Some(output);
                output_mass = output_mass
                    .checked_add(output.mass())
                    .ok_or(ProductionValidationError::OutputMassOverflow { job: *id })?;
            }
        }
        if output_mass != job.consumed_mass {
            return Err(ProductionValidationError::OutputMassMismatch {
                job: *id,
                output: output_mass,
                consumed: job.consumed_mass,
            });
        }
        let is_indexed = state
            .due_jobs
            .get(&job.completes_at)
            .is_some_and(|ids| ids.contains(id));
        if job.suspension.is_some() && is_indexed {
            return Err(ProductionValidationError::SuspendedJobInDueIndex {
                job: *id,
                due: job.completes_at,
            });
        }
        if job.suspension.is_none() && !is_indexed {
            return Err(ProductionValidationError::MissingDueIndex {
                job: *id,
                due: job.completes_at,
            });
        }
    }

    for (due, ids) in &state.due_jobs {
        if ids.is_empty() {
            return Err(ProductionValidationError::EmptyDueIndex { due: *due });
        }
        for id in ids {
            let Some(job) = state.jobs.get(id) else {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            };
            if job.suspension.is_some() {
                return Err(ProductionValidationError::SuspendedJobInDueIndex {
                    job: *id,
                    due: *due,
                });
            }
            if job.completes_at != *due {
                return Err(ProductionValidationError::UnexpectedDueIndex {
                    job: *id,
                    due: *due,
                });
            }
        }
    }
    if let Some((store, indexed, expected)) = state.energy_occupancy_mismatch() {
        return Err(ProductionValidationError::EnergyOccupancyIndexMismatch {
            store,
            indexed,
            expected,
        });
    }
    Ok(())
}
