//! Trusted-load integrity reconstruction for derived production occupancy indexes.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::inventory::StockpileId;

use super::ProductionIndexes;
use crate::production::{ProductionJobId, ProductionJobRecord};

type ProductionOccupancyMismatch<Resource> =
    (Resource, Option<ProductionJobId>, Option<ProductionJobId>);

fn first_map_key_mismatch<Key, Value>(
    indexed: &BTreeMap<Key, Value>,
    expected: &BTreeMap<Key, Value>,
) -> Option<Key>
where
    Key: Copy + Ord,
    Value: PartialEq,
{
    let mut indexed_entries = indexed.iter().peekable();
    let mut expected_entries = expected.iter().peekable();
    loop {
        match (
            indexed_entries.peek().copied(),
            expected_entries.peek().copied(),
        ) {
            (Some((&indexed_key, indexed_value)), Some((&expected_key, expected_value))) => {
                match indexed_key.cmp(&expected_key) {
                    Ordering::Less => return Some(indexed_key),
                    Ordering::Greater => return Some(expected_key),
                    Ordering::Equal => {
                        let _ = indexed_entries.next();
                        let _ = expected_entries.next();
                        if indexed_value != expected_value {
                            return Some(indexed_key);
                        }
                    }
                }
            }
            (Some((&indexed_key, _)), None) => return Some(indexed_key),
            (None, Some((&expected_key, _))) => return Some(expected_key),
            (None, None) => return None,
        }
    }
}

impl ProductionIndexes {
    pub(in crate::production::state) fn future_material_lot_id_demand_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Option<(u64, u64)> {
        let expected = jobs
            .map(ProductionJobRecord::future_material_lot_id_demand_upper_bound)
            .try_fold(0_u64, u64::checked_add)
            .unwrap_or_else(|| unreachable!("resident production output parcels fit u64"));
        (self.future_material_lot_id_demand != expected)
            .then_some((self.future_material_lot_id_demand, expected))
    }

    fn expected_energy_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<BTreeMap<EnergyStoreId, ProductionJobId>, EnergyStoreId> {
        let mut occupied = BTreeMap::new();
        for job in jobs {
            for store in job
                .consumed_energy()
                .map(|trace| trace.source())
                .into_iter()
                .chain(job.released_energy().map(|trace| trace.destination()))
            {
                if occupied.insert(store, job.id()).is_some() {
                    return Err(store);
                }
            }
        }
        Ok(occupied)
    }

    pub(in crate::production::state) fn energy_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<Option<ProductionOccupancyMismatch<EnergyStoreId>>, EnergyStoreId> {
        let expected = Self::expected_energy_occupancy(jobs)?;
        Ok(
            first_map_key_mismatch(&self.energy_occupancy, &expected).map(|store| {
                (
                    store,
                    self.energy_occupancy.get(&store).copied(),
                    expected.get(&store).copied(),
                )
            }),
        )
    }

    fn expected_equipment_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<BTreeMap<EquipmentId, ProductionJobId>, EquipmentId> {
        let mut occupied = BTreeMap::new();
        for job in jobs {
            if let Some(equipment) = job
                .equipment_provider()
                .map(|provider| provider.equipment())
                && occupied.insert(equipment, job.id()).is_some()
            {
                return Err(equipment);
            }
        }
        Ok(occupied)
    }

    pub(in crate::production::state) fn equipment_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Result<Option<ProductionOccupancyMismatch<EquipmentId>>, EquipmentId> {
        let expected = Self::expected_equipment_occupancy(jobs)?;
        Ok(
            first_map_key_mismatch(&self.equipment_occupancy, &expected).map(|equipment| {
                (
                    equipment,
                    self.equipment_occupancy.get(&equipment).copied(),
                    expected.get(&equipment).copied(),
                )
            }),
        )
    }

    fn expected_output_stockpile_occupancy<'a>(
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> BTreeMap<StockpileId, BTreeSet<ProductionJobId>> {
        let mut occupied = BTreeMap::<StockpileId, BTreeSet<ProductionJobId>>::new();
        for job in jobs {
            for stream in job.output_streams() {
                occupied
                    .entry(stream.destination())
                    .or_default()
                    .insert(job.id());
            }
        }
        occupied
    }

    pub(in crate::production::state) fn output_stockpile_occupancy_mismatch<'a>(
        &self,
        jobs: impl Iterator<Item = &'a ProductionJobRecord>,
    ) -> Option<StockpileId> {
        let expected = Self::expected_output_stockpile_occupancy(jobs);
        first_map_key_mismatch(&self.output_stockpile_occupancy, &expected)
    }
}
