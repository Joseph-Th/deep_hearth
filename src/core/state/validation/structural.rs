//! Cross-owner structural validation; this child reconciles mounted runtime owners with structural
//! load channels.

use super::*;

pub(super) fn validate_structural_integrations(
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
        let Some(definition) = registries.equipment().get_equipment(equipment.definition()) else {
            return Err(StateValidationError::Equipment(
                EquipmentValidationError::UnknownDefinition {
                    equipment: equipment.id(),
                    definition: equipment.definition(),
                },
            ));
        };
        let current = mounted_mass_by_element
            .get(&element)
            .copied()
            .unwrap_or(AggregateMass::ZERO);
        let next = current
            .checked_add(AggregateMass::from_mass(definition.mass()))
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
        let next = current
            .checked_add(AggregateMass::from_mass(stockpile.stored_mass()))
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
