//! Inventory records and derived-data validation; sibling transaction code is their only mutation path.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialComposition, MaterialPhase,
    MaterialPhaseStateError, MaterialRegistry, ParticleSizeRange, ParticleSizeStateError,
    validate_material_particle_size_state, validate_material_phase_state,
};
use crate::structural::StructuralElementId;

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
    #[serde(rename = "allow_solid")]
    can_store_solid: bool,
    #[serde(rename = "allow_liquid")]
    can_store_liquid: bool,
    maximum_temperature: Temperature,
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
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Default loose/general solid storage used by the legacy stockpile allocation convenience API.
    #[must_use]
    pub const fn solid_only() -> Self {
        Self {
            can_store_solid: true,
            can_store_liquid: false,
            maximum_temperature: Temperature::from_millikelvin(u32::MAX),
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

    pub(crate) fn validate(self) -> Result<(), StockpileStorageProfileError> {
        if !self.can_store_solid && !self.can_store_liquid {
            return Err(StockpileStorageProfileError::NoAcceptedPhase);
        }
        if self.maximum_temperature.millikelvin() == 0 {
            return Err(StockpileStorageProfileError::ZeroMaximumTemperature);
        }
        Ok(())
    }
}

/// Invalid stockpile containment envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockpileStorageProfileError {
    NoAcceptedPhase,
    ZeroMaximumTemperature,
}

impl Display for StockpileStorageProfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAcceptedPhase => formatter
                .write_str("stockpile storage profile must accept at least one material phase"),
            Self::ZeroMaximumTemperature => formatter.write_str(
                "stockpile storage profile maximum temperature must be above absolute zero",
            ),
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
    pub(super) particle_size: Option<ParticleSizeRange>,
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
    pub const fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.particle_size
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
    pub const fn particle_size(&self) -> Option<ParticleSizeRange> {
        self.profile.particle_size
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
    pub(super) revision: u64,
    pub(super) next_stockpile_id: u32,
    pub(super) next_lot_id: u64,
    pub(super) stockpiles: BTreeMap<StockpileId, StockpileRecord>,
    pub(super) lots: BTreeMap<MaterialLotId, MaterialLotRecord>,
    pub(super) stockpiles_by_support: BTreeMap<StructuralElementId, BTreeSet<StockpileId>>,
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

    pub(crate) fn has_valid_support_index(&self) -> bool {
        let records_match_index =
            self.stockpiles
                .values()
                .all(|record| match record.supported_by {
                    Some(support) => self
                        .stockpiles_by_support
                        .get(&support)
                        .is_some_and(|stockpiles| stockpiles.contains(&record.id)),
                    None => true,
                });
        let index_matches_records =
            self.stockpiles_by_support
                .iter()
                .all(|(support, stockpiles)| {
                    support.value() != 0
                        && !stockpiles.is_empty()
                        && stockpiles.iter().all(|id| {
                            id.value() != 0
                                && self
                                    .stockpiles
                                    .get(id)
                                    .is_some_and(|record| record.supported_by == Some(*support))
                        })
                });
        records_match_index && index_matches_records
    }
}

/// Persistent-state validation failure for the inventory owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryValidationError {
    ZeroNextStockpileId,
    ZeroNextLotId,
    ZeroStockpileId,
    ZeroLotId,
    NextIdNotAfterExisting {
        next: u32,
        highest: StockpileId,
    },
    NextLotIdNotAfterExisting {
        next: u64,
        highest: MaterialLotId,
    },
    IdMismatch {
        key: StockpileId,
        record: StockpileId,
    },
    ZeroCapacity {
        stockpile: StockpileId,
    },
    InvalidStorageProfile {
        stockpile: StockpileId,
        error: StockpileStorageProfileError,
    },
    ZeroCommodityMass {
        stockpile: StockpileId,
        commodity: CommodityKey,
    },
    UnknownLotForm {
        lot: MaterialLotId,
        form: FormId,
    },
    LotPhaseNotAccepted {
        lot: MaterialLotId,
        stockpile: StockpileId,
        phase: MaterialPhase,
    },
    LotTemperatureExceedsStorageMaximum {
        lot: MaterialLotId,
        stockpile: StockpileId,
        temperature: Temperature,
        maximum: Temperature,
    },
    LotIdMismatch {
        key: MaterialLotId,
        record: MaterialLotId,
    },
    ZeroLotMass {
        lot: MaterialLotId,
    },
    InvalidLotComposition {
        lot: MaterialLotId,
        error: CompositionError,
    },
    LotCompositionMissingHost {
        lot: MaterialLotId,
        host: crate::material::MaterialId,
    },
    InvalidLotPhaseState {
        lot: MaterialLotId,
        error: MaterialPhaseStateError,
    },
    InvalidLotParticleSizeState {
        lot: MaterialLotId,
        error: ParticleSizeStateError,
    },
    InvalidLotProvenanceRange {
        lot: MaterialLotId,
        earliest: SimulationTick,
        latest: SimulationTick,
    },
    MissingLotOwner {
        lot: MaterialLotId,
        stockpile: StockpileId,
    },
    LotMissingFromOwnerIndex {
        lot: MaterialLotId,
        stockpile: StockpileId,
    },
    UnknownIndexedLot {
        stockpile: StockpileId,
        lot: MaterialLotId,
    },
    IndexedLotOwnedElsewhere {
        stockpile: StockpileId,
        lot: MaterialLotId,
        actual_owner: StockpileId,
    },
    CommodityMassMismatch {
        stockpile: StockpileId,
        commodity: CommodityKey,
        cached: Mass,
        calculated: Mass,
    },
    StoredMassMismatch {
        stockpile: StockpileId,
        cached: Mass,
        calculated: Mass,
    },
    CapacityExceeded {
        stockpile: StockpileId,
    },
    MassOverflow {
        stockpile: StockpileId,
    },
    ZeroSupportElementId {
        stockpile: StockpileId,
    },
    ZeroIndexedSupportElementId,
    EmptySupportIndex {
        element: StructuralElementId,
    },
    MissingSupportIndex {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    UnknownIndexedStockpile {
        stockpile: StockpileId,
        element: StructuralElementId,
    },
    SupportIndexMismatch {
        stockpile: StockpileId,
        indexed: StructuralElementId,
        actual: Option<StructuralElementId>,
    },
}

impl Display for InventoryValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroNextStockpileId => formatter.write_str("next stockpile id must not be zero"),
            Self::ZeroNextLotId => formatter.write_str("next material lot id must not be zero"),
            Self::ZeroStockpileId => formatter.write_str("stockpile id must not be zero"),
            Self::ZeroLotId => formatter.write_str("material lot id must not be zero"),
            Self::NextIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next stockpile id {next} is not after existing id {}",
                highest.value()
            ),
            Self::NextLotIdNotAfterExisting { next, highest } => write!(
                formatter,
                "next material lot id {next} is not after existing id {}",
                highest.value()
            ),
            Self::IdMismatch { key, record } => write!(
                formatter,
                "stockpile map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroCapacity { stockpile } => {
                write!(
                    formatter,
                    "stockpile {} has zero capacity",
                    stockpile.value()
                )
            }
            Self::InvalidStorageProfile { stockpile, error } => write!(
                formatter,
                "stockpile {} has invalid storage profile: {error}",
                stockpile.value()
            ),
            Self::ZeroCommodityMass {
                stockpile,
                commodity,
            } => write!(
                formatter,
                "stockpile {} contains zero mass for material {} form {}",
                stockpile.value(),
                commodity.material().value(),
                commodity.form().value()
            ),
            Self::UnknownLotForm { lot, form } => write!(
                formatter,
                "material lot {} references unknown form {}",
                lot.value(),
                form.value()
            ),
            Self::LotPhaseNotAccepted {
                lot,
                stockpile,
                phase,
            } => write!(
                formatter,
                "material lot {} is {phase:?} but stockpile {} does not accept that phase",
                lot.value(),
                stockpile.value()
            ),
            Self::LotTemperatureExceedsStorageMaximum {
                lot,
                stockpile,
                temperature,
                maximum,
            } => write!(
                formatter,
                "material lot {} temperature {} mK exceeds stockpile {} maximum {} mK",
                lot.value(),
                temperature.millikelvin(),
                stockpile.value(),
                maximum.millikelvin()
            ),
            Self::LotIdMismatch { key, record } => write!(
                formatter,
                "material lot map key {} disagrees with record id {}",
                key.value(),
                record.value()
            ),
            Self::ZeroLotMass { lot } => {
                write!(formatter, "material lot {} has zero mass", lot.value())
            }
            Self::InvalidLotComposition { lot, error } => write!(
                formatter,
                "material lot {} has invalid composition: {error}",
                lot.value()
            ),
            Self::LotCompositionMissingHost { lot, host } => write!(
                formatter,
                "material lot {} composition omits host material {}",
                lot.value(),
                host.value()
            ),
            Self::InvalidLotPhaseState { lot, error } => write!(
                formatter,
                "material lot {} has invalid phase state: {error}",
                lot.value()
            ),
            Self::InvalidLotParticleSizeState { lot, error } => write!(
                formatter,
                "material lot {} has invalid particle-size state: {error}",
                lot.value()
            ),
            Self::InvalidLotProvenanceRange {
                lot,
                earliest,
                latest,
            } => write!(
                formatter,
                "material lot {} provenance range {}..={} is invalid",
                lot.value(),
                earliest.value(),
                latest.value()
            ),
            Self::MissingLotOwner { lot, stockpile } => write!(
                formatter,
                "material lot {} references missing owner stockpile {}",
                lot.value(),
                stockpile.value()
            ),
            Self::LotMissingFromOwnerIndex { lot, stockpile } => write!(
                formatter,
                "material lot {} is absent from owner stockpile {} lot index",
                lot.value(),
                stockpile.value()
            ),
            Self::UnknownIndexedLot { stockpile, lot } => write!(
                formatter,
                "stockpile {} indexes missing material lot {}",
                stockpile.value(),
                lot.value()
            ),
            Self::IndexedLotOwnedElsewhere {
                stockpile,
                lot,
                actual_owner,
            } => write!(
                formatter,
                "stockpile {} indexes material lot {} owned by stockpile {}",
                stockpile.value(),
                lot.value(),
                actual_owner.value()
            ),
            Self::CommodityMassMismatch {
                stockpile,
                commodity,
                cached,
                calculated,
            } => write!(
                formatter,
                "stockpile {} cached material {} form {} mass {} mg disagrees with lot total {} mg",
                stockpile.value(),
                commodity.material().value(),
                commodity.form().value(),
                cached.milligrams(),
                calculated.milligrams()
            ),
            Self::StoredMassMismatch {
                stockpile,
                cached,
                calculated,
            } => write!(
                formatter,
                "stockpile {} cached mass {} mg disagrees with calculated mass {} mg",
                stockpile.value(),
                cached.milligrams(),
                calculated.milligrams()
            ),
            Self::CapacityExceeded { stockpile } => write!(
                formatter,
                "stockpile {} stored plus reserved mass exceeds capacity",
                stockpile.value()
            ),
            Self::MassOverflow { stockpile } => write!(
                formatter,
                "stockpile {} mass accounting overflows",
                stockpile.value()
            ),
            Self::ZeroSupportElementId { stockpile } => write!(
                formatter,
                "stockpile {} references zero structural support id",
                stockpile.value()
            ),
            Self::ZeroIndexedSupportElementId => {
                formatter.write_str("inventory support index contains zero structural element id")
            }
            Self::EmptySupportIndex { element } => write!(
                formatter,
                "inventory support index element {} contains no stockpiles",
                element.value()
            ),
            Self::MissingSupportIndex { stockpile, element } => write!(
                formatter,
                "stockpile {} references structural support {} but is absent from its reverse index",
                stockpile.value(),
                element.value()
            ),
            Self::UnknownIndexedStockpile { stockpile, element } => write!(
                formatter,
                "inventory support index element {} references missing stockpile {}",
                element.value(),
                stockpile.value()
            ),
            Self::SupportIndexMismatch {
                stockpile,
                indexed,
                actual,
            } => write!(
                formatter,
                "inventory support index assigns stockpile {} to element {} but record support is {actual:?}",
                stockpile.value(),
                indexed.value()
            ),
        }
    }
}

impl Error for InventoryValidationError {}

pub(crate) fn validate_loaded_inventory(
    materials: &MaterialRegistry,
    state: &InventoryState,
) -> Result<(), InventoryValidationError> {
    if state.next_stockpile_id == 0 {
        return Err(InventoryValidationError::ZeroNextStockpileId);
    }
    if state.next_lot_id == 0 {
        return Err(InventoryValidationError::ZeroNextLotId);
    }
    if let Some(highest) = state.stockpiles.keys().next_back().copied()
        && state.next_stockpile_id <= highest.value()
    {
        return Err(InventoryValidationError::NextIdNotAfterExisting {
            next: state.next_stockpile_id,
            highest,
        });
    }

    if let Some(highest) = state.lots.keys().next_back().copied()
        && state.next_lot_id <= highest.value()
    {
        return Err(InventoryValidationError::NextLotIdNotAfterExisting {
            next: state.next_lot_id,
            highest,
        });
    }

    for (stockpile, record) in &state.stockpiles {
        record.storage_profile.validate().map_err(|error| {
            InventoryValidationError::InvalidStorageProfile {
                stockpile: *stockpile,
                error,
            }
        })?;
    }

    let mut calculated_by_stockpile =
        BTreeMap::<StockpileId, (Mass, BTreeMap<CommodityKey, Mass>)>::new();
    for (key, lot) in &state.lots {
        if key.value() == 0 || lot.id.value() == 0 {
            return Err(InventoryValidationError::ZeroLotId);
        }
        if *key != lot.id {
            return Err(InventoryValidationError::LotIdMismatch {
                key: *key,
                record: lot.id,
            });
        }
        if lot.mass.is_zero() {
            return Err(InventoryValidationError::ZeroLotMass { lot: *key });
        }
        lot.composition().validate().map_err(|error| {
            InventoryValidationError::InvalidLotComposition { lot: *key, error }
        })?;
        if lot
            .composition()
            .parts_per_million(lot.commodity().material())
            == 0
        {
            return Err(InventoryValidationError::LotCompositionMissingHost {
                lot: *key,
                host: lot.commodity().material(),
            });
        }
        validate_material_phase_state(
            materials,
            lot.commodity(),
            lot.composition(),
            lot.temperature(),
        )
        .map_err(|error| InventoryValidationError::InvalidLotPhaseState { lot: *key, error })?;
        validate_material_particle_size_state(materials, lot.commodity(), lot.particle_size())
            .map_err(
                |error| InventoryValidationError::InvalidLotParticleSizeState { lot: *key, error },
            )?;
        if lot.latest_created_at() < lot.created_at() {
            return Err(InventoryValidationError::InvalidLotProvenanceRange {
                lot: *key,
                earliest: lot.created_at(),
                latest: lot.latest_created_at(),
            });
        }
        let Some(owner) = state.stockpiles.get(&lot.stockpile) else {
            return Err(InventoryValidationError::MissingLotOwner {
                lot: *key,
                stockpile: lot.stockpile,
            });
        };
        let form_id = lot.commodity().form();
        let Some(form) = materials.get_form(form_id) else {
            return Err(InventoryValidationError::UnknownLotForm {
                lot: *key,
                form: form_id,
            });
        };
        if !owner.storage_profile.can_store_phase(form.phase()) {
            return Err(InventoryValidationError::LotPhaseNotAccepted {
                lot: *key,
                stockpile: lot.stockpile,
                phase: form.phase(),
            });
        }
        if lot.temperature() > owner.storage_profile.maximum_temperature() {
            return Err(
                InventoryValidationError::LotTemperatureExceedsStorageMaximum {
                    lot: *key,
                    stockpile: lot.stockpile,
                    temperature: lot.temperature(),
                    maximum: owner.storage_profile.maximum_temperature(),
                },
            );
        }
        if !owner.lot_ids.contains(key) {
            return Err(InventoryValidationError::LotMissingFromOwnerIndex {
                lot: *key,
                stockpile: lot.stockpile,
            });
        }

        let aggregate = calculated_by_stockpile
            .entry(lot.stockpile)
            .or_insert((Mass::ZERO, BTreeMap::new()));
        aggregate.0 =
            aggregate
                .0
                .checked_add(lot.mass)
                .ok_or(InventoryValidationError::MassOverflow {
                    stockpile: lot.stockpile,
                })?;
        let commodity_mass = aggregate
            .1
            .get(&lot.commodity())
            .copied()
            .unwrap_or(Mass::ZERO)
            .checked_add(lot.mass)
            .ok_or(InventoryValidationError::MassOverflow {
                stockpile: lot.stockpile,
            })?;
        aggregate.1.insert(lot.commodity(), commodity_mass);
    }

    for (key, record) in &state.stockpiles {
        if key.value() == 0 || record.id.value() == 0 {
            return Err(InventoryValidationError::ZeroStockpileId);
        }
        if *key != record.id {
            return Err(InventoryValidationError::IdMismatch {
                key: *key,
                record: record.id,
            });
        }
        if record.capacity.is_zero() {
            return Err(InventoryValidationError::ZeroCapacity { stockpile: *key });
        }
        if let Some(support) = record.supported_by {
            if support.value() == 0 {
                return Err(InventoryValidationError::ZeroSupportElementId { stockpile: *key });
            }
            if !state
                .stockpiles_by_support
                .get(&support)
                .is_some_and(|stockpiles| stockpiles.contains(key))
            {
                return Err(InventoryValidationError::MissingSupportIndex {
                    stockpile: *key,
                    element: support,
                });
            }
        }

        for lot_id in &record.lot_ids {
            let Some(lot) = state.lots.get(lot_id) else {
                return Err(InventoryValidationError::UnknownIndexedLot {
                    stockpile: *key,
                    lot: *lot_id,
                });
            };
            if lot.stockpile != *key {
                return Err(InventoryValidationError::IndexedLotOwnedElsewhere {
                    stockpile: *key,
                    lot: *lot_id,
                    actual_owner: lot.stockpile,
                });
            }
        }

        let (calculated, calculated_contents) = calculated_by_stockpile
            .remove(key)
            .unwrap_or((Mass::ZERO, BTreeMap::new()));
        for (commodity, mass) in &record.contents {
            if mass.is_zero() {
                return Err(InventoryValidationError::ZeroCommodityMass {
                    stockpile: *key,
                    commodity: *commodity,
                });
            }
            let lot_mass = calculated_contents
                .get(commodity)
                .copied()
                .unwrap_or(Mass::ZERO);
            if lot_mass != *mass {
                return Err(InventoryValidationError::CommodityMassMismatch {
                    stockpile: *key,
                    commodity: *commodity,
                    cached: *mass,
                    calculated: lot_mass,
                });
            }
        }
        for (commodity, lot_mass) in &calculated_contents {
            let cached = record
                .contents
                .get(commodity)
                .copied()
                .unwrap_or(Mass::ZERO);
            if cached != *lot_mass {
                return Err(InventoryValidationError::CommodityMassMismatch {
                    stockpile: *key,
                    commodity: *commodity,
                    cached,
                    calculated: *lot_mass,
                });
            }
        }
        if calculated != record.stored_mass {
            return Err(InventoryValidationError::StoredMassMismatch {
                stockpile: *key,
                cached: record.stored_mass,
                calculated,
            });
        }
        let committed = record
            .stored_mass
            .checked_add(record.reserved_inbound)
            .ok_or(InventoryValidationError::MassOverflow { stockpile: *key })?;
        if committed > record.capacity {
            return Err(InventoryValidationError::CapacityExceeded { stockpile: *key });
        }
    }
    for (element, stockpiles) in &state.stockpiles_by_support {
        if element.value() == 0 {
            return Err(InventoryValidationError::ZeroIndexedSupportElementId);
        }
        if stockpiles.is_empty() {
            return Err(InventoryValidationError::EmptySupportIndex { element: *element });
        }
        for stockpile in stockpiles {
            let Some(record) = state.stockpiles.get(stockpile) else {
                return Err(InventoryValidationError::UnknownIndexedStockpile {
                    stockpile: *stockpile,
                    element: *element,
                });
            };
            if record.supported_by != Some(*element) {
                return Err(InventoryValidationError::SupportIndexMismatch {
                    stockpile: *stockpile,
                    indexed: *element,
                    actual: record.supported_by,
                });
            }
        }
    }
    debug_assert!(calculated_by_stockpile.is_empty());
    Ok(())
}
