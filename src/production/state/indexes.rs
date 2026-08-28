//! Derived production scheduling and exclusive-resource indexes.

use std::collections::{BTreeMap, BTreeSet};

use crate::core::time::SimulationTick;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::{ProductionJobId, ProductionJobRecord};

type ProductionOccupancyMismatch<Resource> =
    (Resource, Option<ProductionJobId>, Option<ProductionJobId>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ProductionJobIndexProjection {
    due_tick: Option<SimulationTick>,
    energy_stores: Vec<EnergyStoreId>,
    equipment: Option<EquipmentId>,
    output_stockpiles: BTreeSet<StockpileId>,
}

impl ProductionJobIndexProjection {
    pub(super) fn from_job(job: &ProductionJobRecord) -> Self {
        Self {
            due_tick: (!job.is_suspended()).then_some(job.completes_at()),
            energy_stores: job
                .consumed_energy()
                .map(|trace| trace.source())
                .into_iter()
                .chain(job.released_energy().map(|trace| trace.destination()))
                .collect(),
            equipment: job
                .equipment_provider()
                .map(|provider| provider.equipment()),
            output_stockpiles: job
                .output_streams()
                .iter()
                .map(|stream| stream.destination())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProductionIndexes {
    pub(super) due_jobs: BTreeMap<SimulationTick, BTreeSet<ProductionJobId>>,
    energy_occupancy: BTreeMap<EnergyStoreId, ProductionJobId>,
    equipment_occupancy: BTreeMap<EquipmentId, ProductionJobId>,
    output_stockpile_occupancy: BTreeMap<StockpileId, BTreeSet<ProductionJobId>>,
}

impl ProductionIndexes {
    pub(super) const fn new() -> Self {
        Self {
            due_jobs: BTreeMap::new(),
            energy_occupancy: BTreeMap::new(),
            equipment_occupancy: BTreeMap::new(),
            output_stockpile_occupancy: BTreeMap::new(),
        }
    }

    pub(super) fn rebuild<'a>(&mut self, jobs: impl Iterator<Item = &'a ProductionJobRecord>) {
        *self = Self::new();
        for job in jobs {
            self.insert_rebuilt(job.id(), &ProductionJobIndexProjection::from_job(job));
        }
    }

    fn insert_rebuilt(&mut self, id: ProductionJobId, projection: &ProductionJobIndexProjection) {
        if let Some(due_tick) = projection.due_tick {
            self.due_jobs.entry(due_tick).or_default().insert(id);
        }
        for store in projection.energy_stores.iter().copied() {
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

    pub(super) fn earliest_due_tick(&self) -> Option<SimulationTick> {
        self.due_jobs.keys().next().copied()
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
        assert_eq!(
            projection.energy_stores.len(),
            projection
                .energy_stores
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len(),
            "validated production job cannot reserve one energy store more than once"
        );
        for store in &projection.energy_stores {
            assert!(
                !self.energy_occupancy.contains_key(store),
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

    pub(super) fn insert_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
        assert!(
            self.due_jobs.entry(due).or_default().insert(id),
            "runtime invariant broken: production due index already contains job {}",
            id.value()
        );
    }

    pub(super) fn remove_due_job(&mut self, id: ProductionJobId, due: SimulationTick) {
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
        if let Some(due_tick) = projection.due_tick {
            self.insert_due_job(id, due_tick);
        }
        for store in projection.energy_stores.iter().copied() {
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
        if let Some(due_tick) = projection.due_tick {
            self.remove_due_job(id, due_tick);
        }
        for store in &projection.energy_stores {
            assert_eq!(
                self.energy_occupancy.remove(store),
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

    fn expected_energy_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<BTreeMap<EnergyStoreId, ProductionJobId>, EnergyStoreId> {
        let mut occupied = BTreeMap::new();
        for job in jobs {
            for store in ProductionJobIndexProjection::from_job(job).energy_stores {
                if occupied.insert(store, job.id()).is_some() {
                    return Err(store);
                }
            }
        }
        Ok(occupied)
    }

    pub(super) fn energy_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<Option<ProductionOccupancyMismatch<EnergyStoreId>>, EnergyStoreId> {
        let expected = Self::expected_energy_occupancy(jobs)?;
        let stores = self
            .energy_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for store in stores {
            let indexed = self.energy_occupancy.get(&store).copied();
            let expected = expected.get(&store).copied();
            if indexed != expected {
                return Ok(Some((store, indexed, expected)));
            }
        }
        Ok(None)
    }

    fn expected_equipment_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<BTreeMap<EquipmentId, ProductionJobId>, EquipmentId> {
        let mut occupied = BTreeMap::new();
        for job in jobs {
            if let Some(equipment) = ProductionJobIndexProjection::from_job(job).equipment
                && occupied.insert(equipment, job.id()).is_some()
            {
                return Err(equipment);
            }
        }
        Ok(occupied)
    }

    pub(super) fn equipment_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<Option<ProductionOccupancyMismatch<EquipmentId>>, EquipmentId> {
        let expected = Self::expected_equipment_occupancy(jobs)?;
        let equipment_ids = self
            .equipment_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        for equipment in equipment_ids {
            let indexed = self.equipment_occupancy.get(&equipment).copied();
            let expected = expected.get(&equipment).copied();
            if indexed != expected {
                return Ok(Some((equipment, indexed, expected)));
            }
        }
        Ok(None)
    }

    fn expected_output_stockpile_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> BTreeMap<StockpileId, BTreeSet<ProductionJobId>> {
        let mut occupied = BTreeMap::<StockpileId, BTreeSet<ProductionJobId>>::new();
        for job in jobs {
            for stockpile in ProductionJobIndexProjection::from_job(job).output_stockpiles {
                occupied.entry(stockpile).or_default().insert(job.id());
            }
        }
        occupied
    }

    pub(super) fn output_stockpile_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Option<StockpileId> {
        let expected = Self::expected_output_stockpile_occupancy(jobs);
        let stockpiles = self
            .output_stockpile_occupancy
            .keys()
            .chain(expected.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        stockpiles.into_iter().find(|stockpile| {
            self.output_stockpile_occupancy.get(stockpile) != expected.get(stockpile)
        })
    }
}
