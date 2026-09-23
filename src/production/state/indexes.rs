//! Derived production scheduling and exclusive-resource indexes.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::{ProductionJobId, ProductionJobRecord};

mod integrity;
mod mutation;
mod schedule;

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

    pub(super) fn rebuild<'a>(
        &mut self,
        jobs: impl Iterator<Item = (&'a ProductionJobId, &'a ProductionJobRecord)>,
    ) {
        *self = Self::new();
        for (id, job) in jobs {
            self.insert_rebuilt(*id, &ProductionJobIndexProjection::from_job(job));
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
}
