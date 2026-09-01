//! Owns durable production jobs, schedules, reservations, and resource-occupancy indexes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

mod indexes;
mod job;
mod lifecycle;
mod validation;

use indexes::ProductionIndexes;
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
}
