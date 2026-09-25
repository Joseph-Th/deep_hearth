//! Owns durable production jobs, schedules, reservations, and resource-occupancy indexes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::{InventoryState, StockpileId};

mod indexes;
mod job;
mod lifecycle;
mod validation;

pub(in crate::production) use indexes::ProductionAvailabilityDependencyRevisions;
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
        self.indexes.rebuild(self.jobs.iter());
    }

    pub(in crate::production) fn physical_availability_dependencies_changed(
        &self,
        current: ProductionAvailabilityDependencyRevisions,
    ) -> bool {
        self.indexes
            .physical_availability_dependencies_changed(current)
    }

    pub(in crate::production) fn record_physical_availability_dependencies(
        &mut self,
        current: ProductionAvailabilityDependencyRevisions,
    ) {
        self.indexes
            .record_physical_availability_dependencies(current);
    }

    pub(in crate::production) fn player_labor_suspended_jobs(
        &self,
    ) -> impl Iterator<Item = ProductionJobId> + '_ {
        self.indexes.player_labor_suspended_jobs()
    }

    pub(in crate::production) fn physical_availability_candidate_jobs(
        &self,
        inventory: &InventoryState,
    ) -> BTreeSet<ProductionJobId> {
        self.indexes
            .physical_availability_candidate_jobs(inventory.all_supported_stockpiles())
    }

    pub(crate) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.indexes.earliest_due_tick()
    }

    pub(crate) fn has_scheduled_revision_capacity_from(&self, revision: u64) -> bool {
        revision
            .checked_add(self.scheduled_completion_bucket_count())
            .is_some()
    }

    pub(in crate::production) fn has_scheduled_revision_capacity_with_tick_from(
        &self,
        revision: u64,
        completes_at: SimulationTick,
    ) -> bool {
        revision
            .checked_add(
                self.indexes
                    .scheduled_bucket_count_with_additional_tick_where(completes_at, |_| true),
            )
            .is_some()
    }

    pub(crate) fn scheduled_completion_bucket_count(&self) -> u64 {
        self.indexes.scheduled_bucket_count()
    }

    pub(crate) const fn future_material_lot_id_demand(&self) -> u64 {
        self.indexes.future_material_lot_id_demand()
    }

    pub(in crate::production) fn has_scheduled_revision_capacity(&self) -> bool {
        self.has_scheduled_revision_capacity_from(self.revision)
    }

    pub(crate) fn has_scheduled_equipment_revision_capacity_from(&self, revision: u64) -> bool {
        revision
            .checked_add(self.scheduled_equipment_revision_bucket_count())
            .is_some()
    }

    pub(crate) fn scheduled_equipment_revision_bucket_count(&self) -> u64 {
        self.indexes.scheduled_equipment_revision_bucket_count()
    }

    pub(in crate::production) fn has_scheduled_equipment_revision_capacity_with_tick_from(
        &self,
        revision: u64,
        completes_at: SimulationTick,
    ) -> bool {
        revision
            .checked_add(
                self.indexes
                    .scheduled_equipment_revision_bucket_count_with_tick(completes_at),
            )
            .is_some()
    }

    pub(crate) fn has_scheduled_released_energy_revision_capacity_from(
        &self,
        revision: u64,
    ) -> bool {
        revision
            .checked_add(self.scheduled_released_energy_revision_bucket_count())
            .is_some()
    }

    pub(crate) fn scheduled_released_energy_revision_bucket_count(&self) -> u64 {
        self.indexes.scheduled_energy_revision_bucket_count()
    }

    pub(in crate::production) fn has_scheduled_released_energy_revision_capacity_with_tick_from(
        &self,
        revision: u64,
        completes_at: SimulationTick,
    ) -> bool {
        revision
            .checked_add(
                self.indexes
                    .scheduled_energy_revision_bucket_count_with_tick(completes_at),
            )
            .is_some()
    }

    pub(crate) fn has_scheduled_supported_output_revision_capacity_from(
        &self,
        revision: u64,
        inventory: &InventoryState,
    ) -> bool {
        revision
            .checked_add(self.scheduled_supported_output_revision_bucket_count(inventory))
            .is_some()
    }

    pub(crate) fn scheduled_supported_output_revision_bucket_count(
        &self,
        inventory: &InventoryState,
    ) -> u64 {
        self.scheduled_completion_bucket_count_where(|job| {
            job.requires_structure_revision_at_completion(inventory)
        })
    }

    pub(in crate::production) fn has_scheduled_supported_output_revision_capacity_with_tick_from(
        &self,
        revision: u64,
        inventory: &InventoryState,
        completes_at: SimulationTick,
    ) -> bool {
        revision
            .checked_add(
                self.scheduled_completion_bucket_count_with_tick_where(completes_at, |job| {
                    job.requires_structure_revision_at_completion(inventory)
                }),
            )
            .is_some()
    }

    fn scheduled_completion_bucket_count_where(
        &self,
        mut predicate: impl FnMut(&ProductionJobRecord) -> bool,
    ) -> u64 {
        self.indexes.scheduled_bucket_count_where(|id| {
            let job = self.jobs.get(&id).unwrap_or_else(|| {
                panic!("runtime invariant broken: production due index references missing job")
            });
            predicate(job)
        })
    }

    fn scheduled_completion_bucket_count_with_tick_where(
        &self,
        completes_at: SimulationTick,
        mut predicate: impl FnMut(&ProductionJobRecord) -> bool,
    ) -> u64 {
        self.indexes
            .scheduled_bucket_count_with_additional_tick_where(completes_at, |id| {
                let job = self.jobs.get(&id).unwrap_or_else(|| {
                    panic!("runtime invariant broken: production due index references missing job")
                });
                predicate(job)
            })
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
