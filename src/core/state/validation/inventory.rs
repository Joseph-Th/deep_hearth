//! Cross-owner inventory validation; this child checks authored references and temporal lot
//! provenance.

use crate::core::state::AppState;
use crate::inventory::MaterialLotRecord;
use crate::registry::Registries;

use super::StateValidationError;

pub(super) fn validate_inventory_references(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    validate_stockpile_commodity_references(registries, state)?;
    for lot in state.systems.inventory.lots() {
        validate_lot_cross_owner_references(registries, state, lot)?;
    }
    Ok(())
}

fn validate_stockpile_commodity_references(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    for stockpile in state.systems.inventory.stockpiles() {
        for (commodity, _) in stockpile.contents() {
            if !registries.materials().has_commodity(commodity) {
                return Err(StateValidationError::UnknownStoredCommodity {
                    stockpile: stockpile.id(),
                    commodity,
                });
            }
        }
    }
    Ok(())
}

fn validate_lot_cross_owner_references(
    registries: &Registries,
    state: &AppState,
    lot: &MaterialLotRecord,
) -> Result<(), StateValidationError> {
    if lot.created_at() > state.tick() {
        return Err(StateValidationError::LotCreatedInFuture {
            lot: lot.id(),
            created_at: lot.created_at(),
            current: state.tick(),
        });
    }
    if lot.latest_created_at() > state.tick() {
        return Err(StateValidationError::LotProvenanceInFuture {
            lot: lot.id(),
            latest_created_at: lot.latest_created_at(),
            current: state.tick(),
        });
    }
    for component in lot.composition().components() {
        if registries
            .materials()
            .get_material(component.material())
            .is_none()
        {
            return Err(StateValidationError::UnknownLotCompositionMaterial {
                lot: lot.id(),
                material: component.material(),
            });
        }
    }
    Ok(())
}
