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

    pub(in crate::production::state) fn scheduled_energy_revision_bucket_count(&self) -> u64 {
        u64::try_from(self.energy_revision_jobs_by_due.len())
            .unwrap_or_else(|_| unreachable!("production energy revision buckets fit memory"))
    }

    pub(in crate::production::state) fn scheduled_equipment_revision_bucket_count(&self) -> u64 {
        u64::try_from(self.equipment_revision_jobs_by_due.len())
            .unwrap_or_else(|_| unreachable!("production equipment revision buckets fit memory"))
    }

    pub(in crate::production::state) fn scheduled_energy_revision_bucket_count_with_tick(
        &self,
        additional_tick: SimulationTick,
    ) -> u64 {
        self.scheduled_energy_revision_bucket_count()
            + u64::from(
                !self
                    .energy_revision_jobs_by_due
                    .contains_key(&additional_tick),
            )
    }

    pub(in crate::production::state) fn scheduled_equipment_revision_bucket_count_with_tick(
        &self,
        additional_tick: SimulationTick,
    ) -> u64 {
        self.scheduled_equipment_revision_bucket_count()
            + u64::from(
                !self
                    .equipment_revision_jobs_by_due
                    .contains_key(&additional_tick),
            )
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

    pub(in crate::production::state) fn insert_due_job_with_requirements(
        &mut self,
        id: ProductionJobId,
        due: SimulationTick,
        requires_energy_revision: bool,
        requires_equipment_revision: bool,
    ) {
        self.insert_due_job(id, due);
        if requires_energy_revision {
            increment_requirement_bucket(&mut self.energy_revision_jobs_by_due, due);
        }
        if requires_equipment_revision {
            increment_requirement_bucket(&mut self.equipment_revision_jobs_by_due, due);
        }
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

    pub(in crate::production::state) fn remove_due_job_with_requirements(
        &mut self,
        id: ProductionJobId,
        due: SimulationTick,
        requires_energy_revision: bool,
        requires_equipment_revision: bool,
    ) {
        self.remove_due_job(id, due);
        if requires_energy_revision {
            decrement_requirement_bucket(&mut self.energy_revision_jobs_by_due, due);
        }
        if requires_equipment_revision {
            decrement_requirement_bucket(&mut self.equipment_revision_jobs_by_due, due);
        }
    }
}

fn increment_requirement_bucket(
    buckets: &mut std::collections::BTreeMap<SimulationTick, u64>,
    due: SimulationTick,
) {
    let count = buckets.entry(due).or_default();
    *count = count
        .checked_add(1)
        .unwrap_or_else(|| unreachable!("resident production job count fits u64"));
}

fn decrement_requirement_bucket(
    buckets: &mut std::collections::BTreeMap<SimulationTick, u64>,
    due: SimulationTick,
) {
    let remove = {
        let count = buckets.get_mut(&due).unwrap_or_else(|| {
            panic!("runtime invariant broken: missing revision-requirement bucket")
        });
        *count = count.checked_sub(1).unwrap_or_else(|| {
            panic!("runtime invariant broken: empty revision-requirement bucket")
        });
        *count == 0
    };
    if remove {
        buckets.remove(&due);
    }
}
