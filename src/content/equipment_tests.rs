//! Built-in equipment capability, recovery, upgrade, and installation-policy tests.

use std::cmp::Ordering;

use super::*;
use crate::equipment::resolve_equipment_capability;

#[test]
fn failed_thermal_equipment_exposes_no_heat_transfer_capability() {
    let registry = build_equipment_registry();
    for (equipment, capability) in [
        (EQUIPMENT_ELECTRIC_FURNACE, CAPABILITY_HEATING_POWER),
        (EQUIPMENT_CASTING_MOLD, CAPABILITY_COOLING_POWER),
    ] {
        let definition = registry
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("built-in thermal equipment disappeared"));
        assert_eq!(
            resolve_equipment_capability(definition, Condition::FAILED, capability),
            None
        );
    }
}

#[test]
fn primitive_equipment_services_replace_authored_embodied_components() {
    let registry = build_equipment_registry();
    for (equipment, component, mass) in [
        (
            EQUIPMENT_STONE_PICK,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            EQUIPMENT_STONE_HAND_CRANK,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_PICK,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
        (
            EQUIPMENT_STONE_CRUSHER,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
    ] {
        let maintenance = registry
            .get_equipment(equipment)
            .and_then(|definition| definition.maintenance_profile())
            .unwrap_or_else(|| {
                panic!(
                    "primitive equipment {} lost service route",
                    equipment.value()
                )
            });
        assert!(maintenance.is_component_replacement());
        assert_eq!(maintenance.replacement(), component);
        assert_eq!(maintenance.full_service_replacement_mass(), mass);
        assert_eq!(maintenance.restored_condition(), Condition::PRISTINE);
        assert_eq!(
            maintenance.spent(),
            CommodityKey::new(component.material(), FORM_SCRAP)
        );
    }
}

#[test]
fn industrial_maintenance_replacement_mass_scales_with_machine_mass() {
    let registry = build_equipment_registry();
    for equipment in [
        EQUIPMENT_JAW_CRUSHER,
        EQUIPMENT_ELECTRIC_FURNACE,
        EQUIPMENT_CASTING_MOLD,
        EQUIPMENT_DRY_SCREEN,
        EQUIPMENT_GRINDING_MILL,
        EQUIPMENT_GRAVITY_SEPARATOR,
    ] {
        let definition = registry
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("industrial equipment {} disappeared", equipment.value()));
        let maintenance = definition.maintenance_profile().unwrap_or_else(|| {
            panic!(
                "industrial equipment {} lost its maintenance profile",
                equipment.value()
            )
        });
        assert_eq!(
            maintenance.full_service_replacement_mass().milligrams(),
            definition
                .mass()
                .milligrams()
                .div_ceil(INDUSTRIAL_MAINTENANCE_MASS_DIVISOR),
            "industrial full-service maintenance stock must scale with machine mass"
        );
        assert_eq!(
            maintenance.replacement(),
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
        );
        assert_eq!(
            maintenance.spent(),
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)
        );
    }
}

#[test]
fn every_builtin_equipment_definition_has_a_condition_recovery_route() {
    let registry = build_equipment_registry();
    for definition in registry.definitions() {
        assert!(
            definition.maintenance_profile().is_some() || definition.worn_recovery_form().is_some(),
            "built-in equipment {} must be repairable or destructively recoverable after wear",
            definition.id().value()
        );
    }
}

#[test]
fn primitive_copper_upgrades_improve_their_intended_nominal_capability() {
    let registry = build_equipment_registry();
    for (base, upgraded, capability) in [
        (
            EQUIPMENT_STONE_PICK,
            EQUIPMENT_COPPER_REINFORCED_PICK,
            CAPABILITY_MINING_FLOW,
        ),
        (
            EQUIPMENT_STONE_HAND_CRANK,
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            CAPABILITY_MANUAL_POWER_OUTPUT,
        ),
    ] {
        let base_definition = registry
            .get_equipment(base)
            .unwrap_or_else(|| panic!("primitive base equipment {} disappeared", base.value()));
        let upgraded_definition = registry.get_equipment(upgraded).unwrap_or_else(|| {
            panic!(
                "primitive upgraded equipment {} disappeared",
                upgraded.value()
            )
        });
        let base_value = base_definition
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| {
                panic!(
                    "primitive base equipment {} lost capability {}",
                    base.value(),
                    capability.value()
                )
            });
        let upgraded_value = upgraded_definition
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| {
                panic!(
                    "primitive upgraded equipment {} lost capability {}",
                    upgraded.value(),
                    capability.value()
                )
            });

        assert_eq!(
            base_value.compare(upgraded_value),
            Some(Ordering::Less),
            "primitive upgrade {} -> {} must improve capability {}",
            base.value(),
            upgraded.value(),
            capability.value()
        );
    }
}

#[test]
fn industrial_machines_are_fixed_while_primitive_equipment_remains_portable() {
    let registry = build_equipment_registry();
    for equipment in [
        EQUIPMENT_JAW_CRUSHER,
        EQUIPMENT_ELECTRIC_FURNACE,
        EQUIPMENT_CASTING_MOLD,
        EQUIPMENT_DRY_SCREEN,
        EQUIPMENT_GRAVITY_SEPARATOR,
        EQUIPMENT_GRINDING_MILL,
    ] {
        assert!(
            registry
                .get_equipment(equipment)
                .is_some_and(|definition| definition.requires_structural_support()),
            "industrial equipment {} must require structural installation",
            equipment.value()
        );
    }
    for equipment in [
        EQUIPMENT_STONE_PICK,
        EQUIPMENT_STONE_HAND_CRANK,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
    ] {
        assert!(
            registry
                .get_equipment(equipment)
                .is_some_and(|definition| !definition.requires_structural_support()),
            "primitive equipment {} must remain portable",
            equipment.value()
        );
    }
}
