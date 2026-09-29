//! Shared future owner-mutation and material-identity headroom accounting.

use crate::inventory::StockpileId;

use super::AppState;

fn checked_combined_demand(primary: u64, secondary: u64) -> Option<u64> {
    primary.checked_add(secondary)
}

fn checked_demand_after_release(future: u64, released: u64) -> Option<u64> {
    future.checked_sub(released)
}

fn checked_demand_after_adjustment(future: u64, additional: u64, released: u64) -> Option<u64> {
    checked_demand_after_release(future, released)?.checked_add(additional)
}

impl AppState {
    pub(crate) fn checked_future_inventory_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems.production.scheduled_completion_bucket_count(),
            self.future_nonproduction_inventory_revision_demand(),
        )
    }

    pub(crate) fn future_nonproduction_inventory_revision_demand(&self) -> u64 {
        self.systems
            .player_work
            .future_inventory_revision_demand()
            .checked_add(self.systems.mining.future_claim_count())
            .unwrap_or_else(|| {
                unreachable!("resident nonproduction inventory revision demand fits u64")
            })
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

    pub(crate) fn future_nonproduction_structure_revision_demand(&self) -> u64 {
        // Only supported mining destinations require a structure-owned load mutation when their
        // retained output is claimed. Support assignment changes reproject this demand atomically.
        self.systems
            .mining
            .future_supported_claim_count(&self.systems.inventory)
    }

    pub(crate) fn retained_mining_claim_count_for_stockpile(&self, stockpile: StockpileId) -> u64 {
        self.systems
            .mining
            .future_claim_count_for_destination(stockpile)
    }

    pub(crate) fn checked_future_structure_revision_demand(&self) -> Option<u64> {
        checked_combined_demand(
            self.systems
                .production
                .scheduled_supported_output_revision_bucket_count(&self.systems.inventory),
            self.future_nonproduction_structure_revision_demand(),
        )
    }

    pub(crate) fn checked_future_mining_revision_demand(&self) -> Option<u64> {
        self.systems.mining.checked_future_revision_demand()
    }

    pub(crate) fn checked_future_material_lot_id_demand(&self) -> Option<u64> {
        let production_and_player = checked_combined_demand(
            self.systems.production.future_material_lot_id_demand(),
            self.systems
                .player_work
                .future_material_lot_id_demand(&self.systems.inventory),
        )?;
        checked_combined_demand(
            production_and_player,
            self.systems.mining.future_claim_count(),
        )
    }

    pub(crate) fn has_material_lot_id_headroom_from(
        &self,
        next_lot_id: u64,
        additional_future_demand: u64,
    ) -> bool {
        self.has_material_lot_id_headroom_from_after_releasing(
            next_lot_id,
            additional_future_demand,
            0,
        )
    }

    pub(crate) fn has_material_lot_id_headroom_from_after_releasing(
        &self,
        next_lot_id: u64,
        additional_future_demand: u64,
        released_future_demand: u64,
    ) -> bool {
        self.checked_future_material_lot_id_demand()
            .and_then(|future| checked_demand_after_release(future, released_future_demand))
            .and_then(|future| next_lot_id.checked_add(future))
            .and_then(|cursor| cursor.checked_add(additional_future_demand))
            .is_some()
    }

    pub(crate) fn can_spend_inventory_revisions(&self, immediate_steps: u64) -> bool {
        self.can_spend_inventory_revisions_after_releasing(immediate_steps, 0)
    }

    pub(crate) fn can_spend_inventory_revisions_after_releasing(
        &self,
        immediate_steps: u64,
        released_future_demand: u64,
    ) -> bool {
        self.checked_future_inventory_revision_demand()
            .and_then(|future| checked_demand_after_release(future, released_future_demand))
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
        self.can_spend_structure_revisions_after_releasing(immediate_steps, 0)
    }

    pub(crate) fn can_spend_structure_revisions_after_releasing(
        &self,
        immediate_steps: u64,
        released_future_demand: u64,
    ) -> bool {
        self.can_spend_structure_revisions_after_adjusting(
            immediate_steps,
            0,
            released_future_demand,
        )
    }

    pub(crate) fn can_spend_structure_revisions_after_adjusting(
        &self,
        immediate_steps: u64,
        additional_future_demand: u64,
        released_future_demand: u64,
    ) -> bool {
        self.checked_future_structure_revision_demand()
            .and_then(|future| {
                checked_demand_after_adjustment(
                    future,
                    additional_future_demand,
                    released_future_demand,
                )
            })
            .and_then(|future| self.systems.structures.revision().checked_add(future))
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }

    pub(crate) fn can_spend_mining_revisions(&self, immediate_steps: u64) -> bool {
        self.can_spend_mining_revisions_after_releasing(immediate_steps, 0)
    }

    pub(crate) fn can_spend_mining_revisions_after_releasing(
        &self,
        immediate_steps: u64,
        released_future_demand: u64,
    ) -> bool {
        self.checked_future_mining_revision_demand()
            .and_then(|future| checked_demand_after_release(future, released_future_demand))
            .and_then(|future| self.systems.mining.revision().checked_add(future))
            .and_then(|revision| revision.checked_add(immediate_steps))
            .is_some()
    }
}

#[cfg(test)]
#[path = "headroom_tests.rs"]
mod tests;
