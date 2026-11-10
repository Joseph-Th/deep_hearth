//! Persistent logistics records and actor-facing carrying assessment.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::core::quantity::Mass;
use crate::core::state::AppState;
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::material::CommodityKey;
use crate::spatial::VoxelCoord;

use super::body::PlayerBodyState;
use super::traversal::TraversalEdge;

mod control;
mod custody_mutations;
mod queries;
pub(crate) use control::{
    LocomotionExertionAccumulator, PlayerPendingPrimaryBreak, PlayerPendingSecondaryUse,
};
/// Familiar fixed quick-access bar width.
pub const PLAYER_QUICK_ACCESS_SLOT_COUNT: usize = 9;

/// Stable zero-based quick-access slot identity.
///
/// The field stays private and deserialization enforces the same bound as the
/// constructor, so malformed external input cannot manufacture an invalid slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct PlayerQuickAccessSlot(u8);

impl<'de> Deserialize<'de> for PlayerQuickAccessSlot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let index = u8::deserialize(deserializer)?;
        Self::new(index).ok_or_else(|| de::Error::custom("quick-access slot index is out of range"))
    }
}

impl PlayerQuickAccessSlot {
    pub const FIRST: Self = Self(0);

    #[must_use]
    pub const fn new(index: u8) -> Option<Self> {
        if index < PLAYER_QUICK_ACCESS_SLOT_COUNT as u8 {
            Some(Self(index))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    pub(crate) const fn is_valid(self) -> bool {
        self.0 < PLAYER_QUICK_ACCESS_SLOT_COUNT as u8
    }
}

/// Persisted quick-access selection for the player's direct-action hand.
///
/// Equipment references canonical carried equipment identity. Material references a semantic
/// commodity in the player's carried stockpile rather than a runtime lot ID, so ordinary lot
/// coalescing and splitting cannot silently invalidate quick-access intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerActiveSelection {
    Equipment(EquipmentId),
    Material(CommodityKey),
}

impl PlayerActiveSelection {
    #[must_use]
    pub const fn equipment(equipment: EquipmentId) -> Self {
        Self::Equipment(equipment)
    }

    #[must_use]
    pub const fn material(commodity: CommodityKey) -> Self {
        Self::Material(commodity)
    }
}

/// Persistent world/custody record for the single locally controlled player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerLogisticsRecord {
    body: PlayerBodyState,
    carried_stockpile: StockpileId,
    respawn_position: VoxelCoord,
    locomotion_exertion: LocomotionExertionAccumulator,
    pending_fall_vitality_loss_ppm: u32,
}

impl PlayerLogisticsRecord {
    pub(super) const fn new(body: PlayerBodyState, carried_stockpile: StockpileId) -> Self {
        Self {
            body,
            carried_stockpile,
            respawn_position: body.voxel_position(),
            locomotion_exertion: LocomotionExertionAccumulator::ZERO,
            pending_fall_vitality_loss_ppm: 0,
        }
    }

    #[must_use]
    pub const fn position(self) -> VoxelCoord {
        self.body.voxel_position()
    }

    #[must_use]
    pub const fn body(self) -> PlayerBodyState {
        self.body
    }

    #[must_use]
    pub const fn carried_stockpile(self) -> StockpileId {
        self.carried_stockpile
    }

    #[must_use]
    pub const fn respawn_position(self) -> VoxelCoord {
        self.respawn_position
    }

    pub(crate) const fn locomotion_exertion(self) -> LocomotionExertionAccumulator {
        self.locomotion_exertion
    }

    #[must_use]
    pub const fn pending_fall_vitality_loss_ppm(self) -> u32 {
        self.pending_fall_vitality_loss_ppm
    }
}

/// Persistent owner for player location and carried-custody identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsState {
    revision: u64,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    player: Option<PlayerLogisticsRecord>,
    traversable_edges: Vec<TraversalEdge>,
    /// Local route adjacency rebuilt from authoritative persisted edges.
    #[serde(skip)]
    traversal_neighbors: BTreeMap<VoxelCoord, BTreeSet<VoxelCoord>>,
    carried_equipment: Vec<EquipmentId>,
    quick_access_slots: [Option<PlayerActiveSelection>; PLAYER_QUICK_ACCESS_SLOT_COUNT],
    active_quick_access_slot: PlayerQuickAccessSlot,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    pending_secondary_use: Option<PlayerPendingSecondaryUse>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_required_optional")]
    pending_primary_break: Option<PlayerPendingPrimaryBreak>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    stockpile_locations: BTreeMap<StockpileId, VoxelCoord>,
    /// Rebuilt from ground custody; the player-carried pack is not stationary.
    #[serde(skip)]
    stockpiles_by_position: BTreeMap<VoxelCoord, BTreeSet<StockpileId>>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    equipment_locations: BTreeMap<EquipmentId, VoxelCoord>,
    /// Rebuildable stationary-equipment spatial index; carried equipment is not indexed here.
    #[serde(skip)]
    equipment_by_position: BTreeMap<VoxelCoord, BTreeSet<EquipmentId>>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    energy_store_locations: BTreeMap<EnergyStoreId, VoxelCoord>,
    /// Local-light and spatial access index reconstructed solely from location custody.
    #[serde(skip)]
    energy_stores_by_position: BTreeMap<VoxelCoord, BTreeSet<EnergyStoreId>>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    fluid_store_locations: BTreeMap<FluidStoreId, VoxelCoord>,
    /// Local finite-fluid lookup rebuilt solely from authoritative location custody.
    #[serde(skip)]
    fluid_stores_by_position: BTreeMap<VoxelCoord, BTreeSet<FluidStoreId>>,
}

/// Custody created by a death and respawn: old carried matter and equipment
/// become one ground cache while the new player receives a fresh carrier.
pub(super) struct PlayerRespawnCustody<'a> {
    pub(super) fresh_carried_stockpile: StockpileId,
    pub(super) death_position: VoxelCoord,
    pub(super) dropped_equipment: &'a [EquipmentId],
}

impl LogisticsState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            player: None,
            traversable_edges: Vec::new(),
            traversal_neighbors: BTreeMap::new(),
            carried_equipment: Vec::new(),
            quick_access_slots: [None; PLAYER_QUICK_ACCESS_SLOT_COUNT],
            active_quick_access_slot: PlayerQuickAccessSlot::FIRST,
            pending_secondary_use: None,
            pending_primary_break: None,
            stockpile_locations: BTreeMap::new(),
            stockpiles_by_position: BTreeMap::new(),
            equipment_locations: BTreeMap::new(),
            equipment_by_position: BTreeMap::new(),
            energy_store_locations: BTreeMap::new(),
            energy_stores_by_position: BTreeMap::new(),
            fluid_store_locations: BTreeMap::new(),
            fluid_stores_by_position: BTreeMap::new(),
        }
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

    pub(super) fn apply_player_respawn(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        expected_player: PlayerLogisticsRecord,
        respawn_body: PlayerBodyState,
        custody: PlayerRespawnCustody<'_>,
    ) {
        let PlayerRespawnCustody {
            fresh_carried_stockpile: carried_stockpile,
            death_position,
            dropped_equipment,
        } = custody;
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(self.player, Some(expected_player));
        assert_eq!(
            self.carried_equipment.as_slice(),
            dropped_equipment,
            "respawn equipment custody changed after validation"
        );
        assert!(
            !self
                .stockpile_locations
                .contains_key(&expected_player.carried_stockpile()),
            "player carried stockpile cannot already have a ground location"
        );
        assert!(
            !self.stockpile_locations.contains_key(&carried_stockpile),
            "new carried stockpile cannot already have a ground location"
        );
        let previous = self
            .stockpile_locations
            .insert(expected_player.carried_stockpile(), death_position);
        assert!(previous.is_none());
        assert!(
            self.stockpiles_by_position
                .entry(death_position)
                .or_default()
                .insert(expected_player.carried_stockpile())
        );
        for equipment in dropped_equipment {
            assert!(
                !self.equipment_locations.contains_key(equipment),
                "carried equipment cannot already have a stationary location"
            );
            let previous = self.equipment_locations.insert(*equipment, death_position);
            assert!(previous.is_none());
            assert!(
                self.equipment_by_position
                    .entry(death_position)
                    .or_default()
                    .insert(*equipment)
            );
        }
        self.carried_equipment.clear();
        for selection in &mut self.quick_access_slots {
            if matches!(selection, Some(PlayerActiveSelection::Equipment(_))) {
                *selection = None;
            }
        }
        self.pending_secondary_use = None;
        self.pending_primary_break = None;
        self.player = Some(PlayerLogisticsRecord {
            body: respawn_body,
            carried_stockpile,
            respawn_position: expected_player.respawn_position(),
            locomotion_exertion: LocomotionExertionAccumulator::ZERO,
            pending_fall_vitality_loss_ppm: 0,
        });
        self.revision = next_revision;
    }

    pub(crate) fn apply_pending_secondary_use(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        pending: PlayerPendingSecondaryUse,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.player.is_some(),
            "buffered contextual use requires player logistics"
        );
        assert!(
            !self.has_pending_direct_action(),
            "buffered contextual use cannot replace an existing direct-action request"
        );
        self.pending_secondary_use = Some(pending);
        self.revision = next_revision;
    }

    pub(crate) fn apply_clear_pending_secondary_use(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        expected: PlayerPendingSecondaryUse,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(self.pending_secondary_use, Some(expected));
        self.pending_secondary_use = None;
        self.revision = next_revision;
    }

    pub(crate) fn apply_pending_primary_break(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        pending: PlayerPendingPrimaryBreak,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert!(
            self.player.is_some(),
            "buffered structural break requires player logistics"
        );
        assert!(
            !self.has_pending_direct_action(),
            "buffered structural break cannot replace an existing direct-action request"
        );
        self.pending_primary_break = Some(pending);
        self.revision = next_revision;
    }

    pub(crate) fn apply_clear_pending_primary_break(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        expected: PlayerPendingPrimaryBreak,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        assert_eq!(self.pending_primary_break, Some(expected));
        self.pending_primary_break = None;
        self.revision = next_revision;
    }

    #[cfg(test)]
    pub(super) fn relocate_player_for_test(&mut self, destination: VoxelCoord) {
        let next_revision = self
            .revision
            .checked_add(1)
            .unwrap_or_else(|| panic!("player relocation fixture exhausted logistics revisions"));
        let player = self
            .player
            .as_mut()
            .unwrap_or_else(|| panic!("player relocation fixture requires admitted logistics"));
        player.body = player.body.snap_to_voxel_floor(destination);
        self.revision = next_revision;
    }

    pub(crate) fn apply_player_body_motion(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        expected_body: PlayerBodyState,
        body: PlayerBodyState,
        locomotion_exertion: LocomotionExertionAccumulator,
        pending_fall_vitality_loss_ppm: u32,
    ) {
        assert_eq!(self.revision, expected_revision);
        assert_eq!(expected_revision.checked_add(1), Some(next_revision));
        let player = self.player.as_mut().unwrap_or_else(|| {
            unreachable!("validated player body motion requires initialized logistics")
        });
        assert_eq!(
            player.body, expected_body,
            "player body changed after motion planning"
        );
        player.body = body;
        player.locomotion_exertion = locomotion_exertion;
        player.pending_fall_vitality_loss_ppm = pending_fall_vitality_loss_ppm;
        self.revision = next_revision;
    }
}

#[cfg(any(test, feature = "test-gameplay"))]
pub(crate) fn add_traversable_edge_for_test(
    state: &mut AppState,
    first: VoxelCoord,
    second: VoxelCoord,
) {
    let edge = TraversalEdge::new(first, second)
        .unwrap_or_else(|| panic!("test traversal edge must be face-adjacent"));
    let revision = state.logistics().revision();
    let next_revision = revision
        .checked_add(1)
        .unwrap_or_else(|| panic!("test traversal edge exhausted logistics revision"));
    let logistics = state.logistics_state_mut();
    assert!(
        logistics.traversable_edges.binary_search(&edge).is_err(),
        "test traversal edge already exists"
    );
    let insertion = logistics
        .traversable_edges
        .binary_search(&edge)
        .unwrap_or_else(|index| index);
    logistics.traversable_edges.insert(insertion, edge);
    LogisticsState::index_traversal_edge(&mut logistics.traversal_neighbors, edge);
    logistics.revision = next_revision;
}

/// Actor-facing snapshot of total body-scale carried load at the player's world position.
#[must_use]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerCarryingAssessment {
    position: VoxelCoord,
    stockpile: StockpileId,
    capacity: Mass,
    stored: Mass,
    available: Mass,
    equipment: Mass,
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

    #[must_use]
    pub const fn equipment(self) -> Mass {
        self.equipment
    }
}

/// Returns the player's current position and total load across material and portable equipment.
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
    let equipment = state
        .logistics()
        .carried_equipment()
        .fold(Mass::ZERO, |total, equipment| {
            let mass = state
                .equipment()
                .get_equipment(equipment)
                .unwrap_or_else(|| {
                    panic!("runtime invariant broken: carried equipment disappeared")
                })
                .embodied_mass();
            total
                .checked_add(mass)
                .unwrap_or_else(|| panic!("carried equipment mass overflowed"))
        });
    let capacity = stockpile
        .capacity()
        .checked_add(equipment)
        .unwrap_or_else(|| panic!("player body carrying capacity overflowed"));
    let stored = stockpile
        .stored_mass()
        .checked_add(equipment)
        .unwrap_or_else(|| panic!("player total carried mass overflowed"));
    Some(PlayerCarryingAssessment {
        position: player.position(),
        stockpile: stockpile.id(),
        capacity,
        stored,
        available: stockpile.available_capacity(),
        equipment,
    })
}

mod spatial_indexes;

#[cfg(test)]
#[path = "traversal_index_tests.rs"]
mod traversal_index_tests;

#[cfg(test)]
#[path = "stationary_index_tests.rs"]
mod stationary_index_tests;
