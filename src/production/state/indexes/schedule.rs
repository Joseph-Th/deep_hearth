//! Due-tick scheduling queries and mutation for derived production indexes.

use std::collections::BTreeSet;

use crate::core::time::SimulationTick;

use super::{ProductionIndexes, ProductionJobId};

impl ProductionIndexes {
    pub(in crate::production::state) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    pub(in crate::production::state) fn scheduled_bucket_count(&self) -> u64 {
        u64::try_from(self.due_jobs.len())
            .unwrap_or_else(|_| unreachable!("production due-bucket count fits memory"))
    }

    pub(in crate::production::state) fn scheduled_bucket_count_where(
        &self,
        mut job_matches: impl FnMut(ProductionJobId) -> bool,
    ) -> u64 {
        let count = self
            .due_jobs
            .values()
            .filter(|jobs| jobs.iter().copied().any(&mut job_matches))
            .count();
        u64::try_from(count)
            .unwrap_or_else(|_| unreachable!("production filtered due-bucket count fits memory"))
    }

    pub(in crate::production::state) fn scheduled_bucket_count_with_additional_tick_where(
        &self,
        additional_tick: SimulationTick,
        mut job_matches: impl FnMut(ProductionJobId) -> bool,
    ) -> u64 {
        let mut matching_buckets = 0_usize;
        let mut additional_tick_already_present = false;
        for (tick, jobs) in &self.due_jobs {
            if !jobs.iter().copied().any(&mut job_matches) {
                continue;
            }
            matching_buckets = matching_buckets
                .checked_add(1)
                .unwrap_or_else(|| unreachable!("production due-bucket count fits memory"));
            additional_tick_already_present |= *tick == additional_tick;
        }
        if !additional_tick_already_present {
            matching_buckets = matching_buckets
                .checked_add(1)
                .unwrap_or_else(|| unreachable!("production due-bucket count fits memory"));
        }
        u64::try_from(matching_buckets)
            .unwrap_or_else(|_| unreachable!("production due-bucket count fits u64"))
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
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
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
    }
}
