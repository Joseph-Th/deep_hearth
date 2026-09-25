//! Cross-index availability assertions and atomic production-index mutation.

use super::{ProductionIndexes, ProductionJobId, ProductionJobIndexProjection};

impl ProductionIndexes {
    pub(in crate::production::state) fn assert_job_available(
        &self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        if let (Some(consumed), Some(released)) = (
            projection.consumed_energy_store,
            projection.released_energy_store,
        ) {
            assert_ne!(
                consumed, released,
                "validated production job cannot reserve one energy store more than once"
            );
        }
        assert!(
            !self.player_labor_suspended_jobs.contains(&id),
            "validated production job cannot replace a player-labor suspension index entry"
        );
        assert!(
            !self.suspended_jobs.contains(&id),
            "validated production job cannot replace a suspension index entry"
        );
        assert!(
            !self.required_active_support_jobs.contains(&id),
            "validated production job cannot replace an active-support index entry"
        );
        for store in projection.energy_stores() {
            assert!(
                !self.energy_occupancy.contains_key(&store),
                "validated production job cannot replace an existing energy-store reservation"
            );
        }
        if let Some(equipment) = projection.equipment {
            assert!(
                !self.equipment_occupancy.contains_key(&equipment),
                "validated production job cannot replace an existing equipment reservation"
            );
        }
        self.assert_due_job_absent(id);
        for stockpile in &projection.output_stockpiles {
            assert!(
                !self
                    .output_stockpile_occupancy
                    .get(stockpile)
                    .is_some_and(|occupants| occupants.contains(&id)),
                "runtime invariant broken: production output-stockpile occupancy already contains job {}",
                id.value()
            );
        }
    }

    pub(in crate::production::state) fn assert_job_removable(
        &self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        if let Some(due_tick) = projection.due_tick {
            self.assert_due_job_present(id, due_tick);
        } else {
            self.assert_due_job_absent(id);
        }
        for store in projection.energy_stores() {
            assert_eq!(
                self.energy_occupancy.get(&store).copied(),
                Some(id),
                "runtime invariant broken: energy occupancy index disagrees with production job {}",
                id.value()
            );
        }
        if let Some(equipment) = projection.equipment {
            assert_eq!(
                self.equipment_occupancy.get(&equipment).copied(),
                Some(id),
                "runtime invariant broken: equipment occupancy index disagrees with production job {}",
                id.value()
            );
        }
        assert_eq!(
            self.player_labor_suspended_jobs.contains(&id),
            projection.player_labor_suspended,
            "runtime invariant broken: player-labor suspension index disagrees with production job {}",
            id.value()
        );
        assert_eq!(
            self.suspended_jobs.contains(&id),
            projection.suspended,
            "runtime invariant broken: suspension index disagrees with production job {}",
            id.value()
        );
        assert_eq!(
            self.required_active_support_jobs.contains(&id),
            projection.requires_active_support,
            "runtime invariant broken: active-support index disagrees with production job {}",
            id.value()
        );
        for stockpile in &projection.output_stockpiles {
            assert!(
                self.output_stockpile_occupancy
                    .get(stockpile)
                    .is_some_and(|occupants| occupants.contains(&id)),
                "runtime invariant broken: output-stockpile occupancy index disagrees with production job {}",
                id.value()
            );
        }
    }

    pub(in crate::production::state) fn insert_job(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_add(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| unreachable!("resident production output parcels fit u64"));
        if let Some(due_tick) = projection.due_tick {
            self.insert_due_job_with_requirements(
                id,
                due_tick,
                projection.requires_energy_revision,
                projection.requires_equipment_revision,
            );
        }
        for store in projection.energy_stores() {
            assert!(
                self.energy_occupancy.insert(store, id).is_none(),
                "runtime invariant broken: production energy occupancy replaced an existing job"
            );
        }
        if let Some(equipment) = projection.equipment {
            assert!(
                self.equipment_occupancy.insert(equipment, id).is_none(),
                "runtime invariant broken: production equipment occupancy replaced an existing job"
            );
        }
        if projection.suspended {
            assert!(
                self.suspended_jobs.insert(id),
                "runtime invariant broken: suspension index already contains job {}",
                id.value()
            );
        }
        if projection.player_labor_suspended {
            assert!(
                self.player_labor_suspended_jobs.insert(id),
                "runtime invariant broken: player-labor suspension index already contains job {}",
                id.value()
            );
        }
        if projection.requires_active_support {
            assert!(
                self.required_active_support_jobs.insert(id),
                "runtime invariant broken: active-support index already contains job {}",
                id.value()
            );
        }
        for stockpile in &projection.output_stockpiles {
            assert!(
                self.output_stockpile_occupancy
                    .entry(*stockpile)
                    .or_default()
                    .insert(id),
                "runtime invariant broken: production output-stockpile occupancy already contains job {}",
                id.value()
            );
        }
    }

    pub(in crate::production::state) fn remove_job(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_sub(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| panic!("production future lot-id demand underflowed"));
        if let Some(due_tick) = projection.due_tick {
            self.remove_due_job_with_requirements(
                id,
                due_tick,
                projection.requires_energy_revision,
                projection.requires_equipment_revision,
            );
        }
        for store in projection.energy_stores() {
            assert_eq!(
                self.energy_occupancy.remove(&store),
                Some(id),
                "runtime invariant broken: energy occupancy index disagrees with production job {}",
                id.value()
            );
        }
        if let Some(equipment) = projection.equipment {
            assert_eq!(
                self.equipment_occupancy.remove(&equipment),
                Some(id),
                "runtime invariant broken: equipment occupancy index disagrees with production job {}",
                id.value()
            );
        }
        if projection.suspended {
            assert!(
                self.suspended_jobs.remove(&id),
                "runtime invariant broken: suspension index is missing production job {}",
                id.value()
            );
        }
        if projection.player_labor_suspended {
            assert!(
                self.player_labor_suspended_jobs.remove(&id),
                "runtime invariant broken: player-labor suspension index is missing production job {}",
                id.value()
            );
        }
        if projection.requires_active_support {
            assert!(
                self.required_active_support_jobs.remove(&id),
                "runtime invariant broken: active-support index is missing production job {}",
                id.value()
            );
        }
        for stockpile in &projection.output_stockpiles {
            let remove_bucket = {
                let occupants = self
                    .output_stockpile_occupancy
                    .get_mut(stockpile)
                    .unwrap_or_else(|| {
                        panic!(
                            "runtime invariant broken: output-stockpile occupancy index missing production job {}",
                            id.value()
                        )
                    });
                assert!(
                    occupants.remove(&id),
                    "runtime invariant broken: output-stockpile occupancy index disagrees with production job {}",
                    id.value()
                );
                occupants.is_empty()
            };
            if remove_bucket {
                self.output_stockpile_occupancy.remove(stockpile);
            }
        }
    }

    pub(in crate::production::state) fn set_player_labor_suspended(
        &mut self,
        id: ProductionJobId,
        suspended: bool,
    ) {
        if suspended {
            assert!(
                self.player_labor_suspended_jobs.insert(id),
                "runtime invariant broken: player-labor suspension index already contains job {}",
                id.value()
            );
        } else {
            assert!(
                self.player_labor_suspended_jobs.remove(&id),
                "runtime invariant broken: player-labor suspension index is missing job {}",
                id.value()
            );
        }
    }

    pub(in crate::production::state) fn set_suspended(
        &mut self,
        id: ProductionJobId,
        suspended: bool,
    ) {
        if suspended {
            assert!(
                self.suspended_jobs.insert(id),
                "runtime invariant broken: suspension index already contains job {}",
                id.value()
            );
        } else {
            assert!(
                self.suspended_jobs.remove(&id),
                "runtime invariant broken: suspension index is missing job {}",
                id.value()
            );
        }
    }
}
