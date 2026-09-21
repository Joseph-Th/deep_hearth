//! Resolves authored equipment capabilities for gameplay-harness planning.

use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::core::quantity::Mass;
use deep_hearth::equipment::{EquipmentDefinitionId, project_equipment_capability};
use deep_hearth::maintenance::Condition;
use deep_hearth::registry::Registries;

pub(super) fn pristine_equipment_capability(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    capability: CapabilityId,
) -> CapabilityValue {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    project_equipment_capability(definition, Condition::PRISTINE, capability).unwrap_or_else(|| {
        panic!(
            "gameplay harness equipment {} is missing authored capability {}",
            equipment.value(),
            capability.value()
        )
    })
}

pub(super) fn nominal_equipment_mass_capability(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    capability: CapabilityId,
) -> Mass {
    match pristine_equipment_capability(registries, equipment, capability) {
        CapabilityValue::Mass(mass) => mass,
        value @ (CapabilityValue::Temperature(_)
        | CapabilityValue::Pressure(_)
        | CapabilityValue::Power(_)
        | CapabilityValue::MassFlow(_)) => panic!(
            "gameplay harness expected mass capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
    }
}
