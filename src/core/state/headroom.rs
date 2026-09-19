//! Shared future owner-mutation and material-identity headroom accounting.

use super::AppState;

fn checked_combined_demand(primary: u64, secondary: u64) -> Option<u64> {
    primary.checked_add(secondary)
}

impl AppState {
    pub(crate) fn checked_future_inventory_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems.production.scheduled_completion_bucket_count(),
            self.future_nonproduction_inventory_revision_demand(),
        )
    }

    pub(crate) fn future_nonproduction_inventory_revision_demand(&self) -> u64 {
        self.systems.player_work.future_inventory_revision_demand()
    }

    pub(crate) fn checked_future_energy_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems
                .production
                .scheduled_released_energy_revision_bucket_count(),
            self.future_nonproduction_energy_revision_demand(),
        )
    }

    pub(crate) fn future_nonproduction_energy_revision_demand(&self) -> u64 {
        self.systems.player_work.future_energy_revision_demand()
    }

    pub(crate) fn checked_future_equipment_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems
                .production
                .scheduled_equipment_revision_bucket_count(),
            self.checked_future_nonproduction_equipment_revision_demand()?,
        )
    }

    pub(crate) fn checked_future_nonproduction_equipment_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems
                .mining
                .scheduled_equipment_revision_bucket_count(),
            self.systems.player_work.future_equipment_revision_demand(),
        )
    }

    pub(crate) fn future_structure_revision_demand(&self) -> u64 {
        self.systems
            .production
            .scheduled_supported_output_revision_bucket_count(&self.systems.inventory)
    }

    pub(crate) fn checked_future_material_lot_id_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems.production.future_material_lot_id_demand(),
            self.systems
                .player_work
                .future_material_lot_id_demand(&self.systems.inventory),
        )
    }

    pub(crate) fn has_material_lot_id_headroom_from(
        &self,
        next_lot_id: u64,
        additional_future_demand: u64,
    ) -> bool {
        self.checked_future_material_lot_id_demand()
            .and_then(|future| next_lot_id.checked_add(future))
            .and_then(|cursor| cursor.checked_add(additional_future_demand))
            .is_some()
    }

    pub(crate) fn can_spend_inventory_revisions(&self, immediate_steps: u64) -> bool {
        self.checked_future_inventory_revision_demand()
            .and_then(|future| self.systems.inventory.revision().checked_add(future))
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }

    pub(crate) fn can_spend_energy_revisions(&self, immediate_steps: u64) -> bool {
        self.checked_future_energy_revision_demand()
            .and_then(|future| self.systems.energy.revision().checked_add(future))
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }

    pub(crate) fn can_spend_equipment_revisions(&self, immediate_steps: u64) -> bool {
        self.checked_future_equipment_revision_demand()
            .and_then(|future| self.systems.equipment.revision().checked_add(future))
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }

    pub(crate) fn can_spend_structure_revisions(&self, immediate_steps: u64) -> bool {
        self.systems
            .structures
            .revision()
            .checked_add(self.future_structure_revision_demand())
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }
}

#[cfg(test)]
#[path = "headroom_tests.rs"]
mod tests;
