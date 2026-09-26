//! Persistent logistics records and actor-facing carrying assessment.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::spatial::VoxelCoord;

/// Persistent world/custody record for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerLogisticsRecord {
    position: VoxelCoord,
    carried_stockpile: StockpileId,
}

impl PlayerLogisticsRecord {
    pub(super) const fn new(position: VoxelCoord, carried_stockpile: StockpileId) -> Self {
        Self {
            position,
            carried_stockpile,
        }
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn carried_stockpile(self) -> StockpileId {
        self.carried_stockpile
    }
}

/// Persistent owner for player location and carried-custody identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsState {
    revision: u64,
    player: Option<PlayerLogisticsRecord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    stockpile_locations: BTreeMap<StockpileId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    equipment_locations: BTreeMap<EquipmentId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    energy_store_locations: BTreeMap<EnergyStoreId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    fluid_store_locations: BTreeMap<FluidStoreId, VoxelCoord>,
}

impl LogisticsState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            player: None,
            stockpile_locations: BTreeMap::new(),
            equipment_locations: BTreeMap::new(),
            energy_store_locations: BTreeMap::new(),
            fluid_store_locations: BTreeMap::new(),
        }
    }

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

    /// Returns the world voxel of one equipment instance managed by logistics.
    #[must_use]
    pub fn equipment_position(&self, equipment: EquipmentId) -> Option<VoxelCoord> {
        self.equipment_locations.get(&equipment).copied()
    }

    /// Iterates equipment locations in stable equipment-ID order.
    pub fn equipment_locations(&self) -> impl Iterator<Item = (EquipmentId, VoxelCoord)> + '_ {
        self.equipment_locations
            .iter()
            .map(|(equipment, position)| (*equipment, *position))
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn player(&self) -> Option<&PlayerLogisticsRecord> {
        self.player.as_ref()
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

    pub(super) fn apply_initialization(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        player: PlayerLogisticsRecord,
    ) {
        assert_eq!(
            self.revision, expected_revision,
            "player logistics initialization requires its validated owner revision"
        );
        assert_eq!(
            expected_revision.checked_add(1),
            Some(next_revision),
            "player logistics initialization must advance revision exactly once"
        );
        assert!(
            self.player.is_none(),
            "player logistics initialization cannot replace an existing player record"
        );
        self.player = Some(player);
        self.revision = next_revision;
    }

    pub(super) fn apply_ground_placement(
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
        self.revision = next_revision;
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
        self.revision = next_revision;
    }

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
        self.revision = next_revision;
    }
}

/// Actor-facing snapshot of current carried material capacity at the player's world position.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCarryingAssessment {
    position: VoxelCoord,
    stockpile: StockpileId,
    capacity: Mass,
    stored: Mass,
    available: Mass,
}

impl PlayerCarryingAssessment {
    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub const fn stockpile(self) -> StockpileId {
        self.stockpile
    }

    #[must_use]
    pub const fn capacity(self) -> Mass {
        self.capacity
    }

    #[must_use]
    pub const fn stored(self) -> Mass {
        self.stored
    }

    #[must_use]
    pub const fn available(self) -> Mass {
        self.available
    }
}

/// Returns the player's current position and inventory-owned carried capacity.
#[must_use]
pub fn assess_player_carrying(state: &AppState) -> Option<PlayerCarryingAssessment> {
    let player = state.logistics().player().copied()?;
    let stockpile = state
        .inventory()
        .get_stockpile(player.carried_stockpile())
        .unwrap_or_else(|| {
            panic!(
                "runtime invariant broken: player carried stockpile {} disappeared",
                player.carried_stockpile().value()
            )
        });
    Some(PlayerCarryingAssessment {
        position: player.position(),
        stockpile: stockpile.id(),
        capacity: stockpile.capacity(),
        stored: stockpile.stored_mass(),
        available: stockpile.available_capacity(),
    })
}
