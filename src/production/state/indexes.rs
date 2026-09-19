//! Derived production scheduling and exclusive-resource indexes.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::{ProductionJobId, ProductionJobRecord};

mod integrity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProductionJobIndexProjection {
    due_tick: Option<SimulationTick>,
    consumed_energy_store: Option<EnergyStoreId>,
    released_energy_store: Option<EnergyStoreId>,
    equipment: Option<EquipmentId>,
    output_stockpiles: BTreeSet<StockpileId>,
    future_material_lot_id_demand: u64,
}

impl ProductionJobIndexProjection {
    pub(super) fn from_job(job: &ProductionJobRecord) -> Self {
        Self {
            due_tick: (!job.is_suspended()).then_some(job.completes_at()),
            consumed_energy_store: job.consumed_energy().map(|trace| trace.source()),
            released_energy_store: job.released_energy().map(|trace| trace.destination()),
            equipment: job
                .equipment_provider()
                .map(|provider| provider.equipment()),
            output_stockpiles: job
                .output_streams()
                .iter()
                .map(|stream| stream.destination())
                .collect(),
            future_material_lot_id_demand: job.future_material_lot_id_demand_upper_bound(),
        }
    }

    fn energy_stores(&self) -> impl Iterator<Item = EnergyStoreId> {
        [self.consumed_energy_store, self.released_energy_store]
            .into_iter()
            .flatten()
    }
}

#[cfg(test)]
#[path = "indexes_tests.rs"]
mod tests;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProductionIndexes {
    pub(super) due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
    energy_occupancy: BTreeMap<EnergyStoreId, ProductionJobId>,
    equipment_occupancy: BTreeMap<EquipmentId, ProductionJobId>,
    output_stockpile_occupancy: BTreeMap<StockpileId, BTreeSet<ProductionJobId>>,
    future_material_lot_id_demand: u64,
}

impl ProductionIndexes {
    pub(super) const fn new() -> Self {
        Self {
            due_jobs: BTreeMap::new(),
            energy_occupancy: BTreeMap::new(),
            equipment_occupancy: BTreeMap::new(),
            output_stockpile_occupancy: BTreeMap::new(),
            future_material_lot_id_demand: 0,
        }
    }

    pub(super) fn rebuild<'a>(&mut self, jobs: impl Iterator<Item = &'a ProductionJobRecord>) {
        *self = Self::new();
        for job in jobs {
            self.insert_rebuilt(job.id(), &ProductionJobIndexProjection::from_job(job));
        }
    }

    fn insert_rebuilt(&mut self, id: ProductionJobId, projection: &ProductionJobIndexProjection) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_add(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| unreachable!("resident production output parcels fit u64"));
        if let Some(due_tick) = projection.due_tick {
            self.due_jobs.entry(due_tick).or_default().insert(id);
        }
        for store in projection.energy_stores() {
            self.energy_occupancy.entry(store).or_insert(id);
        }
        if let Some(equipment) = projection.equipment {
            self.equipment_occupancy.entry(equipment).or_insert(id);
        }
        for stockpile in &projection.output_stockpiles {
            self.output_stockpile_occupancy
                .entry(*stockpile)
                .or_default()
                .insert(id);
        }
    }

    pub(super) const fn future_material_lot_id_demand(&self) -> u64 {
        self.future_material_lot_id_demand
    }

    pub(super) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
    }

    pub(super) fn scheduled_bucket_count(&self) -> u64 {
        u64::try_from(self.due_jobs.len())
            .unwrap_or_else(|_| unreachable!("production due-bucket count fits memory"))
    }

    pub(super) fn scheduled_bucket_count_where(
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

    pub(super) fn scheduled_bucket_count_with_additional_tick_where(
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

    pub(super) fn jobs_due_at(&self, tick: SimulationTick) -> BTreeSet<ProductionJobId> {
        self.due_jobs.get(&tick).cloned().unwrap_or_default()
    }

    pub(super) fn energy_occupant(&self, store: EnergyStoreId) -> Option<ProductionJobId> {
        self.energy_occupancy.get(&store).copied()
    }

    pub(super) fn equipment_occupant(&self, equipment: EquipmentId) -> Option<&ProductionJobId> {
        self.equipment_occupancy.get(&equipment)
    }

    pub(super) fn output_stockpile_occupants(
        &self,
        stockpile: StockpileId,
    ) -> Option<&BTreeSet<ProductionJobId>> {
        self.output_stockpile_occupancy.get(&stockpile)
    }

    pub(super) fn assert_job_available(
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
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
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

    pub(super) fn assert_due_job_present(&self, id: ProductionJobId, due: SimulationTick) {
        assert!(
            self.due_jobs
                .get(&due)
                .is_some_and(|jobs| jobs.contains(&id)),
            "runtime invariant broken: production due index is missing job {}",
            id.value()
        );
    }

    pub(super) fn assert_due_job_absent(&self, id: ProductionJobId) {
        assert!(
            self.due_jobs.values().all(|jobs| !jobs.contains(&id)),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
    }

    pub(super) fn assert_job_removable(
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

    pub(super) fn insert_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
        self.assert_due_job_absent(id);
        assert!(
            self.due_jobs.entry(due).or_default().insert(id),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
    }

    pub(super) fn remove_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
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

    pub(super) fn insert_job(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_add(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| unreachable!("resident production output parcels fit u64"));
        if let Some(due_tick) = projection.due_tick {
            self.insert_due_job(id, due_tick);
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

    pub(super) fn remove_job(
        &mut self,
        id: ProductionJobId,
        projection: &ProductionJobIndexProjection,
    ) {
        self.future_material_lot_id_demand = self
            .future_material_lot_id_demand
            .checked_sub(projection.future_material_lot_id_demand)
            .unwrap_or_else(|| panic!("production future lot-id demand underflowed"));
        if let Some(due_tick) = projection.due_tick {
            self.remove_due_job(id, due_tick);
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
}
