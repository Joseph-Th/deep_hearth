//! Validates cross-owner structural support and load-channel consistency.

use crate::core::state::AppState;
use crate::equipment::{
    EquipmentStructuralLoadConsistencyError, validate_existing_equipment_structural_load,
};
use crate::fluid::validate_existing_fluid_load;
use crate::inventory::{
    StockpileStructuralLoadConsistencyError, validate_existing_stockpile_structural_load,
};
use crate::registry::Registries;
use crate::structural::{StructuralLifecycle, StructuralLoadKind, analyze_structure};

use super::StateValidationError;

fn validate_equipment_structural_loads(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    for equipment in state.systems.equipment.equipment() {
        let Some(element) = equipment.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
            return Err(StateValidationError::UnknownEquipmentSupport {
                equipment: equipment.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::EquipmentSupportedByPlannedElement {
                equipment: equipment.id(),
                element,
            });
        }
    }
    for structural in state.systems.structures.elements() {
        let element = structural.id();
        let stored = structural.load(StructuralLoadKind::Equipment);
        validate_existing_equipment_structural_load(registries, state, element, stored).map_err(
            |error| match error {
                EquipmentStructuralLoadConsistencyError::AggregateMassOverflow { element } => {
                    StateValidationError::MountedEquipmentMassOverflow { element }
                }
                EquipmentStructuralLoadConsistencyError::WeightForceOverflow { element } => {
                    StateValidationError::MountedEquipmentWeightOverflow { element }
                }
                EquipmentStructuralLoadConsistencyError::ExistingLoadMismatch {
                    element,
                    stored,
                    expected,
                } => StateValidationError::EquipmentStructuralLoadMismatch {
                    element,
                    stored,
                    expected,
                },
            },
        )?;
    }

    Ok(())
}

fn validate_stockpile_structural_loads(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    for stockpile in state.systems.inventory.stockpiles() {
        let Some(element) = stockpile.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
            return Err(StateValidationError::UnknownStockpileSupport {
                stockpile: stockpile.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::StockpileSupportedByPlannedElement {
                stockpile: stockpile.id(),
                element,
            });
        }
    }
    for structural in state.systems.structures.elements() {
        let element = structural.id();
        let stored = structural.load(StructuralLoadKind::StoredMatter);
        validate_existing_stockpile_structural_load(registries, state, element, stored).map_err(
            |error| match error {
                StockpileStructuralLoadConsistencyError::UnknownStockpile { stockpile } => {
                    StateValidationError::UnknownStockpileSupport { stockpile, element }
                }
                StockpileStructuralLoadConsistencyError::AggregateMassOverflow { element } => {
                    StateValidationError::StoredMatterMassOverflow { element }
                }
                StockpileStructuralLoadConsistencyError::WeightForceOverflow { element } => {
                    StateValidationError::StoredMatterWeightOverflow { element }
                }
                StockpileStructuralLoadConsistencyError::ExistingLoadMismatch {
                    element,
                    stored,
                    expected,
                } => StateValidationError::StoredMatterStructuralLoadMismatch {
                    element,
                    stored,
                    expected,
                },
            },
        )?;
    }

    Ok(())
}

fn validate_fluid_structural_loads(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    for store in state.systems.fluid.stores() {
        let Some(element) = store.supported_by() else {
            continue;
        };
        let Some(structural) = state.systems.structures.get_element(element) else {
            return Err(StateValidationError::UnknownFluidSupport {
                store: store.id(),
                element,
            });
        };
        if structural.lifecycle() == StructuralLifecycle::Planned {
            return Err(StateValidationError::FluidSupportedByPlannedElement {
                store: store.id(),
                element,
            });
        }
    }
    for structural in state.systems.structures.elements() {
        validate_existing_fluid_load(registries, state, structural.id())
            .map_err(StateValidationError::FluidStructuralLoad)?;
    }

    Ok(())
}

fn validate_resolved_structural_damage(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    let structural_analysis = analyze_structure(
        registries.structural(),
        registries.materials(),
        &state.systems.structures,
    )
    .map_err(StateValidationError::StructureAnalysis)?;
    if let Some(event) = structural_analysis.damage_events().first().copied() {
        return Err(StateValidationError::UnresolvedStructuralDamage { event });
    }
    Ok(())
}

pub(super) fn validate_structural_integrations(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    validate_equipment_structural_loads(registries, state)?;
    validate_stockpile_structural_loads(registries, state)?;
    validate_fluid_structural_loads(registries, state)?;
    validate_resolved_structural_damage(registries, state)
}
