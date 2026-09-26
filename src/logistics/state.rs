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
    ground_stockpiles: BTreeMap<StockpileId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    detached_equipment: BTreeMap<EquipmentId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    detached_energy_stores: BTreeMap<EnergyStoreId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    fluid_stores: BTreeMap<FluidStoreId, VoxelCoord>,
}

impl LogisticsState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            player: None,
            ground_stockpiles: BTreeMap::new(),
            detached_equipment: BTreeMap::new(),
            detached_energy_stores: BTreeMap::new(),
            fluid_stores: BTreeMap::new(),
        }
    }

    /// Returns the world voxel of one finite fluid store when logistics owns a location.
    #[must_use]
    pub fn fluid_store_position(&self, store: FluidStoreId) -> Option<VoxelCoord> {
        self.fluid_stores.get(&store).copied()
    }

    /// Iterates explicitly located finite fluid stores in stable store-ID order.
    pub fn fluid_stores(&self) -> impl Iterator<Item = (FluidStoreId, VoxelCoord)> + '_ {
        self.fluid_stores
            .iter()
            .map(|(store, position)| (*store, *position))
    }

    /// Returns the world voxel of one detached finite-energy store managed by logistics.
    #[must_use]
    pub fn energy_store_position(&self, store: EnergyStoreId) -> Option<VoxelCoord> {
        self.detached_energy_stores.get(&store).copied()
    }

    /// Iterates detached energy-store locations in stable store-ID order.
    pub fn detached_energy_stores(&self) -> impl Iterator<Item = (EnergyStoreId, VoxelCoord)> + '_ {
        self.detached_energy_stores
            .iter()
            .map(|(store, position)| (*store, *position))
    }

    /// Returns the world voxel of one detached, unmounted equipment instance managed by logistics.
    #[must_use]
    pub fn equipment_position(&self, equipment: EquipmentId) -> Option<VoxelCoord> {
        self.detached_equipment.get(&equipment).copied()
    }

    /// Iterates detached equipment locations in stable equipment-ID order.
    pub fn detached_equipment(&self) -> impl Iterator<Item = (EquipmentId, VoxelCoord)> + '_ {
        self.detached_equipment
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

    /// Returns the world voxel of one loose, unsupported stockpile managed by logistics.
    #[must_use]
    pub fn ground_stockpile_position(&self, stockpile: StockpileId) -> Option<VoxelCoord> {
        self.ground_stockpiles.get(&stockpile).copied()
    }

    /// Returns the known world voxel of a player-carried or loose ground stockpile.
    #[must_use]
    pub fn stockpile_position(&self, stockpile: StockpileId) -> Option<VoxelCoord> {
        if let Some(player) = self.player
            && player.carried_stockpile() == stockpile
        {
            return Some(player.position());
        }
        self.ground_stockpile_position(stockpile)
    }

    /// Iterates loose ground stockpiles in stable stockpile-ID order.
    pub fn ground_stockpiles(&self) -> impl Iterator<Item = (StockpileId, VoxelCoord)> + '_ {
        self.ground_stockpiles
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
            !self.ground_stockpiles.contains_key(&stockpile),
            "ground stockpile placement cannot replace an existing location"
        );
        assert!(
            self.player
                .is_none_or(|player| player.carried_stockpile() != stockpile),
            "player-carried custody cannot also be placed on the ground"
        );
        let previous = self.ground_stockpiles.insert(stockpile, position);
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
            !self.detached_equipment.contains_key(&equipment),
            "detached equipment placement cannot replace an existing location"
        );
        let previous = self.detached_equipment.insert(equipment, position);
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
            self.detached_equipment.remove(&equipment),
            Some(expected_position),
            "detached equipment location changed after validation"
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
            !self.detached_energy_stores.contains_key(&store),
            "detached energy-store placement cannot replace an existing location"
        );
        let previous = self.detached_energy_stores.insert(store, position);
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
            self.detached_energy_stores.remove(&store),
            Some(expected_position),
            "detached energy-store location changed after validation"
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
            !self.fluid_stores.contains_key(&store),
            "fluid-store placement cannot replace an existing location"
        );
        let previous = self.fluid_stores.insert(store, position);
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
