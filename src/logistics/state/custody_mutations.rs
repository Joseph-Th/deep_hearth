//! Atomic logistics custody mutations for ground stores, equipment, energy and water.

use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

use super::super::traversal::TraversalEdge;
use super::{LogisticsState, PlayerActiveSelection, PlayerQuickAccessSlot};

impl LogisticsState {
    pub(in crate::logistics) fn apply_ground_placement(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        stockpile: StockpileId,
        position: VoxelCoord,
    ) {
        assert_eq!(
            self.revision, expected_revision,
            "ground stockpile placement requires its validated logistics revision"
        );
        assert_eq!(
            expected_revision.checked_add(1),
            Some(next_revision),
            "ground stockpile placement must advance logistics revision exactly once"
        );
        assert!(
            !self.stockpile_locations.contains_key(&stockpile),
            "stockpile placement cannot replace an existing location"
        );
        assert!(
            self.player
                .is_none_or(|player| player.carried_stockpile() != stockpile),
            "player-carried custody cannot also be placed on the ground"
        );
        let previous = self.stockpile_locations.insert(stockpile, position);
        assert!(previous.is_none());
        assert!(
            self.stockpiles_by_position
                .entry(position)
                .or_default()
                .insert(stockpile)
        );
        self.revision = next_revision;
    }

    pub(crate) fn apply_equipment_placement(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        equipment: EquipmentId,
        position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !self.equipment_locations.contains_key(&equipment),
            "equipment placement cannot replace an existing location"
        );
        let previous = self.equipment_locations.insert(equipment, position);
        assert!(previous.is_none());
        assert!(
            self.equipment_by_position
                .entry(position)
                .or_default()
                .insert(equipment)
        );
        self.revision = next_revision;
    }

    fn remove_stationary_equipment_index(&mut self, equipment: EquipmentId, position: VoxelCoord) {
        let at_position = self
            .equipment_by_position
            .get_mut(&position)
            .expect("located equipment must appear in its spatial index");
        assert!(at_position.remove(&equipment));
        if at_position.is_empty() {
            self.equipment_by_position.remove(&position);
        }
    }

    pub(crate) fn apply_equipment_location_removal(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        equipment: EquipmentId,
        expected_position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(
            self.equipment_locations.remove(&equipment),
            Some(expected_position),
            "equipment location changed after validation"
        );
        self.remove_stationary_equipment_index(equipment, expected_position);
        self.revision = next_revision;
    }

    pub(in crate::logistics) fn apply_equipment_pickup(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        equipment: EquipmentId,
        expected_position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.player.is_some(),
            "equipment pickup requires a logistics player"
        );
        assert_eq!(
            self.equipment_locations.remove(&equipment),
            Some(expected_position),
            "equipment location changed after pickup validation"
        );
        self.remove_stationary_equipment_index(equipment, expected_position);
        let insertion = match self.carried_equipment.binary_search(&equipment) {
            Ok(_) => panic!("validated equipment pickup cannot duplicate carried custody"),
            Err(index) => index,
        };
        self.carried_equipment.insert(insertion, equipment);
        self.revision = next_revision;
    }

    pub(in crate::logistics) fn apply_quick_access_binding(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        slot: PlayerQuickAccessSlot,
        selection: Option<PlayerActiveSelection>,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.player.is_some(),
            "active selection requires a logistics player"
        );
        assert!(slot.is_valid(), "quick-access slot must be in range");
        assert!(
            selection.is_none_or(|selection| match selection {
                PlayerActiveSelection::Equipment(equipment) => self.is_equipment_carried(equipment),
                PlayerActiveSelection::Material(_) => true,
            }),
            "active selection must reference canonical carried custody"
        );
        if let Some(selection) = selection {
            for existing in &mut self.quick_access_slots {
                if *existing == Some(selection) {
                    *existing = None;
                }
            }
        }
        self.quick_access_slots[slot.index()] = selection;
        self.revision = next_revision;
    }

    pub(in crate::logistics) fn apply_active_quick_access_slot(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        slot: PlayerQuickAccessSlot,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.player.is_some(),
            "quick-access selection requires a logistics player"
        );
        assert!(slot.is_valid(), "quick-access slot must be in range");
        self.active_quick_access_slot = slot;
        self.revision = next_revision;
    }

    pub(in crate::logistics) fn apply_equipment_drop(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        equipment: EquipmentId,
        position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let index = match self.carried_equipment.binary_search(&equipment) {
            Ok(index) => index,
            Err(_) => panic!("validated equipment drop requires carried custody"),
        };
        self.carried_equipment.remove(index);
        for selection in &mut self.quick_access_slots {
            if *selection == Some(PlayerActiveSelection::Equipment(equipment)) {
                *selection = None;
            }
        }
        assert!(
            self.equipment_locations
                .insert(equipment, position)
                .is_none(),
            "equipment drop cannot replace a stationary location"
        );
        assert!(
            self.equipment_by_position
                .entry(position)
                .or_default()
                .insert(equipment)
        );
        self.revision = next_revision;
    }

    pub(crate) fn apply_energy_store_placement(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        store: EnergyStoreId,
        position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !self.energy_store_locations.contains_key(&store),
            "energy-store placement cannot replace an existing location"
        );
        let previous = self.energy_store_locations.insert(store, position);
        assert!(previous.is_none());
        self.energy_stores_by_position
            .entry(position)
            .or_default()
            .insert(store);
        self.revision = next_revision;
    }

    pub(crate) fn apply_energy_store_location_removal(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        store: EnergyStoreId,
        expected_position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(
            self.energy_store_locations.remove(&store),
            Some(expected_position),
            "energy-store location changed after validation"
        );
        let at_position = self
            .energy_stores_by_position
            .get_mut(&expected_position)
            .expect("located energy store must appear in its derived position index");
        assert!(at_position.remove(&store));
        if at_position.is_empty() {
            self.energy_stores_by_position.remove(&expected_position);
        }
        self.revision = next_revision;
    }

    #[cfg(any(test, feature = "test-gameplay"))]
    pub(crate) fn apply_fluid_store_placement(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        store: FluidStoreId,
        position: VoxelCoord,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !self.fluid_store_locations.contains_key(&store),
            "fluid-store placement cannot replace an existing location"
        );
        let previous = self.fluid_store_locations.insert(store, position);
        assert!(previous.is_none());
        self.fluid_stores_by_position
            .entry(position)
            .or_default()
            .insert(store);
        self.revision = next_revision;
    }

    /// Atomically locate natural surface water already allocated by the
    /// fluid owner during the same validated terrain-expansion transaction.
    pub(crate) fn apply_generated_spring_locations(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        locations: Vec<(FluidStoreId, VoxelCoord)>,
    ) {
        assert!(!locations.is_empty());
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let mut previous = None;
        for (store, position) in locations {
            assert!(previous.is_none_or(|prior| prior < store));
            assert!(self.fluid_store_locations.insert(store, position).is_none());
            self.fluid_stores_by_position
                .entry(position)
                .or_default()
                .insert(store);
            previous = Some(store);
        }
        self.revision = next_revision;
    }

    pub(crate) fn apply_generated_opening_access(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        water_store: FluidStoreId,
        water_position: VoxelCoord,
        traversable_edges: Vec<TraversalEdge>,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            !self.fluid_store_locations.contains_key(&water_store),
            "generated opening water cannot replace an existing location"
        );
        assert!(
            self.traversable_edges.is_empty(),
            "initial generated traversal access requires pristine access state"
        );
        assert!(
            !traversable_edges.is_empty(),
            "initial generated traversal access cannot be empty"
        );
        assert!(
            traversable_edges.iter().all(|edge| edge.is_canonical()),
            "generated traversal edges must be canonical face neighbors"
        );
        assert!(
            traversable_edges.windows(2).all(|pair| pair[0] < pair[1]),
            "generated traversal edges must be strictly ordered and unique"
        );
        let previous = self
            .fluid_store_locations
            .insert(water_store, water_position);
        assert!(previous.is_none());
        self.fluid_stores_by_position
            .entry(water_position)
            .or_default()
            .insert(water_store);
        self.traversable_edges = traversable_edges;
        self.traversal_neighbors.clear();
        for edge in &self.traversable_edges {
            Self::index_traversal_edge(&mut self.traversal_neighbors, *edge);
        }
        self.revision = next_revision;
    }
}
