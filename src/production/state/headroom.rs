//! Future revision and material-identity headroom reserved by durable production schedules.

use crate::inventory::InventoryState;

use super::ProductionState;

impl ProductionState {
    /// Conservative cross-owner inventory revisions still owed by durable production jobs.
    ///
    /// Completion execution may batch jobs that happen to share a due tick, but suspension can
    /// later separate those schedules. Headroom therefore reserves one eventual inventory
    /// mutation per retained job instead of relying on present due-bucket coalescing.
    pub(crate) fn future_inventory_revision_demand(&self) -> u64 {
        u64::try_from(self.jobs.len())
            .unwrap_or_else(|_| unreachable!("resident production job count fits u64"))
    }

    pub(crate) fn future_completion_revision_demand(&self) -> u64 {
        u64::try_from(self.jobs.len())
            .unwrap_or_else(|_| unreachable!("resident production completion demand fits u64"))
    }

    pub(crate) fn has_future_completion_revision_capacity_from(
        &self,
        revision: u64,
        additional_jobs: u64,
    ) -> bool {
        revision
            .checked_add(self.future_completion_revision_demand())
            .and_then(|revision| revision.checked_add(additional_jobs))
            .is_some()
    }

    pub(crate) fn future_equipment_revision_demand(&self) -> u64 {
        u64::try_from(
            self.jobs
                .values()
                .filter(|job| job.requires_equipment_revision_at_completion())
                .count(),
        )
        .unwrap_or_else(|_| unreachable!("resident production equipment revision demand fits u64"))
    }

    pub(crate) fn future_released_energy_revision_demand(&self) -> u64 {
        u64::try_from(
            self.jobs
                .values()
                .filter(|job| job.requires_energy_revision_at_completion())
                .count(),
        )
        .unwrap_or_else(|_| unreachable!("resident production energy revision demand fits u64"))
    }

    pub(crate) fn future_supported_output_revision_demand(
        &self,
        inventory: &InventoryState,
    ) -> u64 {
        u64::try_from(
            self.jobs
                .values()
                .filter(|job| job.requires_structure_revision_at_completion(inventory))
                .count(),
        )
        .unwrap_or_else(|_| unreachable!("resident production structure revision demand fits u64"))
    }

    pub(crate) fn has_future_inventory_revision_capacity_from(
        &self,
        revision: u64,
        additional_jobs: u64,
    ) -> bool {
        revision
            .checked_add(self.future_inventory_revision_demand())
            .and_then(|revision| revision.checked_add(additional_jobs))
            .is_some()
    }

    pub(crate) fn has_future_equipment_revision_capacity_from(
        &self,
        revision: u64,
        additional_jobs: u64,
    ) -> bool {
        revision
            .checked_add(self.future_equipment_revision_demand())
            .and_then(|revision| revision.checked_add(additional_jobs))
            .is_some()
    }

    pub(crate) fn has_future_released_energy_revision_capacity_from(
        &self,
        revision: u64,
        additional_jobs: u64,
    ) -> bool {
        revision
            .checked_add(self.future_released_energy_revision_demand())
            .and_then(|revision| revision.checked_add(additional_jobs))
            .is_some()
    }

    pub(crate) fn has_future_supported_output_revision_capacity_from(
        &self,
        revision: u64,
        inventory: &InventoryState,
        additional_jobs: u64,
    ) -> bool {
        revision
            .checked_add(self.future_supported_output_revision_demand(inventory))
            .and_then(|revision| revision.checked_add(additional_jobs))
            .is_some()
    }

    /// Future structural-demand change if one stockpile's support status is replaced.
    pub(crate) fn future_supported_output_revision_adjustment_for_stockpile(
        &self,
        inventory: &InventoryState,
        stockpile: crate::inventory::StockpileId,
        supported_after: bool,
    ) -> (u64, u64) {
        let mut additional = 0_u64;
        let mut released = 0_u64;
        for job in self.jobs.values() {
            let before = job.requires_structure_revision_at_completion(inventory);
            let after = job.output_streams().iter().any(|stream| {
                if stream.destination() == stockpile {
                    supported_after
                } else {
                    inventory
                        .get_stockpile(stream.destination())
                        .unwrap_or_else(|| {
                            panic!(
                                "runtime invariant broken: production job {} output references missing stockpile {}",
                                job.id().value(),
                                stream.destination().value()
                            )
                        })
                        .supported_by()
                        .is_some()
                }
            });
            match (before, after) {
                (false, true) => {
                    additional = additional.checked_add(1).unwrap_or_else(|| {
                        unreachable!("resident production structural demand fits u64")
                    })
                }
                (true, false) => {
                    released = released.checked_add(1).unwrap_or_else(|| {
                        unreachable!("resident production structural demand fits u64")
                    })
                }
                _ => {}
            }
        }
        (additional, released)
    }

    pub(crate) const fn future_material_lot_id_demand(&self) -> u64 {
        self.indexes.future_material_lot_id_demand()
    }
}
