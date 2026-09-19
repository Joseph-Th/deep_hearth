//! Derived due-work and equipment-occupancy indexes for mining jobs.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::equipment::EquipmentId;
use crate::inventory::{InventoryState, StockpileId};

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

    /// Returns the number of durable outputs that still require an explicit inventory claim.
    ///
    /// Every retained mining job owns exactly one output parcel until claim retirement. Coalescing
    /// may avoid allocating a fresh lot identity, but continuation budgeting conservatively keeps
    /// one identity and one claim mutation available per job.
    pub(crate) fn future_claim_count(&self) -> u64 {
        u64::try_from(self.jobs.len())
            .unwrap_or_else(|_| unreachable!("resident mining job count fits u64"))
    }

    /// Returns retained outputs currently targeting one stockpile.
    pub(crate) fn future_claim_count_for_destination(&self, destination: StockpileId) -> u64 {
        let count = self
            .jobs
            .values()
            .filter(|job| job.destination() == destination)
            .count();
        u64::try_from(count)
            .unwrap_or_else(|_| unreachable!("resident mining destination claim count fits u64"))
    }

    /// Returns retained outputs whose current destination support makes claim structurally mutating.
    ///
    /// An unmounted stockpile can receive its reserved matter without changing structure state.
    /// Mounting that stockpile later creates this future obligation; unmounting releases it.
    pub(crate) fn future_supported_claim_count(&self, inventory: &InventoryState) -> u64 {
        let count = self
            .jobs
            .values()
            .filter(|job| {
                inventory
                    .get_stockpile(job.destination())
                    .is_some_and(|stockpile| stockpile.supported_by().is_some())
            })
            .count();
        u64::try_from(count)
            .unwrap_or_else(|_| unreachable!("resident supported mining claim count fits u64"))
    }

    /// Returns future mining-owner revisions already owed by retained jobs.
    ///
    /// Each working due bucket needs one transition to ready-to-claim, and every retained job
    /// needs one later claim-retirement revision.
    pub(crate) fn checked_future_revision_demand(&self) -> Option<u64> {
        let due = u64::try_from(self.due_jobs.len())
            .unwrap_or_else(|_| unreachable!("resident mining due-bucket count fits u64"));
        due.checked_add(self.future_claim_count())
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
        u64::try_from(count)
            .unwrap_or_else(|_| unreachable!("mining due-bucket count fits represented memory"))
    }
}

#[cfg(test)]
#[path = "indexes_tests.rs"]
mod tests;
