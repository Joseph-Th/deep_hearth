//! Validates cross-owner structural support and load-channel consistency.

use std::collections::BTreeMap;

use crate::core::quantity::AggregateMass;
use crate::core::state::AppState;
use crate::fluid::validate_existing_fluid_load;
use crate::registry::Registries;
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, analyze_structure,
    calculate_aggregate_weight_force_ceiling,
};

use super::StateValidationError;

fn validate_equipment_structural_loads(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    let mut mounted_mass_by_element = BTreeMap::<StructuralElementId, AggregateMass>::new();
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
        let current = mounted_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let next = current
            .checked_add(AggregateMass::from_mass(equipment.embodied_mass()))
            .ok_or(StateValidationError::MountedEquipmentMassOverflow { element })?;
        mounted_mass_by_element.insert(element, next);
    }
    for structural in state.systems.structures.elements() {
        let element = structural.id();
        let mass = mounted_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
            .ok_or(StateValidationError::MountedEquipmentWeightOverflow { element })?;
        let stored = structural.load(StructuralLoadKind::Equipment);
        if stored != expected {
            return Err(StateValidationError::EquipmentStructuralLoadMismatch {
                element,
                stored,
                expected,
            });
        }
    }

    Ok(())
}

fn validate_stockpile_structural_loads(
    registries: &Registries,
    state: &AppState,
) -> Result<(), StateValidationError> {
    let mut stored_mass_by_element = BTreeMap::<StructuralElementId, AggregateMass>::new();
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
        let current = stored_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let stockpile_mass = stockpile
            .stored_mass()
            .checked_add(stockpile.embodied_mass())
            .ok_or(StateValidationError::StoredMatterMassOverflow { element })?;
        let next = current
            .checked_add(AggregateMass::from_mass(stockpile_mass))
            .ok_or(StateValidationError::StoredMatterMassOverflow { element })?;
        stored_mass_by_element.insert(element, next);
    }
    for structural in state.systems.structures.elements() {
        let element = structural.id();
        let mass = stored_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let expected = calculate_aggregate_weight_force_ceiling(mass, registries.core().gravity())
            .ok_or(StateValidationError::StoredMatterWeightOverflow { element })?;
        let stored = structural.load(StructuralLoadKind::StoredMatter);
        if stored != expected {
            return Err(StateValidationError::StoredMatterStructuralLoadMismatch {
                element,
                stored,
                expected,
            });
        }
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
