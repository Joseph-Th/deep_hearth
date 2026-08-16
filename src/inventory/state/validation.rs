//! Persistent-state validation for inventory; this child audits private owner data without exposing mutation.

use super::*;

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
