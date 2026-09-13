//! Revision-bound mining job insertion, completion transition, and claim retirement.

use crate::core::time::SimulationTick;

use super::{MiningJobId, MiningJobPhase, MiningJobRecord, MiningState};

impl MiningState {
    pub(crate) fn assert_job_insertable(
        &self,
        record: &MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        assert_eq!(
            record.id().value(),
            self.next_job_id,
            "mining job allocation must consume the current identity cursor"
        );
        assert_eq!(
            self.next_job_id.checked_add(1),
            Some(next_job_id),
            "mining job allocation must advance the identity cursor exactly once"
        );
        assert_eq!(
            self.revision.checked_add(1),
            Some(next_revision),
            "mining job allocation must advance the owner revision exactly once"
        );
        assert!(record.is_working());
        assert!(!self.equipment_occupancy.contains_key(&record.equipment()));
        let id = record.identity.id;
        assert!(
            !self.jobs.contains_key(&id),
            "validated mining job ID must be unique"
        );
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "runtime invariant broken: mining due index already contains job {}",
            id.value()
        );
    }

    pub(crate) fn insert_job(
        &mut self,
        record: MiningJobRecord,
        next_job_id: u64,
        next_revision: u64,
    ) {
        self.assert_job_insertable(&record, next_job_id, next_revision);
        let id = record.identity.id;
        let inserted = self
            .due_jobs
            .entry(record.schedule.completes_at)
            .or_default()
            .insert(id);
        assert!(
            inserted,
            "prechecked mining due index rejected job {}",
            id.value()
        );
        let previous = self.equipment_occupancy.insert(record.equipment(), id);
        assert!(
            previous.is_none(),
            "prechecked mining equipment occupancy replaced an existing job"
        );
        let previous = self.jobs.insert(id, record);
        assert!(previous.is_none(), "prechecked mining job ID was replaced");
        self.next_job_id = next_job_id;
        self.revision = next_revision;
    }

    pub(crate) fn mark_due_jobs_ready(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        completion_tick: SimulationTick,
    ) -> Vec<MiningJobId> {
        self.assert_due_jobs_ready_available(expected_revision, next_revision, completion_tick);
        let jobs = self
            .due_jobs
            .remove(&completion_tick)
            .unwrap_or_else(|| unreachable!("due mining bucket was prechecked"));
        let mut ready = Vec::with_capacity(jobs.len());
        for id in jobs {
            let record = self
                .jobs
                .get_mut(&id)
                .unwrap_or_else(|| unreachable!("due mining job was prechecked"));
            record.schedule.phase = MiningJobPhase::ReadyToClaim;
            let removed = self.equipment_occupancy.remove(&record.equipment());
            assert_eq!(removed, Some(id));
            ready.push(id);
        }
        self.revision = next_revision;
        ready
    }

    pub(crate) fn assert_due_jobs_ready_available(
        &self,
        expected_revision: u64,
        next_revision: u64,
        completion_tick: SimulationTick,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let jobs = self
            .due_jobs
            .get(&completion_tick)
            .unwrap_or_else(|| panic!("validated due mining bucket disappeared"));
        assert!(!jobs.is_empty(), "due mining bucket cannot be empty");
        for &id in jobs {
            let record = self
                .jobs
                .get(&id)
                .unwrap_or_else(|| panic!("validated mining job disappeared"));
            assert!(record.is_working());
            assert_eq!(record.completes_at(), completion_tick);
            assert_eq!(self.equipment_occupancy.get(&record.equipment()), Some(&id));
        }
    }

    pub(crate) fn assert_ready_job_removable(
        &self,
        id: MiningJobId,
        expected_revision: u64,
        next_revision: u64,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let record = self
            .jobs
            .get(&id)
            .unwrap_or_else(|| panic!("validated mining claim job disappeared"));
        assert!(record.is_ready_to_claim());
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "ready mining job remained in due index"
        );
        assert!(
            self.equipment_occupancy.values().all(|job| *job != id),
            "ready mining job retained equipment occupancy"
        );
    }

    pub(crate) fn remove_ready_job(
        &mut self,
        id: MiningJobId,
        expected_revision: u64,
        next_revision: u64,
    ) -> MiningJobRecord {
        self.assert_ready_job_removable(id, expected_revision, next_revision);
        let record = self
            .jobs
            .remove(&id)
            .unwrap_or_else(|| unreachable!("ready mining claim job was prechecked"));
        self.revision = next_revision;
        record
    }
}
