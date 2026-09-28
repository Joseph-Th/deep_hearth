//! Future revision and material-identity headroom reserved by durable production schedules.

use crate::core::time::SimulationTick;
use crate::inventory::InventoryState;

use super::{ProductionJobRecord, ProductionState};

impl ProductionState {
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
}
