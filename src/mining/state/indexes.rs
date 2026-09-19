//! Derived due-work and equipment-occupancy indexes for mining jobs.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;

use super::{MiningJobId, MiningState};

impl MiningState {
    pub(crate) fn rebuild_derived_indexes(&mut self) {
        let mut due_jobs = BTreeMap::<SimulationTick, BTreeSet<MiningJobId>>::new();
        let mut equipment_occupancy = BTreeMap::<EquipmentId, MiningJobId>::new();
        for job in self.jobs.values().filter(|job| job.is_working()) {
            due_jobs
                .entry(job.completes_at())
                .or_default()
                .insert(job.id());
            equipment_occupancy
                .entry(job.equipment())
                .or_insert(job.id());
        }
        self.due_jobs = due_jobs;
        self.equipment_occupancy = equipment_occupancy;
    }

    pub(crate) fn get_equipment_occupant(&self, equipment: EquipmentId) -> Option<MiningJobId> {
        self.equipment_occupancy.get(&equipment).copied()
    }

    pub(crate) fn jobs_due_at(&self, tick: SimulationTick) -> Option<&BTreeSet<MiningJobId>> {
        self.due_jobs.get(&tick)
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    /// Returns the number of scheduled completion ticks that will mutate equipment condition.
    ///
    /// Completed and ready-to-claim jobs remain in durable history, so future revision budgeting
    /// must use the due-work index rather than rescanning every retained mining record.
    pub(crate) fn scheduled_equipment_revision_bucket_count(&self) -> u64 {
        let count = self
            .due_jobs
            .values()
            .filter(|jobs| {
                jobs.iter().any(|id| {
                    let job = self.jobs.get(id).unwrap_or_else(|| {
                        panic!("runtime invariant broken: mining due index references missing job")
                    });
                    job.equipment_condition_after() != job.equipment_condition_before()
                })
            })
            .count();
        u64::try_from(count).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
#[path = "indexes_tests.rs"]
mod tests;
