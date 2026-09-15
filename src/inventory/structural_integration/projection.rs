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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StockpileStructuralLoadConsistencyError {
    UnknownStockpile {
        stockpile: StockpileId,
    },
    AggregateMassOverflow {
        element: StructuralElementId,
    },
    WeightForceOverflow {
        element: StructuralElementId,
    },
    ExistingLoadMismatch {
        element: StructuralElementId,
        stored: Force,
        expected: Force,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedStockpileMassError {
    UnknownStockpile { stockpile: StockpileId },
    AggregateMassOverflow { element: StructuralElementId },
}

impl From<SupportedStockpileMassError> for StockpileStructuralLoadError {
    fn from(error: SupportedStockpileMassError) -> Self {
        match error {
            SupportedStockpileMassError::UnknownStockpile { stockpile } => {
                Self::UnknownStockpile { stockpile }
            }
            SupportedStockpileMassError::AggregateMassOverflow { element } => {
                Self::AggregateMassOverflow { element }
            }
        }
    }
}

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

fn project_supported_mass(
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<StockpileId, Mass>,
    excluded: Option<StockpileId>,
) -> Result<SupportedMassProjection, SupportedStockpileMassError> {
    let mut current = AggregateMass::ZERO;
    let mut projected = AggregateMass::ZERO;
    for stockpile in state.inventory().supported_stockpiles(element) {
        let record = state
            .inventory()
            .get_stockpile(stockpile)
            .ok_or(SupportedStockpileMassError::UnknownStockpile { stockpile })?;
        let current_mass = record
            .stored_mass()
            .checked_add(record.embodied_mass())
            .ok_or(SupportedStockpileMassError::AggregateMassOverflow { element })?;
        current = current
            .checked_add(AggregateMass::from_mass(current_mass))
            .ok_or(SupportedStockpileMassError::AggregateMassOverflow { element })?;

        if excluded == Some(stockpile) {
            continue;
        }
        let projected_stored_mass = overrides
            .get(&stockpile)
            .copied()
            .unwrap_or_else(|| record.stored_mass());
        let projected_mass = projected_stored_mass
            .checked_add(record.embodied_mass())
            .ok_or(SupportedStockpileMassError::AggregateMassOverflow { element })?;
        projected = projected
            .checked_add(AggregateMass::from_mass(projected_mass))
            .ok_or(SupportedStockpileMassError::AggregateMassOverflow { element })?;
    }
    Ok(SupportedMassProjection { current, projected })
}

pub(super) fn supported_mass_projection(
    state: &AppState,
    element: StructuralElementId,
    overrides: &BTreeMap<StockpileId, Mass>,
    excluded: Option<StockpileId>,
) -> Result<SupportedMassProjection, StockpileStructuralLoadError> {
    project_supported_mass(state, element, overrides, excluded).map_err(Into::into)
}

pub(super) fn validate_existing_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    current_mass: AggregateMass,
) -> Result<(), StockpileStructuralLoadError> {
    let stored = state
        .structures()
        .get_element(element)
        .ok_or(StockpileStructuralLoadError::Structure(
            StructuralMutationError::UnknownElement { element },
        ))?
        .load(StructuralLoadKind::StoredMatter);
    validate_stockpile_load_value(registries, element, current_mass, stored).map_err(|error| {
        match error {
            StockpileStructuralLoadConsistencyError::WeightForceOverflow { element } => {
                StockpileStructuralLoadError::WeightForceOverflow { element }
            }
            StockpileStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            } => StockpileStructuralLoadError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            },
            StockpileStructuralLoadConsistencyError::UnknownStockpile { .. }
            | StockpileStructuralLoadConsistencyError::AggregateMassOverflow { .. } => {
                unreachable!("load-value validation receives an already-aggregated mass")
            }
        }
    })
}

fn validate_stockpile_load_value(
    registries: &Registries,
    element: StructuralElementId,
    current_mass: AggregateMass,
    stored: Force,
) -> Result<(), StockpileStructuralLoadConsistencyError> {
    let expected =
        calculate_aggregate_weight_force_ceiling(current_mass, registries.core().gravity())
            .ok_or(StockpileStructuralLoadConsistencyError::WeightForceOverflow { element })?;
    if stored != expected {
        return Err(
            StockpileStructuralLoadConsistencyError::ExistingLoadMismatch {
                element,
                stored,
                expected,
            },
        );
    }
    Ok(())
}

pub(crate) fn validate_existing_stockpile_structural_load(
    registries: &Registries,
    state: &AppState,
    element: StructuralElementId,
    stored: Force,
) -> Result<(), StockpileStructuralLoadConsistencyError> {
    let current_mass = project_supported_mass(state, element, &BTreeMap::new(), None)
        .map_err(|error| match error {
            SupportedStockpileMassError::UnknownStockpile { stockpile } => {
                StockpileStructuralLoadConsistencyError::UnknownStockpile { stockpile }
            }
            SupportedStockpileMassError::AggregateMassOverflow { element } => {
                StockpileStructuralLoadConsistencyError::AggregateMassOverflow { element }
            }
        })?
        .current;
    validate_stockpile_load_value(registries, element, current_mass, stored)
}
