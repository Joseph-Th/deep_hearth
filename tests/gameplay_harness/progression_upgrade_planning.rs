//! Primitive progression upgrade-material planning shared by generation and live execution.

use deep_hearth::content::{FORM_NATIVE_METAL, MATERIAL_COPPER};
use deep_hearth::core::quantity::Mass;
use deep_hearth::equipment::EquipmentDefinitionId;
use deep_hearth::material::{CommodityKey, MaterialAssemblyProfile};
use deep_hearth::registry::Registries;

use super::manual_craft_topology_planning::manual_craft_topology_plan_for_output_from_inputs;

pub(super) fn equipment_upgrade_additions(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
) -> &MaterialAssemblyProfile {
    registries
        .equipment()
        .get_equipment(equipment)
        .and_then(|definition| definition.upgrade_profile())
        .map(|profile| profile.additions())
        .unwrap_or_else(|| {
            panic!(
                "primitive progression equipment {} is not runtime-upgradeable",
                equipment.value()
            )
        })
}

pub(super) fn native_input_for_upgrade(
    registries: &Registries,
    equipment: EquipmentDefinitionId,
) -> Mass {
    let native = CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL);
    equipment_upgrade_additions(registries, equipment)
        .inputs()
        .iter()
        .try_fold(Mass::ZERO, |total, input| {
            let (craft, batches) = manual_craft_topology_plan_for_output_from_inputs(
                registries,
                input.commodity(),
                input.mass(),
                &[native],
                "primitive copper upgrade planning",
            );
            let input_mass = craft
                .input_mass()
                .milligrams()
                .checked_mul(batches)
                .map(Mass::from_milligrams)
                .unwrap_or_else(|| panic!("primitive upgrade native-copper input overflowed"));
            total.checked_add(input_mass)
        })
        .unwrap_or_else(|| panic!("primitive upgrade native-copper requirement overflowed"))
}
