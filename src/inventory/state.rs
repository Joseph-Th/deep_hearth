//! Inventory records and private synchronized collection ownership; child validation audits derived state.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, MaterialComposition, MaterialPhase, ParticleSizeDistribution, ParticleSizeRange,
};
use crate::structural::StructuralElementId;

mod lot_mutation;

pub(super) use lot_mutation::{
    LotSlice, apply_aggregate_deposit, apply_aggregate_withdraw, apply_consume_lot_slice,
    apply_insert_or_merge_new_lot, apply_move_full_lot, apply_split_lot,
    get_stockpile_mut_or_panic,
};

/// Persistent identifier for a runtime stockpile record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StockpileId(u32);

impl StockpileId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "stockpile id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Physical containment envelope for one stockpile's directly owned material lots.
///
/// This is intentionally explicit runtime state rather than an implicit property of the UI label
/// "stockpile". A dry pile may hold hot or cold solids, while a crucible-like store can explicitly
/// admit liquid matter up to an authored thermal limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockpileStorageProfile {
    can_store_solid: bool,
    can_store_liquid: bool,
    maximum_temperature: Temperature,
    preservation_multiplier_ppm: u32,
}

impl StockpileStorageProfile {
    /// Builds a validated material-containment envelope.
    pub fn new(
        can_store_solid: bool,
        can_store_liquid: bool,
        maximum_temperature: Temperature,
    ) -> Result<Self, StockpileStorageProfileError> {
        let profile = Self {
            can_store_solid,
            can_store_liquid,
            maximum_temperature,
            preservation_multiplier_ppm: 1_000_000,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Builds containment with an explicit multiplier applied to authored perishability lifetimes.
    /// One million ppm is ambient storage; larger values extend shelf life.
    pub fn with_preservation(
        can_store_solid: bool,
        can_store_liquid: bool,
        maximum_temperature: Temperature,
        preservation_multiplier_ppm: u32,
    ) -> Result<Self, StockpileStorageProfileError> {
        let profile = Self {
            can_store_solid,
            can_store_liquid,
            maximum_temperature,
            preservation_multiplier_ppm,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Unbounded-temperature containment for dry storage that accepts solid matter only.
    #[must_use]
    pub const fn solid_only() -> Self {
        Self {
            can_store_solid: true,
            can_store_liquid: false,
            maximum_temperature: Temperature::from_millikelvin(u32::MAX),
            preservation_multiplier_ppm: 1_000_000,
        }
    }

    #[must_use]
    pub const fn can_store_phase(self, phase: MaterialPhase) -> bool {
        match phase {
            MaterialPhase::Solid => self.can_store_solid,
            MaterialPhase::Liquid => self.can_store_liquid,
        }
    }

    #[must_use]
    pub const fn maximum_temperature(self) -> Temperature {
        self.maximum_temperature
    }

    /// Returns the multiplier applied to food or other perishable shelf-life definitions.
    #[must_use]
    pub const fn preservation_multiplier_ppm(self) -> u32 {
        self.preservation_multiplier_ppm
    }

    pub(crate) fn validate(self) -> Result<(), StockpileStorageProfileError> {
        if !self.can_store_solid && !self.can_store_liquid {
            return Err(StockpileStorageProfileError::NoAcceptedPhase);
        }
        if self.maximum_temperature.millikelvin() == 0 {
            return Err(StockpileStorageProfileError::ZeroMaximumTemperature);
        }
        if self.preservation_multiplier_ppm == 0 {
            return Err(StockpileStorageProfileError::ZeroPreservationMultiplier);
        }
        Ok(())
    }
}

/// Invalid stockpile containment envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockpileStorageProfileError {
    NoAcceptedPhase,
    ZeroMaximumTemperature,
    ZeroPreservationMultiplier,
}

impl Display for StockpileStorageProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAcceptedPhase => formatter
                .write_str("stockpile storage profile must accept at least one material phase"),
            Self::ZeroMaximumTemperature => formatter.write_str(
                "stockpile storage profile maximum temperature must be above absolute zero",
            ),
            Self::ZeroPreservationMultiplier => {
                formatter.write_str("stockpile preservation multiplier must be nonzero")
            }
        }
    }
}

impl Error for StockpileStorageProfileError {}

/// Physical/provenance snapshot of one material slice consumed by an in-flight operation.
///
/// Source lot identity is deliberately not retained: a fully consumed lot may cease to exist.
/// The trace is historical evidence, not an ownership reference and not a second matter owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedMaterialTrace {
    pub(super) mass: Mass,
    pub(super) profile: MaterialLotProfile,
    pub(super) provenance: MaterialLotProvenance,
}

impl ConsumedMaterialTrace {
    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn profile(&self) -> &MaterialLotProfile {
        &self.profile
    }

    #[must_use]
    pub const fn provenance(&self) -> MaterialLotProvenance {
        self.provenance
    }
}

/// Persistent identifier for one homogeneous runtime material lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialLotId(u64);

impl MaterialLotId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        assert!(value != 0, "material lot id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Runtime properties that determine whether two newly created lots are fungible.
///
/// Every behaviorally meaningful per-lot property belongs here. Compaction compares this profile
/// by value, so adding a future field such as freshness or treatment state automatically makes it
/// part of lot fungibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialLotProfile {
    pub(super) commodity: CommodityKey,
    pub(super) temperature: Temperature,
    pub(super) composition: MaterialComposition,
    pub(super) particle_size: Option<ParticleSizeDistribution>,
}

impl MaterialLotProfile {
    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.commodity
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.composition
    }

    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.particle_size
            .as_ref()
            .map(ParticleSizeDistribution::envelope)
    }

    /// Returns the authoritative weighted particulate profile, if present.
    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.particle_size.as_ref()
    }
}

/// Provenance range retained when compatible newly created matter coalesces into an existing lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialLotProvenance {
    pub(super) earliest_created_at: SimulationTick,
    pub(super) latest_created_at: SimulationTick,
}

impl MaterialLotProvenance {
    #[must_use]
    pub const fn earliest_created_at(self) -> SimulationTick {
        self.earliest_created_at
    }

    #[must_use]
    pub const fn latest_created_at(self) -> SimulationTick {
        self.latest_created_at
    }
}

/// One homogeneous batch of matter whose local runtime properties must remain distinguishable.
///
/// Lots are the authoritative source for matter identity, mass, thermal state, and ownership.
/// Stockpile commodity totals and lot-ID collections are derived indexes maintained atomically by
/// the inventory transaction module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialLotRecord {
    pub(super) id: MaterialLotId,
    pub(super) stockpile: StockpileId,
    pub(super) mass: Mass,
    pub(super) profile: MaterialLotProfile,
    pub(super) provenance: MaterialLotProvenance,
}

impl MaterialLotRecord {
    #[must_use]
    pub const fn id(&self) -> MaterialLotId {
        self.id
    }

    #[must_use]
    pub const fn stockpile(&self) -> StockpileId {
        self.stockpile
    }

    #[must_use]
    pub const fn commodity(&self) -> CommodityKey {
        self.profile.commodity
    }

    #[must_use]
    pub const fn mass(&self) -> Mass {
        self.mass
    }

    #[must_use]
    pub const fn temperature(&self) -> Temperature {
        self.profile.temperature
    }

    #[must_use]
    pub const fn composition(&self) -> &MaterialComposition {
        &self.profile.composition
    }

    #[must_use]
    pub fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.profile.particle_size()
    }

    /// Returns the authoritative weighted particulate profile, if present.
    #[must_use]
    pub const fn particle_size_distribution(&self) -> Option<&ParticleSizeDistribution> {
        self.profile.particle_size_distribution()
    }

    #[must_use]
    pub const fn created_at(&self) -> SimulationTick {
        self.provenance.earliest_created_at
    }

    /// Returns the latest creation tick represented after compatible new matter was coalesced.
    #[must_use]
    pub const fn latest_created_at(&self) -> SimulationTick {
        self.provenance.latest_created_at
    }
}

/// One capacity-constrained aggregate store for fungible material mass.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockpileRecord {
    pub(super) id: StockpileId,
    pub(super) capacity: Mass,
    pub(super) storage_profile: StockpileStorageProfile,
    pub(super) supported_by: Option<StructuralElementId>,
    pub(super) stored_mass: Mass,
    pub(super) reserved_inbound: Mass,
    pub(super) lot_ids: BTreeSet<MaterialLotId>,
    pub(super) contents: BTreeMap<CommodityKey, Mass>,
}

impl StockpileRecord {
    #[must_use]
    pub const fn id(&self) -> StockpileId {
        self.id
    }

    #[must_use]
    pub const fn capacity(&self) -> Mass {
        self.capacity
    }

    #[must_use]
    pub const fn storage_profile(&self) -> StockpileStorageProfile {
        self.storage_profile
    }

    /// Returns the structural member currently carrying this stockpile's stored matter, if assigned.
    #[must_use]
    pub const fn supported_by(&self) -> Option<StructuralElementId> {
        self.supported_by
    }

    #[must_use]
    pub const fn stored_mass(&self) -> Mass {
        self.stored_mass
    }

    #[must_use]
    pub const fn reserved_inbound(&self) -> Mass {
        self.reserved_inbound
    }

    /// Iterates owned lot IDs in stable persistent-ID order.
    pub fn lot_ids(&self) -> impl Iterator<Item = MaterialLotId> + '_ {
        self.lot_ids.iter().copied()
    }

    /// Returns currently stored mass for one exact material/form key.
    #[must_use]
    pub fn get_mass(&self, commodity: CommodityKey) -> Mass {
        self.contents.get(&commodity).copied().unwrap_or(Mass::ZERO)
    }

    /// Iterates stock deterministically in material/form key order.
    pub fn contents(&self) -> impl Iterator<Item = (CommodityKey, Mass)> + '_ {
        self.contents.iter().map(|(key, mass)| (*key, *mass))
    }
}

/// Runtime owner for stockpile records and their generated identifiers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InventoryState {
    revision: u64,
    next_stockpile_id: u32,
    next_lot_id: u64,
    stockpiles: BTreeMap<StockpileId, StockpileRecord>,
    lots: BTreeMap<MaterialLotId, MaterialLotRecord>,
    stockpiles_by_support: BTreeMap<StructuralElementId, BTreeSet<StockpileId>>,
}

impl InventoryState {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            revision: 0,
            next_stockpile_id: 1,
            next_lot_id: 1,
            stockpiles: BTreeMap::new(),
            lots: BTreeMap::new(),
            stockpiles_by_support: BTreeMap::new(),
        }
    }

    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) const fn next_stockpile_id(&self) -> u32 {
        self.next_stockpile_id
    }

    pub(super) const fn next_lot_id(&self) -> u64 {
        self.next_lot_id
    }

    pub(super) fn insert_stockpile(
        &mut self,
        record: StockpileRecord,
        next_stockpile_id: u32,
        next_revision: u64,
    ) {
        let id = record.id;
        let replaced = self.stockpiles.insert(id, record);
        assert!(
            replaced.is_none(),
            "validated stockpile ID must be globally unique"
        );
        self.next_stockpile_id = next_stockpile_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_lot_cursor_and_revision(&mut self, next_lot_id: u64, next_revision: u64) {
        self.next_lot_id = next_lot_id;
        self.revision = next_revision;
    }

    pub(super) fn apply_revision(&mut self, next_revision: u64) {
        self.revision = next_revision;
    }

    pub(crate) const fn has_valid_id_cursors(&self) -> bool {
        self.next_stockpile_id != 0 && self.next_lot_id != 0
    }

    /// Returns one stockpile by stable runtime ID.
    #[must_use]
    pub fn get_stockpile(&self, id: StockpileId) -> Option<&StockpileRecord> {
        self.stockpiles.get(&id)
    }

    /// Iterates stockpiles deterministically by stable runtime ID.
    pub fn stockpiles(&self) -> impl Iterator<Item = &StockpileRecord> {
        self.stockpiles.values()
    }

    /// Returns one homogeneous material lot by stable runtime ID.
    #[must_use]
    pub fn get_lot(&self, id: MaterialLotId) -> Option<&MaterialLotRecord> {
        self.lots.get(&id)
    }

    /// Iterates all material lots deterministically by stable runtime ID.
    pub fn lots(&self) -> impl Iterator<Item = &MaterialLotRecord> {
        self.lots.values()
    }

    /// Iterates stockpiles assigned to one structural support in stable stockpile-ID order.
    pub(crate) fn supported_stockpiles(
        &self,
        support: StructuralElementId,
    ) -> impl Iterator<Item = StockpileId> + '_ {
        self.stockpiles_by_support
            .get(&support)
            .into_iter()
            .flat_map(|stockpiles| stockpiles.iter().copied())
    }

    pub(super) fn apply_support_change(
        &mut self,
        stockpile: StockpileId,
        before: Option<StructuralElementId>,
        after: Option<StructuralElementId>,
        next_revision: u64,
    ) {
        if let Some(before) = before {
            let remove_entry = {
                let indexed = match self.stockpiles_by_support.get_mut(&before) {
                    Some(indexed) => indexed,
                    None => panic!(
                        "runtime invariant broken: inventory support index missing element {} for stockpile {}",
                        before.value(),
                        stockpile.value()
                    ),
                };
                assert!(
                    indexed.remove(&stockpile),
                    "runtime invariant broken: inventory support index element {} missing stockpile {}",
                    before.value(),
                    stockpile.value()
                );
                indexed.is_empty()
            };
            if remove_entry {
                self.stockpiles_by_support.remove(&before);
            }
        }
        if let Some(after) = after {
            let inserted = self
                .stockpiles_by_support
                .entry(after)
                .or_default()
                .insert(stockpile);
            assert!(
                inserted,
                "runtime invariant broken: inventory support index element {} already contains stockpile {}",
                after.value(),
                stockpile.value()
            );
        }
        let record = match self.stockpiles.get_mut(&stockpile) {
            Some(record) => record,
            None => panic!(
                "runtime invariant broken: stockpile {} disappeared during support update",
                stockpile.value()
            ),
        };
        debug_assert_eq!(record.supported_by, before);
        record.supported_by = after;
        self.revision = next_revision;
    }
}

mod validation;

pub use validation::InventoryValidationError;
pub(crate) use validation::validate_loaded_inventory;
