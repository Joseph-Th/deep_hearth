//! Persistent-state validation for inventory; this child audits private owner data without exposing mutation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStateError,
};
use crate::structural::StructuralElementId;

use super::{
    InventoryState, MaterialLotId, MaterialLotRecord, StockpileId, StockpileLotIndex,
    StockpileStorageProfileError,
};

mod indexes;
mod lot;
mod stockpile;

use indexes::{validate_inventory_cursors, validate_lot_indexes, validate_stockpile_support_index};
use lot::validate_material_lot;
use stockpile::{validate_stockpile_record, validate_storage_profiles};

#[derive(Default)]
struct StockpileLotTotals {
    total: Mass,
    by_commodity: BTreeMap<CommodityKey, Mass>,
}

impl StockpileLotTotals {
    fn add_lot(&mut self, lot: &MaterialLotRecord) -> Result<(), InventoryValidationError> {
        self.total =
            self.total
                .checked_add(lot.mass)
                .ok_or(InventoryValidationError::MassOverflow {
                    stockpile: lot.stockpile,
                })?;
        let commodity = lot.commodity();
        let current = self
            .by_commodity
            .get(&commodity)
            .copied()
            .unwrap_or(Mass::ZERO);
        let next = current
            .checked_add(lot.mass)
            .ok_or(InventoryValidationError::MassOverflow {
                stockpile: lot.stockpile,
            })?;
        self.by_commodity.insert(commodity, next);
        Ok(())
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
    UnsupportedLotCommodity {
        lot: MaterialLotId,
        commodity: CommodityKey,
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
    LotProvenanceInFuture {
        lot: MaterialLotId,
        latest: SimulationTick,
        current: SimulationTick,
    },
    LotStorageTransitionBeforeCreation {
        lot: MaterialLotId,
        transition: SimulationTick,
        created: SimulationTick,
    },
    LotStorageTransitionInFuture {
        lot: MaterialLotId,
        transition: SimulationTick,
        current: SimulationTick,
    },
    LotStorageAgeOverflow {
        lot: MaterialLotId,
    },
    MissingLotOwner {
        lot: MaterialLotId,
        stockpile: StockpileId,
    },
    LotIndexMismatch {
        stockpile: StockpileId,
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
            Self::UnsupportedLotCommodity { lot, commodity } => write!(
                formatter,
                "material lot {} uses unauthored material {} form {}",
                lot.value(),
                commodity.material().value(),
                commodity.form().value()
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
            Self::LotProvenanceInFuture {
                lot,
                latest,
                current,
            } => write!(
                formatter,
                "material lot {} provenance reaches tick {} after current tick {}",
                lot.value(),
                latest.value(),
                current.value()
            ),
            Self::LotStorageTransitionBeforeCreation {
                lot,
                transition,
                created,
            } => write!(
                formatter,
                "material lot {} storage history transitions at tick {} before creation tick {}",
                lot.value(),
                transition.value(),
                created.value()
            ),
            Self::LotStorageTransitionInFuture {
                lot,
                transition,
                current,
            } => write!(
                formatter,
                "material lot {} storage history transitions at tick {} after current tick {}",
                lot.value(),
                transition.value(),
                current.value()
            ),
            Self::LotStorageAgeOverflow { lot } => write!(
                formatter,
                "material lot {} storage-age projection exceeds authoritative range",
                lot.value()
            ),
            Self::MissingLotOwner { lot, stockpile } => write!(
                formatter,
                "material lot {} references missing owner stockpile {}",
                lot.value(),
                stockpile.value()
            ),
            Self::LotIndexMismatch { stockpile } => write!(
                formatter,
                "stockpile {} derived lot index disagrees with authoritative lot ownership or commodity identity",
                stockpile.value()
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
    current_tick: SimulationTick,
) -> Result<(), InventoryValidationError> {
    validate_inventory_cursors(state)?;
    validate_storage_profiles(state)?;

    let mut expected_lot_indexes = BTreeMap::<StockpileId, StockpileLotIndex>::new();
    let mut calculated_by_stockpile = BTreeMap::<StockpileId, StockpileLotTotals>::new();
    for (key, lot) in &state.lots {
        validate_material_lot(materials, state, *key, lot, current_tick)?;
        expected_lot_indexes
            .entry(lot.stockpile)
            .or_default()
            .insert(*key, lot.commodity());
        calculated_by_stockpile
            .entry(lot.stockpile)
            .or_default()
            .add_lot(lot)?;
    }

    for (key, record) in &state.stockpiles {
        let calculated = calculated_by_stockpile.remove(key).unwrap_or_default();
        validate_stockpile_record(state, *key, record, calculated)?;
    }
    validate_lot_indexes(state, expected_lot_indexes)?;
    validate_stockpile_support_index(state)?;
    debug_assert!(calculated_by_stockpile.is_empty());
    Ok(())
}
