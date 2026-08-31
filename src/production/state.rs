//! Owns durable production jobs, schedules, reservations, and resource-occupancy indexes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::{SimulationTick, TickSpan};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

mod indexes;
mod job;
mod validation;

use indexes::{ProductionIndexes, ProductionJobIndexProjection};
pub(in crate::production) use job::{
    ProductionJobEquipment, ProductionJobIdentity, ProductionJobResources, ProductionJobSchedule,
};
pub use job::{
    ProductionJobId, ProductionJobRecord, ProductionOccupancyRelease, ProductionOutputStream,
    ProductionSuspension, ProductionSuspensionReason,
};
pub use validation::ProductionValidationError;
pub(crate) use validation::{
    validate_loaded_production, validate_loaded_production_schedule_history,
};

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

    pub(super) fn assert_job_insertable(
        &self,
        job: &ProductionJobRecord,
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
        let projection = ProductionJobIndexProjection::from_job(job);
        assert!(
            !self.jobs.contains_key(&id),
            "validated production job ID must be unique"
        );
        self.indexes.assert_job_available(id, &projection);
    }

    pub(super) fn insert_job(
        &mut self,
        job: ProductionJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        let id = job.identity.id;
        self.assert_job_insertable(&job, next_job_id, next_revision);
        let projection = ProductionJobIndexProjection::from_job(&job);
        let replaced = self.jobs.insert(id, job);
        assert!(
            replaced.is_none(),
            "prechecked production job ID was replaced"
        );
        self.indexes.insert_job(id, &projection);
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(super) fn assert_suspend_job_available(
        &self,
        id: ProductionJobId,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
    ) {
        let record = match self.jobs.get(&id) {
            Some(record) => record.schedule.completes_at,
            None => panic!(
                "runtime invariant broken: production job {} disappeared before suspension",
                id.value()
            ),
        };
        let stored = self
            .jobs
            .get(&id)
            .unwrap_or_else(|| unreachable!("job was just checked"));
        assert!(
            stored.schedule.suspension.is_none(),
            "runtime invariant broken: already-suspended job received another suspension"
        );
        assert_eq!(
            record.value().checked_sub(suspended_at.value()),
            Some(remaining_active_time.value()),
            "production suspension must preserve the remaining active-time schedule"
        );
        assert!(
            !remaining_active_time.is_zero(),
            "running production job cannot suspend with zero active time"
        );
        self.indexes.assert_due_job_present(id, record);
    }

    pub(super) fn suspend_job(
        &mut self,
        id: ProductionJobId,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
        reason: ProductionSuspensionReason,
    ) {
        self.assert_suspend_job_available(id, suspended_at, remaining_active_time);
        let due = self
            .jobs
            .get(&id)
            .map(|record| record.schedule.completes_at)
            .unwrap_or_else(|| unreachable!("production suspension job was prechecked"));
        self.indexes.remove_due_job(id, due);
        let record = self
            .jobs
            .get_mut(&id)
            .unwrap_or_else(|| unreachable!("production suspension job was prechecked"));
        record.schedule.suspension = Some(ProductionSuspension::new(
            suspended_at,
            remaining_active_time,
            reason,
        ));
    }

    pub(super) fn assert_resume_job_available(
        &self,
        id: ProductionJobId,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    ) -> u64 {
        let record = match self.jobs.get(&id) {
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
        self.indexes.assert_due_job_absent(id);
        completed_suspension_time
    }

    pub(super) fn resume_job(
        &mut self,
        id: ProductionJobId,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    ) {
        let completed_suspension_time =
            self.assert_resume_job_available(id, resumed_at, scheduled_completion);
        let record = self
            .jobs
            .get_mut(&id)
            .unwrap_or_else(|| unreachable!("production resume job was prechecked"));
        record.schedule.completed_suspension_time = TickSpan::new(completed_suspension_time);
        record.schedule.completes_at = scheduled_completion;
        record.schedule.suspension = None;
        self.indexes.insert_due_job(id, scheduled_completion);
    }

    pub(super) fn assert_suspension_reason_change_available(
        &self,
        id: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    ) {
        let record = self.jobs.get(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} disappeared before suspension reason change",
                id.value()
            )
        });
        let suspension = record.schedule.suspension.unwrap_or_else(|| {
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
    }

    pub(super) fn change_suspension_reason(
        &mut self,
        id: ProductionJobId,
        previous: ProductionSuspensionReason,
        reason: ProductionSuspensionReason,
    ) {
        self.assert_suspension_reason_change_available(id, previous, reason);
        let suspension = self
            .jobs
            .get_mut(&id)
            .and_then(|record| record.schedule.suspension.as_mut())
            .unwrap_or_else(|| unreachable!("production suspension reason change was prechecked"));
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

    pub(super) fn assert_job_removable(&self, id: ProductionJobId) {
        let job = self.jobs.get(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: missing production job {}",
                id.value()
            )
        });
        let projection = ProductionJobIndexProjection::from_job(job);
        self.indexes.assert_job_removable(id, &projection);
    }

    pub(super) fn remove_job(&mut self, id: ProductionJobId) -> ProductionJobRecord {
        self.assert_job_removable(id);
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
