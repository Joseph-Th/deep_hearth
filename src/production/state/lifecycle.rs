//! Canonical production-job lifecycle mutations and their owner/index assertions.

use crate::core::time::{SimulationTick, TickSpan};

use super::indexes::ProductionJobIndexProjection;
use super::{
    ProductionJobId, ProductionJobRecord, ProductionState, ProductionSuspension,
    ProductionSuspensionReason,
};

impl ProductionState {
    pub(in crate::production) fn assert_job_insertable(
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

    pub(in crate::production) fn insert_job(
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

    pub(in crate::production) fn assert_suspend_job_available(
        &self,
        id: ProductionJobId,
        suspended_at: SimulationTick,
        remaining_active_time: TickSpan,
    ) {
        let stored = self.jobs.get(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} disappeared before suspension",
                id.value()
            )
        });
        let due = stored.schedule.completes_at;
        assert!(
            stored.schedule.suspension.is_none(),
            "runtime invariant broken: already-suspended job received another suspension"
        );
        assert_eq!(
            due.value().checked_sub(suspended_at.value()),
            Some(remaining_active_time.value()),
            "production suspension must preserve the remaining active-time schedule"
        );
        assert!(
            !remaining_active_time.is_zero(),
            "running production job cannot suspend with zero active time"
        );
        self.indexes.assert_due_job_present(id, due);
    }

    pub(in crate::production) fn suspend_job(
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

    pub(in crate::production) fn assert_resume_job_available(
        &self,
        id: ProductionJobId,
        resumed_at: SimulationTick,
        scheduled_completion: SimulationTick,
    ) -> u64 {
        let record = self.jobs.get(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: production job {} disappeared before resume",
                id.value()
            )
        });
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

    pub(in crate::production) fn resume_job(
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

    pub(in crate::production) fn assert_suspension_reason_change_available(
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

    pub(in crate::production) fn change_suspension_reason(
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

    pub(in crate::production) fn apply_revision(&mut self, next_revision: u64) {
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "production revision must advance exactly once per canonical mutation batch"
        );
        self.revision = next_revision;
    }

    pub(in crate::production) fn assert_job_removable(&self, id: ProductionJobId) {
        let job = self.jobs.get(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: missing production job {}",
                id.value()
            )
        });
        let projection = ProductionJobIndexProjection::from_job(job);
        self.indexes.assert_job_removable(id, &projection);
    }

    pub(in crate::production) fn remove_job(&mut self, id: ProductionJobId) -> ProductionJobRecord {
        self.assert_job_removable(id);
        let job = self.jobs.remove(&id).unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: missing production job {}",
                id.value()
            )
        });
        let projection = ProductionJobIndexProjection::from_job(&job);
        self.indexes.remove_job(id, &projection);
        job
    }
}
