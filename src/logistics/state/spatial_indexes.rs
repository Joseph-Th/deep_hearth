//! Derived local position indexes. Authoritative custody stays in LogisticsState.

use std::collections::{BTreeMap, BTreeSet};

use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

use super::LogisticsState;

impl LogisticsState {
    /// Stationary stockpile identities at one exact voxel, sorted by stable
    /// stockpile ID. The carried pack is not stationary ground custody.
    pub(crate) fn stockpiles_at(
        &self,
        position: VoxelCoord,
    ) -> impl Iterator<Item = StockpileId> + '_ {
        self.stockpiles_by_position
            .get(&position)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    /// Only stores inside the maximum mathematically possible finite-light range.
    /// Source order remains stable by ID for deterministic equal-brightness ties.
    pub(crate) fn energy_store_locations_near(
        &self,
        position: VoxelCoord,
        radius: u8,
    ) -> Vec<(EnergyStoreId, VoxelCoord)> {
        let reach = i64::from(radius);
        let low = VoxelCoord::new(position.x().saturating_sub(reach), i64::MIN, i64::MIN);
        let high = VoxelCoord::new(position.x().saturating_add(reach), i64::MAX, i64::MAX);
        let mut nearby = self
            .energy_stores_by_position
            .range(low..=high)
            .filter(|(at, _)| {
                at.y().abs_diff(position.y()) <= u64::from(radius)
                    && at.z().abs_diff(position.z()) <= u64::from(radius)
            })
            .flat_map(|(at, stores)| stores.iter().map(move |store| (*store, *at)))
            .collect::<Vec<_>>();
        nearby.sort_unstable_by_key(|(store, _)| *store);
        nearby
    }

    pub(crate) fn rebuild_derived_indexes(&mut self) {
        self.traversal_neighbors.clear();
        for edge in &self.traversable_edges {
            Self::index_traversal_edge(&mut self.traversal_neighbors, *edge);
        }
        self.stockpiles_by_position.clear();
        for (stockpile, position) in &self.stockpile_locations {
            self.stockpiles_by_position
                .entry(*position)
                .or_default()
                .insert(*stockpile);
        }
        self.equipment_by_position.clear();
        for (equipment, position) in &self.equipment_locations {
            self.equipment_by_position
                .entry(*position)
                .or_default()
                .insert(*equipment);
        }
        self.energy_stores_by_position.clear();
        for (store, position) in &self.energy_store_locations {
            self.energy_stores_by_position
                .entry(*position)
                .or_default()
                .insert(*store);
        }
        self.fluid_stores_by_position.clear();
        for (store, position) in &self.fluid_store_locations {
            self.fluid_stores_by_position
                .entry(*position)
                .or_default()
                .insert(*store);
        }
    }

    pub(crate) fn has_valid_stockpile_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<StockpileId>>::new();
        for (stockpile, position) in &self.stockpile_locations {
            expected.entry(*position).or_default().insert(*stockpile);
        }
        self.stockpiles_by_position == expected
    }

    pub(crate) fn has_valid_fluid_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<FluidStoreId>>::new();
        for (store, position) in &self.fluid_store_locations {
            expected.entry(*position).or_default().insert(*store);
        }
        self.fluid_stores_by_position == expected
    }

    pub(crate) fn has_valid_energy_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<EnergyStoreId>>::new();
        for (store, position) in &self.energy_store_locations {
            expected.entry(*position).or_default().insert(*store);
        }
        self.energy_stores_by_position == expected
    }

    pub(crate) fn has_valid_equipment_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<EquipmentId>>::new();
        for (equipment, position) in &self.equipment_locations {
            expected.entry(*position).or_default().insert(*equipment);
        }
        self.equipment_by_position == expected
    }

    /// Bounded spatial observation of stationary equipment, stable by equipment identity.
    pub(crate) fn equipment_locations_near(
        &self,
        position: VoxelCoord,
        radius: u8,
    ) -> Vec<(EquipmentId, VoxelCoord)> {
        let reach = i64::from(radius);
        let low = VoxelCoord::new(position.x().saturating_sub(reach), i64::MIN, i64::MIN);
        let high = VoxelCoord::new(position.x().saturating_add(reach), i64::MAX, i64::MAX);
        let mut nearby = self
            .equipment_by_position
            .range(low..=high)
            .filter(|(at, _)| {
                at.y().abs_diff(position.y()) <= u64::from(radius)
                    && at.z().abs_diff(position.z()) <= u64::from(radius)
            })
            .flat_map(|(at, equipment)| equipment.iter().map(move |id| (*id, *at)))
            .collect::<Vec<_>>();
        nearby.sort_unstable_by_key(|(equipment, _)| *equipment);
        nearby
    }

    #[must_use]
    pub(crate) fn has_stationary_occupant_at(&self, position: VoxelCoord) -> bool {
        self.stockpiles_by_position.contains_key(&position)
            || self.equipment_by_position.contains_key(&position)
            || self.energy_stores_by_position.contains_key(&position)
            || self.fluid_stores_by_position.contains_key(&position)
    }
}
