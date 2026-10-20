//! Persistent logistics records and actor-facing carrying assessment.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Energy, Mass, Volume};
use crate::core::state::AppState;
use crate::core::time::{CONTROL_TICKS_PER_SIMULATION_TICK, ControlTick};
use crate::energy::EnergyStoreId;
use crate::equipment::EquipmentId;
use crate::fluid::FluidStoreId;
use crate::inventory::StockpileId;
use crate::material::CommodityKey;
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::structural::StructuralElementId;
use crate::survival::SurvivalExertion;

use super::body::PlayerBodyState;
use super::traversal::TraversalEdge;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocomotionExertionAccumulator {
    pending_energy: Energy,
    pending_hydration: Volume,
    energy_remainder: u8,
    hydration_remainder: u8,
}

/// Persisted one-shot structural-break input waiting for the next slow-simulation boundary.
///
/// The exact structural identity is retained so delayed admission cannot fall through to terrain
/// excavation or a replacement member if the originally pointed structure changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlayerPendingPrimaryBreak {
    requested_at: ControlTick,
    position: VoxelCoord,
    element: StructuralElementId,
    direction_x_ppm: i32,
    direction_y_ppm: i32,
    direction_z_ppm: i32,
}

impl PlayerPendingPrimaryBreak {
    pub(crate) const fn new(
        requested_at: ControlTick,
        position: VoxelCoord,
        element: StructuralElementId,
        direction: ViewDirection,
    ) -> Self {
        Self {
            requested_at,
            position,
            element,
            direction_x_ppm: direction.x_parts_per_million(),
            direction_y_ppm: direction.y_parts_per_million(),
            direction_z_ppm: direction.z_parts_per_million(),
        }
    }

    #[must_use]
    pub(crate) const fn requested_at(self) -> ControlTick {
        self.requested_at
    }

    #[must_use]
    pub(crate) const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub(crate) const fn element(self) -> StructuralElementId {
        self.element
    }

    #[must_use]
    pub(crate) fn direction(self) -> Option<ViewDirection> {
        ViewDirection::new(
            self.direction_x_ppm,
            self.direction_y_ppm,
            self.direction_z_ppm,
        )
        .ok()
    }
}

/// Persisted one-shot contextual-use input waiting for the next slow-simulation boundary.
///
/// The record snapshots only familiar control context. The eventual action is resolved again
/// against current owner state, so this buffer is never authorization for a stale world target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlayerPendingSecondaryUse {
    requested_at: ControlTick,
    position: VoxelCoord,
    selection: Option<PlayerActiveSelection>,
    direction_x_ppm: i32,
    direction_y_ppm: i32,
    direction_z_ppm: i32,
}

impl PlayerPendingSecondaryUse {
    pub(crate) const fn new(
        requested_at: ControlTick,
        position: VoxelCoord,
        selection: Option<PlayerActiveSelection>,
        direction: ViewDirection,
    ) -> Self {
        Self {
            requested_at,
            position,
            selection,
            direction_x_ppm: direction.x_parts_per_million(),
            direction_y_ppm: direction.y_parts_per_million(),
            direction_z_ppm: direction.z_parts_per_million(),
        }
    }

    #[must_use]
    pub(crate) const fn requested_at(self) -> ControlTick {
        self.requested_at
    }

    #[must_use]
    pub(crate) const fn position(self) -> VoxelCoord {
        self.position
    }

    #[must_use]
    pub(crate) const fn selection(self) -> Option<PlayerActiveSelection> {
        self.selection
    }

    #[must_use]
    pub(crate) fn direction(self) -> Option<ViewDirection> {
        ViewDirection::new(
            self.direction_x_ppm,
            self.direction_y_ppm,
            self.direction_z_ppm,
        )
        .ok()
    }
}

/// Familiar fixed quick-access bar width.
pub const PLAYER_QUICK_ACCESS_SLOT_COUNT: usize = 9;

/// Stable zero-based quick-access slot identity.
///
/// The field stays private so ordinary callers cannot construct an out-of-range slot. Trusted-load
/// validation still checks decoded values because persistence is untrusted until promotion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerQuickAccessSlot(u8);

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

impl LocomotionExertionAccumulator {
    pub(crate) const ZERO: Self = Self {
        pending_energy: Energy::ZERO,
        pending_hydration: Volume::ZERO,
        energy_remainder: 0,
        hydration_remainder: 0,
    };

    pub(crate) fn checked_accrue(self, exertion: SurvivalExertion) -> Option<Self> {
        let denominator = u128::from(CONTROL_TICKS_PER_SIMULATION_TICK);
        let energy_numerator = exertion
            .energy_cost_per_tick()
            .nanojoules()
            .checked_add(u128::from(self.energy_remainder))?;
        let hydration_numerator = u128::from(exertion.hydration_loss_per_tick().microliters())
            .checked_add(u128::from(self.hydration_remainder))?;
        let energy_whole = energy_numerator / denominator;
        let hydration_whole = hydration_numerator / denominator;
        let energy_remainder = u8::try_from(energy_numerator % denominator).ok()?;
        let hydration_remainder = u8::try_from(hydration_numerator % denominator).ok()?;
        Some(Self {
            pending_energy: self
                .pending_energy
                .checked_add(Energy::from_nanojoules(energy_whole))?,
            pending_hydration: self
                .pending_hydration
                .checked_add(Volume::from_microliters(
                    u64::try_from(hydration_whole).ok()?,
                ))?,
            energy_remainder,
            hydration_remainder,
        })
    }

    /// Adds one discrete locomotion event whose authored cost is already a whole event total.
    ///
    /// Continuous body control uses [`Self::checked_accrue`] because its exertion is a slow-tick
    /// rate sampled at control frequency. Impulses such as jump launch must not be divided by the
    /// control/slow-tick ratio or their one-shot cost would depend on input sampling frequency.
    pub(crate) fn checked_accrue_impulse(self, exertion: SurvivalExertion) -> Option<Self> {
        Some(Self {
            pending_energy: self
                .pending_energy
                .checked_add(exertion.energy_cost_per_tick())?,
            pending_hydration: self
                .pending_hydration
                .checked_add(exertion.hydration_loss_per_tick())?,
            energy_remainder: self.energy_remainder,
            hydration_remainder: self.hydration_remainder,
        })
    }

    pub(crate) const fn pending(self) -> SurvivalExertion {
        SurvivalExertion::new(self.pending_energy, self.pending_hydration)
    }

    pub(crate) const fn settled(self) -> Self {
        Self {
            pending_energy: Energy::ZERO,
            pending_hydration: Volume::ZERO,
            energy_remainder: self.energy_remainder,
            hydration_remainder: self.hydration_remainder,
        }
    }

    pub(crate) const fn has_valid_remainders(self) -> bool {
        self.energy_remainder < CONTROL_TICKS_PER_SIMULATION_TICK as u8
            && self.hydration_remainder < CONTROL_TICKS_PER_SIMULATION_TICK as u8
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
    player: Option<PlayerLogisticsRecord>,
    traversable_edges: Vec<TraversalEdge>,
    /// Local route adjacency rebuilt from authoritative persisted edges.
    #[serde(skip)]
    traversal_neighbors: BTreeMap<VoxelCoord, BTreeSet<VoxelCoord>>,
    carried_equipment: Vec<EquipmentId>,
    quick_access_slots: [Option<PlayerActiveSelection>; PLAYER_QUICK_ACCESS_SLOT_COUNT],
    active_quick_access_slot: PlayerQuickAccessSlot,
    pending_secondary_use: Option<PlayerPendingSecondaryUse>,
    pending_primary_break: Option<PlayerPendingPrimaryBreak>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    stockpile_locations: BTreeMap<StockpileId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    equipment_locations: BTreeMap<EquipmentId, VoxelCoord>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    energy_store_locations: BTreeMap<EnergyStoreId, VoxelCoord>,
    /// Local-light and spatial access index reconstructed solely from location custody.
    #[serde(skip)]
    energy_stores_by_position: BTreeMap<VoxelCoord, BTreeSet<EnergyStoreId>>,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    fluid_store_locations: BTreeMap<FluidStoreId, VoxelCoord>,
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
            equipment_locations: BTreeMap::new(),
            energy_store_locations: BTreeMap::new(),
            energy_stores_by_position: BTreeMap::new(),
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
        self.energy_stores_by_position.clear();
        for (store, position) in &self.energy_store_locations {
            self.energy_stores_by_position
                .entry(*position)
                .or_default()
                .insert(*store);
        }
    }

    pub(crate) fn has_valid_energy_position_index(&self) -> bool {
        let mut expected = BTreeMap::<VoxelCoord, BTreeSet<EnergyStoreId>>::new();
        for (store, position) in &self.energy_store_locations {
            expected.entry(*position).or_default().insert(*store);
        }
        self.energy_stores_by_position == expected
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
        let mut stationary = self
            .equipment_locations()
            .filter(move |(_, position)| Some(*position) == player_position)
            .map(|(equipment, _)| equipment)
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

    fn index_traversal_edge(
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

    #[must_use]
    pub(crate) fn has_stationary_occupant_at(&self, position: VoxelCoord) -> bool {
        self.stockpile_locations
            .values()
            .any(|value| *value == position)
            || self
                .equipment_locations
                .values()
                .any(|value| *value == position)
            || self
                .energy_store_locations
                .values()
                .any(|value| *value == position)
            || self
                .fluid_store_locations
                .values()
                .any(|value| *value == position)
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

    pub(crate) fn apply_player_respawn(
        &mut self,
        expected_revision: u64,
        next_revision: u64,
        expected_player: PlayerLogisticsRecord,
        respawn_body: PlayerBodyState,
        carried_stockpile: StockpileId,
        death_position: VoxelCoord,
        dropped_equipment: &[EquipmentId],
    ) {
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
        for equipment in dropped_equipment {
            assert!(
                !self.equipment_locations.contains_key(equipment),
                "carried equipment cannot already have a stationary location"
            );
            let previous = self.equipment_locations.insert(*equipment, death_position);
            assert!(previous.is_none());
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

    pub(super) fn apply_equipment_pickup(
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
        let insertion = match self.carried_equipment.binary_search(&equipment) {
            Ok(_) => panic!("validated equipment pickup cannot duplicate carried custody"),
            Err(index) => index,
        };
        self.carried_equipment.insert(insertion, equipment);
        self.revision = next_revision;
    }

    pub(super) fn apply_quick_access_binding(
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

    pub(super) fn apply_active_quick_access_slot(
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

    pub(super) fn apply_equipment_drop(
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
        self.traversable_edges = traversable_edges;
        self.traversal_neighbors.clear();
        for edge in &self.traversable_edges {
            Self::index_traversal_edge(&mut self.traversal_neighbors, *edge);
        }
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

#[cfg(test)]
#[path = "traversal_index_tests.rs"]
mod traversal_index_tests;
