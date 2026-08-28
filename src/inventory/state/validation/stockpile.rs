//! Stockpile-local validation for storage policy, cached matter totals, support, and capacity.

use crate::core::quantity::Mass;

use super::super::{InventoryState, StockpileId, StockpileRecord};
use super::{InventoryValidationError, StockpileLotTotals};

pub(super) fn validate_storage_profiles(
    state: &InventoryState,
) -> Result<(), InventoryValidationError> {
    for (stockpile, record) in &state.stockpiles {
        record.storage_profile.validate().map_err(|error| {
            InventoryValidationError::InvalidStorageProfile {
                stockpile: *stockpile,
                error,
            }
        })?;
    }
    Ok(())
}

pub(super) fn validate_stockpile_record(
    state: &InventoryState,
    key: StockpileId,
    record: &StockpileRecord,
    calculated: StockpileLotTotals,
) -> Result<(), InventoryValidationError> {
    if key.value() == 0 || record.id.value() == 0 {
        return Err(InventoryValidationError::ZeroStockpileId);
    }
    if key != record.id {
        return Err(InventoryValidationError::IdMismatch {
            key,
            record: record.id,
        });
    }
    if record.capacity.is_zero() {
        return Err(InventoryValidationError::ZeroCapacity { stockpile: key });
    }
    validate_stockpile_support_reference(state, key, record)?;
    validate_stockpile_cached_contents(key, record, &calculated)?;
    if calculated.total != record.stored_mass {
        return Err(InventoryValidationError::StoredMassMismatch {
            stockpile: key,
            cached: record.stored_mass,
            calculated: calculated.total,
        });
    }
    let committed = record
        .stored_mass
        .checked_add(record.reserved_inbound)
        .ok_or(InventoryValidationError::MassOverflow { stockpile: key })?;
    if committed > record.capacity {
        return Err(InventoryValidationError::CapacityExceeded { stockpile: key });
    }
    Ok(())
}

fn validate_stockpile_support_reference(
    state: &InventoryState,
    key: StockpileId,
    record: &StockpileRecord,
) -> Result<(), InventoryValidationError> {
    let Some(support) = record.supported_by else {
        return Ok(());
    };
    if support.value() == 0 {
        return Err(InventoryValidationError::ZeroSupportElementId { stockpile: key });
    }
    if !state
        .stockpiles_by_support
        .get(&support)
        .is_some_and(|stockpiles| stockpiles.contains(&key))
    {
        return Err(InventoryValidationError::MissingSupportIndex {
            stockpile: key,
            element: support,
        });
    }
    Ok(())
}

fn validate_cached_entry(
    key: StockpileId,
    calculated: &StockpileLotTotals,
    commodity: crate::material::CommodityKey,
    cached: Mass,
) -> Result<(), InventoryValidationError> {
    if cached.is_zero() {
        return Err(InventoryValidationError::ZeroCommodityMass {
            stockpile: key,
            commodity,
        });
    }
    let calculated_mass = calculated
        .by_commodity
        .get(&commodity)
        .copied()
        .unwrap_or(Mass::ZERO);
    if calculated_mass != cached {
        return Err(InventoryValidationError::CommodityMassMismatch {
            stockpile: key,
            commodity,
            cached,
            calculated: calculated_mass,
        });
    }
    Ok(())
}

fn validate_calculated_entry(
    key: StockpileId,
    record: &StockpileRecord,
    commodity: crate::material::CommodityKey,
    calculated: Mass,
) -> Result<(), InventoryValidationError> {
    let cached = record
        .contents
        .get(&commodity)
        .copied()
        .unwrap_or(Mass::ZERO);
    if cached != calculated {
        return Err(InventoryValidationError::CommodityMassMismatch {
            stockpile: key,
            commodity,
            cached,
            calculated,
        });
    }
    Ok(())
}

fn validate_stockpile_cached_contents(
    key: StockpileId,
    record: &StockpileRecord,
    calculated: &StockpileLotTotals,
) -> Result<(), InventoryValidationError> {
    for (commodity, mass) in &record.contents {
        validate_cached_entry(key, calculated, *commodity, *mass)?;
    }
    for (commodity, lot_mass) in &calculated.by_commodity {
        validate_calculated_entry(key, record, *commodity, *lot_mass)?;
    }
    Ok(())
}
