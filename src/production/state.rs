//! Owns durable production jobs, schedules, reservations, and resource-occupancy indexes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::{ConsumedEnergyTrace, EnergyStoreId, ReleasedEnergyTrace};
use crate::equipment::{EquipmentId, EquipmentOperationTrace};
use crate::inventory::{
    ConsumedMaterialTrace, MaterialStorageHistory, StockpileId, checked_consumed_material_mass,
};
use crate::maintenance::Condition;
use crate::material::MaterialLotSpec;

use super::definitions::ProcessId;
use super::resolution::ProcessOutputStreamId;

/// Durable routing for one physically inseparable resolved output stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionOutputStream {
    pub(super) id: ProcessOutputStreamId,
    pub(super) destination: StockpileId,
    pub(super) outputs: Vec<MaterialLotSpec>,
}

/// Why an in-flight production job is currently unable to accumulate active process time.
///
/// Suspension never manufactures a failure product. The production job remains the authoritative
/// owner of its consumed matter and energy until its physical requirements become usable again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ProductionSuspensionReason {
    EquipmentSupportUnavailable { equipment: EquipmentId },
    OutputSupportUnavailable { stockpile: StockpileId },
    PlayerLaborUnavailable,
}

/// When an occupied resource can become available to unrelated work.
///
/// Running jobs have a scheduled wall-clock release. Suspended jobs expose no scheduled release
/// because availability depends on physical recovery and resume.
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct ProductionJobRecord {
    pub(super) identity: ProductionJobIdentity,
    pub(super) schedule: ProductionJobSchedule,
    pub(super) resources: ProductionJobResources,
    pub(super) equipment: ProductionJobEquipment,
    pub(super) output_streams: Vec<ProductionOutputStream>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobIdentity {
    pub(super) id: ProductionJobId,
    pub(super) process: ProcessId,
    pub(super) source: StockpileId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobSchedule {
    pub(super) started_at: SimulationTick,
    pub(super) completes_at: SimulationTick,
    pub(super) active_duration: TickSpan,
    /// Wall-clock suspension time from completed pause intervals.
    ///
    /// The currently active suspension, if any, is deliberately excluded until resume. This keeps
    /// `completes_at = started_at + active_duration + completed_suspension_time` true for both
    /// running and suspended jobs while retaining enough durable history to replay the schedule.
    pub(super) completed_suspension_time: TickSpan,
    pub(super) suspension: Option<ProductionSuspension>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobResources {
    pub(super) consumed_inputs: Vec<ConsumedMaterialTrace>,
    pub(super) material_storage_history: MaterialStorageHistory,
    pub(super) consumed_energy: Option<ConsumedEnergyTrace>,
    pub(super) released_energy: Option<ReleasedEnergyTrace>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProductionJobEquipment {
    pub(super) provider: Option<EquipmentOperationTrace>,
    pub(super) requires_active_support: bool,
    pub(super) condition_after: Option<Condition>,
}

impl ProductionJobRecord {
    #[must_use]
    pub const fn id(&self) -> ProductionJobId {
        self.identity.id
    }

    #[must_use]
    pub const fn process(&self) -> ProcessId {
        self.identity.process
    }

    #[must_use]
    pub const fn source(&self) -> StockpileId {
        self.identity.source
    }

    #[must_use]
    pub const fn started_at(&self) -> SimulationTick {
        self.schedule.started_at
    }

    #[must_use]
    pub const fn completes_at(&self) -> SimulationTick {
        self.schedule.completes_at
    }

    /// Returns the authored/resolved amount of active process time required by this operation.
    /// Wall-clock suspension never changes this physics contract.
    #[must_use]
    pub const fn active_duration(&self) -> TickSpan {
        self.schedule.active_duration
    }

    /// Returns the current suspension state, if this job is retaining work-in-process while paused.
    #[must_use]
    pub const fn suspension(&self) -> Option<ProductionSuspension> {
        self.schedule.suspension
    }

    #[must_use]
    pub const fn is_suspended(&self) -> bool {
        self.schedule.suspension.is_some()
    }

    /// Returns the externally meaningful release horizon for resources exclusively owned by this
    /// job. A suspended operation has no scheduled release until it resumes.
    #[must_use]
    pub const fn occupancy_release(&self) -> ProductionOccupancyRelease {
        if self.schedule.suspension.is_some() {
            ProductionOccupancyRelease::AwaitingResume
        } else {
            ProductionOccupancyRelease::Scheduled(self.schedule.completes_at)
        }
    }

    #[must_use]
    pub fn consumed_mass(&self) -> Mass {
        checked_consumed_material_mass(&self.resources.consumed_inputs).unwrap_or_else(|| {
            panic!(
                "validated production job {} consumed input mass overflowed",
                self.id().value()
            )
        })
    }

    #[must_use]
    pub fn consumed_inputs(&self) -> &[ConsumedMaterialTrace] {
        &self.resources.consumed_inputs
    }

    /// Returns inherited perishability exposure for the in-flight matter, rebased to job start.
    #[must_use]
    pub(crate) const fn material_storage_history(&self) -> MaterialStorageHistory {
        self.resources.material_storage_history
    }

    /// Returns the finite energy moved into this in-flight operation at start, if any.
    #[must_use]
    pub const fn consumed_energy(&self) -> Option<ConsumedEnergyTrace> {
        self.resources.consumed_energy
    }

    /// Returns exact energy released from process inputs and awaiting sink commit, if any.
    #[must_use]
    pub const fn released_energy(&self) -> Option<ReleasedEnergyTrace> {
        self.resources.released_energy
    }

    /// Returns the equipment provider exclusively occupied by this operation, if any.
    #[must_use]
    pub const fn equipment_provider(&self) -> Option<EquipmentOperationTrace> {
        self.equipment.provider
    }

    /// Whether this operation was authorized only while its equipment had an active structural
    /// support. Unsupported/free-standing providers do not acquire this requirement implicitly.
    #[must_use]
    pub const fn has_required_active_support(&self) -> bool {
        self.equipment.requires_active_support
    }

    /// Returns the persisted post-operation condition for the occupied equipment provider.
    #[must_use]
    pub const fn equipment_condition_after(&self) -> Option<Condition> {
        self.equipment.condition_after
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
}

/// Runtime owner for active process jobs and deterministic scheduling/resource indexes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionState {
    revision: u64,
    next_job_id: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    jobs: BTreeMap<ProductionJobId, ProductionJobRecord>,
    #[serde(skip)]
    indexes: ProductionIndexes,
}

impl ProductionState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_job_id: 1,
            jobs: BTreeMap::new(),
            indexes: ProductionIndexes::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) const fn next_job_id(&self) -> u64 {
        self.next_job_id
    }

    pub(crate) fn has_valid_id_cursor(&self) -> bool {
        self.next_job_id != 0
            && self
                .jobs
                .keys()
                .next_back()
                .is_none_or(|highest| highest.value() < self.next_job_id)
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        self.indexes.rebuild(self.jobs.values());
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.indexes.earliest_due_tick()
    }

    pub(super) fn jobs_due_at(&self, tick: SimulationTick) -> BTreeSet<ProductionJobId> {
        self.indexes.jobs_due_at(tick)
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
        self.indexes.energy_occupant(store)
    }

    /// Returns the active production job that exclusively occupies one equipment instance.
    #[must_use]
    pub(crate) fn get_equipment_occupant(
        &self,
        equipment: EquipmentId,
    ) -> Option<&ProductionJobRecord> {
        self.indexes
            .equipment_occupant(equipment)
            .and_then(|job| self.jobs.get(job))
    }

    /// Returns the lowest-ID running production job with in-flight output reserved for a stockpile.
    /// Suspended jobs do not block support relocation because their active-time clock is stopped.
    #[must_use]
    pub(crate) fn get_running_output_stockpile_occupant(
        &self,
        stockpile: StockpileId,
    ) -> Option<&ProductionJobRecord> {
        self.indexes
            .output_stockpile_occupants(stockpile)
            .and_then(|jobs| {
                jobs.iter()
                    .find_map(|job| self.jobs.get(job).filter(|record| !record.is_suspended()))
            })
    }

    pub(super) fn insert_job(
        &mut self,
        job: ProductionJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        let id = job.identity.id;
        assert_eq!(
            id.value(),
            self.next_job_id,
            "production job allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_job_id.checked_add(1),
            Some(next_job_id),
            "production job allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "production job allocation must advance the owner revision exactly once"
        );
        let projection = ProductionJobIndexProjection::from_job(&job);
        assert!(
            !self.jobs.contains_key(&id),
            "validated production job ID must be unique"
        );
        self.indexes.assert_job_available(id, &projection);
        let replaced = self.jobs.insert(id, job);
        assert!(
            replaced.is_none(),
            "prechecked production job ID was replaced"
        );
        self.indexes.insert_job(id, &projection);
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(super) fn suspend_job(
        &mut self,
        id: ProductionJobId,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) {
        let due = match self.jobs.get(&id) {
            Some(record) => record.schedule.completes_at,
            None => panic!(
                "runtime invariant broken: production job {} disappeared before suspension",
                id.value()
            ),
        };
        self.indexes.remove_due_job(id, due);
        let record = match self.jobs.get_mut(&id) {
            Some(record) => record,
            None => unreachable!("production job existence was checked before due-index mutation"),
        };
        assert!(
            record.schedule.suspension.is_none(),
            "runtime invariant broken: already-suspended job received another suspension"
        );
        record.schedule.suspension = Some(ProductionSuspension::new(
            suspended_at,
            remaining_active_time,
            reason,
        ));
    }

    pub(super) fn resume_job(
        &mut self,
        id: ProductionJobId,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    ) {
        let record = match self.jobs.get_mut(&id) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: production job {} disappeared before resume",
                id.value()
            ),
        };
        let suspension = record.schedule.suspension.unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: running job {} received a resume transition",
                id.value()
            )
        });
        let paused_ticks = resumed_at
            .value()
            .checked_sub(suspension.suspended_at().value())
            .unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: production job {} resumed before it suspended",
                    id.value()
                )
            });
        let completed_suspension_time = record
            .schedule
            .completed_suspension_time
            .value()
            .checked_add(paused_ticks)
            .unwrap_or_else(|| {
                panic!(
                    "prevalidated production job {} completed suspension time overflowed",
                    id.value()
                )
            });
        assert_eq!(
            resumed_at.checked_add_span(suspension.remaining_active_time()),
            Some(scheduled_completion),
            "production resume schedule must preserve remaining active time"
        );
        let expected_completion = record
            .schedule
            .started_at
            .checked_add_span(record.schedule.active_duration)
            .and_then(|base| base.checked_add_span(TickSpan::new(completed_suspension_time)));
        assert_eq!(
            expected_completion,
            Some(scheduled_completion),
            "production resume must preserve the durable active-time schedule equation"
        );
        record.schedule.completed_suspension_time = TickSpan::new(completed_suspension_time);
        record.schedule.completes_at = scheduled_completion;
        record.schedule.suspension = None;
        self.indexes.insert_due_job(id, scheduled_completion);
    }

    pub(super) fn change_suspension_reason(
        &mut self,
        id: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    ) {
        let record = self.jobs.get_mut(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} disappeared before suspension reason change",
                id.value()
            )
        });
        let suspension = record.schedule.suspension.as_mut().unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: running job {} received a suspension reason change",
                id.value()
            )
        });
        assert_eq!(
            suspension.reason, previous,
            "runtime invariant broken: production suspension reason changed after planning"
        );
        assert_ne!(previous, reason);
        suspension.reason = reason;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "production revision must advance exactly once per canonical mutation batch"
        );
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
        let projection = ProductionJobIndexProjection::from_job(&job);
        self.indexes.remove_job(id, &projection);
        job
    }
}

mod indexes;
mod validation;

use indexes::{ProductionIndexes, ProductionJobIndexProjection};
pub use validation::ProductionValidationError;
pub(crate) use validation::{
    validate_loaded_production, validate_loaded_production_schedule_history,
};
