//! Pure projection from inventory-owned stockpile mass to structure-owned support load.

use std::collections::BTreeMap;

use crate::core::quantity::{AggregateMass, Force, Mass};
use crate::core::state::AppState;
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, StructuralLoadKind, StructuralMutationError,
    calculate_aggregate_weight_force_ceiling,
};

use crate::inventory::StockpileId;

use super::StockpileStructuralLoadError;

pub(super) fn support_force(
    registries: &Registries,
    element: StructuralElementId,
    mass: AggregateMass,
) -> Result<Force, StockpileStructuralLoadError> {
    calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
        .ok_or(StockpileStructuralLoadError::WeightForceOverflow { element })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SupportedMassProjection {
    pub(super) current: AggregateMass,
    pub(super) projected: AggregateMass,
}

pub(super) fn supported_mass_projection(
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<StockpileId, Mass>,
    excluded: Option<StockpileId>,
) -> Result<SupportedMassProjection, StockpileStructuralLoadError> {
    let mut current = AggregateMass::ZERO;
    let mut projected = AggregateMass::ZERO;
    for stockpile in state.inventory().supported_stockpiles(element) {
        let record = state
            .inventory()
            .get_stockpile(stockpile)
            .ok_or(StockpileStructuralLoadError::UnknownStockpile { stockpile })?;
        let current_mass = record
            .stored_mass()
            .checked_add(record.embodied_mass())
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
        current = current
            .checked_add(AggregateMass::from_mass(current_mass))
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;

        if excluded == Some(stockpile) {
            continue;
        }
        let projected_stored_mass = overrides
            .get(&stockpile)
            .copied()
            .unwrap_or_else(|| record.stored_mass());
        let projected_mass = projected_stored_mass
            .checked_add(record.embodied_mass())
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
        projected = projected
            .checked_add(AggregateMass::from_mass(projected_mass))
            .ok_or(StockpileStructuralLoadError::AggregateMassOverflow { element })?;
    }
    Ok(SupportedMassProjection { current, projected })
}

pub(super) fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    current_mass: AggregateMass,
) -> Result<(), StockpileStructuralLoadError> {
    let expected = support_force(registries, element, current_mass)?;
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(StockpileStructuralLoadError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::StoredMatter);
    if stored != expected {
        return Err(StockpileStructuralLoadError::ExistingLoadMismatch {
            element,
            stored,
            expected,
        });
    }
    Ok(())
}
