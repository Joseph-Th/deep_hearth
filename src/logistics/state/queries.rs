//! Read-only logistics location, carried-custody and route projections.

use std::collections::{BTreeMap, BTreeSet};

use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

use super::{
    LogisticsState, PlayerActiveSelection, PlayerLogisticsRecord, PlayerPendingPrimaryBreak,
    PlayerPendingSecondaryUse, PlayerQuickAccessSlot, TraversalEdge,
};

impl LogisticsState {
    /// Returns the world voxel of one finite fluid store when logistics owns a location.
    #[must_use]
    pub fn fluid_store_position(&self, store: FluidStoreId) -> Option<VoxelCoord> {
        self.fluid_store_locations.get(&store).copied()
    }

    /// Iterates explicitly located finite fluid stores in stable store-ID order.
    pub fn fluid_store_locations(&self) -> impl Iterator<Item = (FluidStoreId, VoxelCoord)> + '_ {
        self.fluid_store_locations
            .iter()
            .map(|(store, position)| (*store, *position))
    }

    /// A single exact location's finite-fluid stores in stable ID order.
    pub(crate) fn fluid_stores_at(
        &self,
        position: VoxelCoord,
    ) -> impl Iterator<Item = FluidStoreId> + '_ {
        self.fluid_stores_by_position
            .get(&position)
            .into_iter()
            .flat_map(|stores| stores.iter().copied())
    }

    /// Returns the world voxel of one finite-energy store managed by logistics.
    #[must_use]
    pub fn energy_store_position(&self, store: EnergyStoreId) -> Option<VoxelCoord> {
        self.energy_store_locations.get(&store).copied()
    }

    /// Iterates energy-store locations in stable store-ID order.
    pub fn energy_store_locations(&self) -> impl Iterator<Item = (EnergyStoreId, VoxelCoord)> + '_ {
        self.energy_store_locations
            .iter()
            .map(|(store, position)| (*store, *position))
    }

    /// Exact-position energy custody in stable store-ID order, without remote scans.
    pub(crate) fn energy_stores_at(
        &self,
        position: VoxelCoord,
    ) -> impl Iterator<Item = EnergyStoreId> + '_ {
        self.energy_stores_by_position
            .get(&position)
            .into_iter()
            .flat_map(|stores| stores.iter().copied())
    }

    /// Returns the world voxel of one equipment instance managed by logistics.
    #[must_use]
    pub fn equipment_position(&self, equipment: EquipmentId) -> Option<VoxelCoord> {
        if self.carried_equipment.binary_search(&equipment).is_ok() {
            return self.player.map(PlayerLogisticsRecord::position);
        }
        self.stationary_equipment_position(equipment)
    }

    /// Returns the stationary world voxel of one equipment instance, excluding player custody.
    #[must_use]
    pub fn stationary_equipment_position(&self, equipment: EquipmentId) -> Option<VoxelCoord> {
        self.equipment_locations.get(&equipment).copied()
    }

    /// Iterates stationary equipment locations in stable equipment-ID order.
    pub fn equipment_locations(&self) -> impl Iterator<Item = (EquipmentId, VoxelCoord)> + '_ {
        self.equipment_locations
            .iter()
            .map(|(equipment, position)| (*equipment, *position))
    }

    /// Iterates player-carried equipment in stable equipment-ID order.
    pub fn carried_equipment(&self) -> impl Iterator<Item = EquipmentId> + '_ {
        self.carried_equipment.iter().copied()
    }

    /// Returns the selection bound to one persisted quick-access slot.
    #[must_use]
    pub fn quick_access_slot(&self, slot: PlayerQuickAccessSlot) -> Option<PlayerActiveSelection> {
        self.quick_access_slots[slot.index()]
    }

    /// Iterates all nine persisted quick-access slots in familiar left-to-right order.
    pub fn quick_access_slots(
        &self,
    ) -> impl ExactSizeIterator<Item = (PlayerQuickAccessSlot, Option<PlayerActiveSelection>)> + '_
    {
        self.quick_access_slots
            .iter()
            .copied()
            .enumerate()
            .map(|(index, selection)| {
                let index = u8::try_from(index)
                    .unwrap_or_else(|_| unreachable!("quick-access slot index fits u8"));
                let slot = PlayerQuickAccessSlot::new(index)
                    .unwrap_or_else(|| unreachable!("quick-access array uses authored slot count"));
                (slot, selection)
            })
    }

    /// Returns the persisted currently selected quick-access slot.
    #[must_use]
    pub const fn active_quick_access_slot(&self) -> PlayerQuickAccessSlot {
        self.active_quick_access_slot
    }

    /// Returns the current direct-action selection from the selected quick-access slot.
    ///
    /// This is a reference into canonical carried custody, never a second item/matter container.
    #[must_use]
    pub fn active_selection(&self) -> Option<PlayerActiveSelection> {
        self.quick_access_slot(self.active_quick_access_slot)
    }

    #[must_use]
    pub(crate) const fn pending_secondary_use(&self) -> Option<PlayerPendingSecondaryUse> {
        self.pending_secondary_use
    }

    #[must_use]
    pub(crate) const fn pending_primary_break(&self) -> Option<PlayerPendingPrimaryBreak> {
        self.pending_primary_break
    }

    #[must_use]
    pub(crate) const fn has_pending_direct_action(&self) -> bool {
        self.pending_secondary_use.is_some() || self.pending_primary_break.is_some()
    }

    /// Iterates equipment present at the admitted player's voxel in stable equipment-ID order.
    ///
    /// Stationary equipment at the player voxel and player-carried equipment are both local. The
    /// merge stays inside logistics custody so actor observation does not need to scan unrelated
    /// equipment-owner records or inspect remote logistics references.
    pub(crate) fn player_local_equipment(&self) -> impl Iterator<Item = EquipmentId> + '_ {
        let player_position = self.player.map(PlayerLogisticsRecord::position);
        let mut stationary = player_position
            .into_iter()
            .flat_map(|position| self.equipment_by_position.get(&position))
            .flat_map(|equipment| equipment.iter().copied())
            .peekable();
        let mut carried = self
            .carried_equipment()
            .filter(move |_| player_position.is_some())
            .peekable();

        std::iter::from_fn(
            move || match (stationary.peek().copied(), carried.peek().copied()) {
                (Some(stationary_id), Some(carried_id)) if stationary_id <= carried_id => {
                    stationary.next()
                }
                (Some(_), Some(_)) => carried.next(),
                (Some(_), None) => stationary.next(),
                (None, Some(_)) => carried.next(),
                (None, None) => None,
            },
        )
    }

    #[must_use]
    pub fn is_equipment_carried(&self, equipment: EquipmentId) -> bool {
        self.carried_equipment.binary_search(&equipment).is_ok()
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub(crate) fn is_pristine_generation_state(&self) -> bool {
        self.revision == 0
            && self.player.is_none()
            && self.traversable_edges.is_empty()
            && self.carried_equipment.is_empty()
            && self.quick_access_slots.iter().all(Option::is_none)
            && self.active_quick_access_slot == PlayerQuickAccessSlot::FIRST
            && self.pending_secondary_use.is_none()
            && self.pending_primary_break.is_none()
            && self.stockpile_locations.is_empty()
            && self.equipment_locations.is_empty()
            && self.energy_store_locations.is_empty()
            && self.fluid_store_locations.is_empty()
    }

    #[must_use]
    pub const fn player(&self) -> Option<&PlayerLogisticsRecord> {
        self.player.as_ref()
    }

    pub(crate) fn traversable_edges(&self) -> impl Iterator<Item = TraversalEdge> + '_ {
        self.traversable_edges.iter().copied()
    }

    #[must_use]
    pub(crate) fn has_traversal_edge(&self, origin: VoxelCoord, destination: VoxelCoord) -> bool {
        TraversalEdge::new(origin, destination)
            .is_some_and(|edge| self.traversable_edges.binary_search(&edge).is_ok())
    }

    pub(crate) fn traversable_destinations(
        &self,
        origin: VoxelCoord,
    ) -> impl Iterator<Item = VoxelCoord> + '_ {
        self.traversal_neighbors
            .get(&origin)
            .into_iter()
            .flat_map(|neighbors| neighbors.iter().copied())
    }

    pub(super) fn index_traversal_edge(
        neighbors: &mut BTreeMap<VoxelCoord, BTreeSet<VoxelCoord>>,
        edge: TraversalEdge,
    ) {
        let (first, second) = edge.endpoints();
        neighbors.entry(first).or_default().insert(second);
        neighbors.entry(second).or_default().insert(first);
    }

    pub(crate) fn has_valid_traversal_index(&self) -> bool {
        let mut expected = BTreeMap::new();
        for edge in &self.traversable_edges {
            Self::index_traversal_edge(&mut expected, *edge);
        }
        self.traversal_neighbors == expected
    }

    /// Returns the world voxel of one stationary stockpile managed by logistics.
    #[must_use]
    pub fn stationary_stockpile_position(&self, stockpile: StockpileId) -> Option<VoxelCoord> {
        self.stockpile_locations.get(&stockpile).copied()
    }

    /// Returns the known world voxel of a player-carried or stationary stockpile.
    #[must_use]
    pub fn stockpile_position(&self, stockpile: StockpileId) -> Option<VoxelCoord> {
        if let Some(player) = self.player
            && player.carried_stockpile() == stockpile
        {
            return Some(player.position());
        }
        self.stationary_stockpile_position(stockpile)
    }

    /// Iterates stationary stockpile locations in stable stockpile-ID order.
    pub fn stockpile_locations(&self) -> impl Iterator<Item = (StockpileId, VoxelCoord)> + '_ {
        self.stockpile_locations
            .iter()
            .map(|(stockpile, position)| (*stockpile, *position))
    }
}
