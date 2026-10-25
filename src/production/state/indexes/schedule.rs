//! Due-tick scheduling queries and mutation for derived production indexes.

use std::collections::BTreeSet;

use crate::core::time::SimulationTick;

use super::{ProductionIndexes, ProductionJobId};

impl ProductionIndexes {
    pub(in crate::production::state) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    pub(in crate::production::state) fn jobs_due_at(
        &self,
        tick: SimulationTick,
    ) -> BTreeSet<ProductionJobId> {
        self.due_jobs.get(&tick).cloned().unwrap_or_default()
    }

    pub(in crate::production::state) fn assert_due_job_present(
        &self,
        id: ProductionJobId,
        due: SimulationTick,
    ) {
        assert_eq!(
            self.due_tick_by_job.get(&id).copied(),
            Some(due),
            "runtime invariant broken: production inverse due index disagrees with job {}",
            id.value()
        );
        assert!(
            self.due_jobs
                .get(&due)
                .is_some_and(|jobs| jobs.contains(&id)),
            "runtime invariant broken: production due index is missing job {}",
            id.value()
        );
    }

    pub(in crate::production::state) fn assert_due_job_absent(&self, id: ProductionJobId) {
        assert!(
            !self.due_tick_by_job.contains_key(&id),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
    }

    pub(in crate::production::state) fn insert_due_job(
        &mut self,
        id: ProductionJobId,
        due: SimulationTick,
    ) {
        self.assert_due_job_absent(id);
        assert!(
            self.due_jobs.entry(due).or_default().insert(id),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
        assert_eq!(
            self.due_tick_by_job.insert(id, due),
            None,
            "runtime invariant broken: production inverse due index replaced job {}",
            id.value()
        );
    }

    pub(in crate::production::state) fn remove_due_job(
        &mut self,
        id: ProductionJobId,
        due: SimulationTick,
    ) {
        self.assert_due_job_present(id, due);
        let remove_bucket = {
            let due_jobs = self.due_jobs.get_mut(&due).unwrap_or_else(|| {
                panic!(
                    "runtime invariant broken: production due index is missing job {}",
                    id.value()
                )
            });
            assert!(
                due_jobs.remove(&id),
                "runtime invariant broken: production due index is missing job {}",
                id.value()
            );
            due_jobs.is_empty()
        };
        if remove_bucket {
            self.due_jobs.remove(&due);
        }
        assert_eq!(
            self.due_tick_by_job.remove(&id),
            Some(due),
            "runtime invariant broken: production inverse due index failed to remove job {}",
            id.value()
        );
    }
}
