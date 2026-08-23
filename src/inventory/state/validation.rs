//! Persistent-state validation for inventory; this child audits private owner data without exposing mutation.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::core::quantity::{Mass, Temperature};
use crate::core::time::SimulationTick;
use crate::material::{
    CommodityKey, CompositionError, FormId, MaterialPhase, MaterialPhaseStateError,
    MaterialRegistry, ParticleSizeStateError, validate_material_particle_size_state,
    validate_material_phase_state,
};
use crate::structural::StructuralElementId;

use super::{
    InventoryState, MaterialLotId, StockpileId, StockpileLotIndex, StockpileStorageProfileError,
};

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

    let mut expected_lot_indexes = BTreeMap::<StockpileId, StockpileLotIndex>::new();
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
        if materials.get_material(lot.commodity().material()).is_some()
            && materials.get_form(lot.commodity().form()).is_some()
            && !materials.has_commodity(lot.commodity())
        {
            return Err(InventoryValidationError::UnsupportedLotCommodity {
                lot: *key,
                commodity: lot.commodity(),
            });
        }
        validate_material_phase_state(
            materials,
            lot.commodity(),
            lot.composition(),
            lot.temperature(),
        )
        .map_err(|error| InventoryValidationError::InvalidLotPhaseState { lot: *key, error })?;
        validate_material_particle_size_state(
            materials,
            lot.commodity(),
            lot.particle_size_distribution(),
        )
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
        if lot.latest_created_at() > current_tick {
            return Err(InventoryValidationError::LotProvenanceInFuture {
                lot: *key,
                latest: lot.latest_created_at(),
                current: current_tick,
            });
        }
        let Some(owner) = state.stockpiles.get(&lot.stockpile) else {
            return Err(InventoryValidationError::MissingLotOwner {
                lot: *key,
                stockpile: lot.stockpile,
            });
        };
        let transition = lot.storage_history().last_transition_at();
        if transition < lot.created_at() {
            return Err(
                InventoryValidationError::LotStorageTransitionBeforeCreation {
                    lot: *key,
                    transition,
                    created: lot.created_at(),
                },
            );
        }
        if transition > current_tick {
            return Err(InventoryValidationError::LotStorageTransitionInFuture {
                lot: *key,
                transition,
                current: current_tick,
            });
        }
        if lot
            .storage_history()
            .project(
                current_tick,
                owner.storage_profile().preservation_multiplier_ppm(),
            )
            .is_none()
        {
            return Err(InventoryValidationError::LotStorageAgeOverflow { lot: *key });
        }
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
        expected_lot_indexes
            .entry(lot.stockpile)
            .or_default()
            .insert(*key, lot.commodity());

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
    if state.lot_indexes != expected_lot_indexes {
        let stockpile = match state
            .lot_indexes
            .keys()
            .chain(expected_lot_indexes.keys())
            .find(|stockpile| {
                state.lot_indexes.get(stockpile) != expected_lot_indexes.get(stockpile)
            })
            .copied()
        {
            Some(stockpile) => stockpile,
            None => panic!("unequal lot-index maps must have a differing key"),
        };
        return Err(InventoryValidationError::LotIndexMismatch { stockpile });
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
