//! Resolves authored equipment capabilities for gameplay-harness planning.

use deep_hearth::capability::{CapabilityId, CapabilityValue};
use deep_hearth::core::quantity::Mass;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::registry::Registries;

pub(super) fn nominal_equipment_mass_capability(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
    capability: CapabilityId,
) -> Mass {
    let definition = registries
        .equipment()
        .get_equipment(equipment)
        .unwrap_or_else(|| panic!("gameplay harness equipment definition disappeared"));
    match definition.capabilities().get_capability(capability) {
        Some(CapabilityValue::Mass(mass)) => mass,
        Some(value) => panic!(
            "gameplay harness expected mass capability {} on equipment {} but found {:?}",
            capability.value(),
            equipment.value(),
            value.kind()
        ),
        None => panic!(
            "gameplay harness equipment {} is missing authored mass capability {}",
            equipment.value(),
            capability.value()
        ),
    }
}
