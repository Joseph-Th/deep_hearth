//! Built-in equipment capability, recovery, upgrade, and installation-policy tests.

use std::cmp::Ordering;

use crate::capability::CapabilityValue;
use crate::core::quantity::Mass;
use crate::material::CommodityKey;

use super::authoring::{INDUSTRIAL_MAINTENANCE_MASS_DIVISOR, condition};
use super::*;
use crate::content::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_CRUSHER_BATCH, CAPABILITY_CRUSHER_FLOW,
    CAPABILITY_HEATING_POWER, CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_MINING_FLOW,
    CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS, CAPABILITY_SEPARATOR_BATCH,
    CAPABILITY_SEPARATOR_FLOW, CAPABILITY_TREADLE_POWER_OUTPUT,
};
use crate::content::materials::{
    FORM_BOARD, FORM_HANDLE, FORM_INGOT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD,
};
use crate::equipment::resolve_equipment_capability;
use crate::maintenance::Condition;

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
fn current_mining_capability_providers_remain_portable_hand_tools() {
    let registry = build_equipment_registry();
    for definition in registry.definitions() {
        let capabilities = definition.capabilities();
        let mining_capabilities = [
            CAPABILITY_MINING_FLOW,
            CAPABILITY_MINING_MAX_BATCH,
            CAPABILITY_MINING_MAX_HARDNESS,
        ];
        let provided = mining_capabilities
            .into_iter()
            .filter(|capability| capabilities.get_capability(*capability).is_some())
            .count();
        if provided == 0 {
            continue;
        }
        assert_eq!(
            provided,
            mining_capabilities.len(),
            "equipment {} must provide the complete mining capability contract or none of it",
            definition.id().value()
        );
        assert!(
            !definition.requires_structural_support(),
            "current hand-mining provider {} must remain portable until mechanized excavation has support-aware mining lifecycle semantics",
            definition.id().value()
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
            EQUIPMENT_STONE_QUARRY_PICK,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
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
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
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
        (
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            CAPABILITY_CRUSHER_FLOW,
        ),
        (
            EQUIPMENT_STONE_CRUSHER,
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            CAPABILITY_CRUSHER_BATCH,
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            CAPABILITY_SEPARATOR_FLOW,
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            CAPABILITY_SEPARATOR_BATCH,
        ),
        (
            EQUIPMENT_STONE_QUARRY_PICK,
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            CAPABILITY_MINING_FLOW,
        ),
        (
            EQUIPMENT_STONE_QUARRY_PICK,
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            CAPABILITY_MINING_MAX_BATCH,
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
fn primitive_mining_tools_offer_distinct_bulk_and_hard_rock_investments() {
    let registry = build_equipment_registry();
    let value = |equipment, capability| {
        registry
            .get_equipment(equipment)
            .and_then(|definition| definition.capabilities().get_capability(capability))
            .unwrap_or_else(|| {
                panic!(
                    "primitive mining equipment {} lost capability {}",
                    equipment.value(),
                    capability.value()
                )
            })
    };

    let stone_flow = value(EQUIPMENT_STONE_PICK, CAPABILITY_MINING_FLOW);
    let hard_pick_flow = value(EQUIPMENT_COPPER_REINFORCED_PICK, CAPABILITY_MINING_FLOW);
    let quarry_flow = value(EQUIPMENT_STONE_QUARRY_PICK, CAPABILITY_MINING_FLOW);
    let reinforced_quarry_flow = value(
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        CAPABILITY_MINING_FLOW,
    );
    assert_eq!(stone_flow.compare(hard_pick_flow), Some(Ordering::Less));
    assert_eq!(hard_pick_flow.compare(quarry_flow), Some(Ordering::Less));
    assert_eq!(
        quarry_flow.compare(reinforced_quarry_flow),
        Some(Ordering::Less)
    );

    let hard_pick_batch = value(
        EQUIPMENT_COPPER_REINFORCED_PICK,
        CAPABILITY_MINING_MAX_BATCH,
    );
    let quarry_batch = value(EQUIPMENT_STONE_QUARRY_PICK, CAPABILITY_MINING_MAX_BATCH);
    let reinforced_quarry_batch = value(
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        CAPABILITY_MINING_MAX_BATCH,
    );
    assert_eq!(hard_pick_batch.compare(quarry_batch), Some(Ordering::Less));
    assert_eq!(
        quarry_batch.compare(reinforced_quarry_batch),
        Some(Ordering::Less)
    );

    let stone_hardness = value(EQUIPMENT_STONE_PICK, CAPABILITY_MINING_MAX_HARDNESS);
    let hard_pick_hardness = value(
        EQUIPMENT_COPPER_REINFORCED_PICK,
        CAPABILITY_MINING_MAX_HARDNESS,
    );
    let quarry_hardness = value(EQUIPMENT_STONE_QUARRY_PICK, CAPABILITY_MINING_MAX_HARDNESS);
    let reinforced_quarry_hardness = value(
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        CAPABILITY_MINING_MAX_HARDNESS,
    );
    assert_eq!(stone_hardness, quarry_hardness);
    assert_eq!(
        quarry_hardness.compare(reinforced_quarry_hardness),
        Some(Ordering::Less)
    );
    assert_eq!(
        reinforced_quarry_hardness.compare(hard_pick_hardness),
        Some(Ordering::Less)
    );
}

#[test]
fn timber_treadle_is_a_bulk_material_alternative_between_stone_and_copper_cranks() {
    let registry = build_equipment_registry();
    let power = |equipment, capability| {
        registry
            .get_equipment(equipment)
            .and_then(|definition| definition.capabilities().get_capability(capability))
            .unwrap_or_else(|| {
                panic!(
                    "primitive power equipment {} disappeared",
                    equipment.value()
                )
            })
    };
    let hand = power(EQUIPMENT_STONE_HAND_CRANK, CAPABILITY_MANUAL_POWER_OUTPUT);
    let treadle = power(
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        CAPABILITY_TREADLE_POWER_OUTPUT,
    );
    let copper = power(
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        CAPABILITY_MANUAL_POWER_OUTPUT,
    );
    assert_eq!(hand.compare(treadle), Some(Ordering::Less));
    assert_eq!(treadle.compare(copper), Some(Ordering::Less));

    let assembly = registry
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_DRIVE)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("timber treadle lost its assembly profile"));
    assert_eq!(assembly.input_mass(), Mass::from_milligrams(2_900_000));
    assert_eq!(
        assembly
            .inputs()
            .iter()
            .find(|input| input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(1_600_000))
    );
}

#[test]
fn primitive_processing_wear_reduces_safe_batch_capacity_before_failure() {
    let registry = build_equipment_registry();
    for (equipment, capability, pristine, degraded) in [
        (
            EQUIPMENT_STONE_CRUSHER,
            CAPABILITY_CRUSHER_BATCH,
            Mass::from_milligrams(1_000_000),
            Mass::from_milligrams(500_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            CAPABILITY_CRUSHER_BATCH,
            Mass::from_milligrams(1_500_000),
            Mass::from_milligrams(750_000),
        ),
        (
            EQUIPMENT_STONE_SEPARATOR,
            CAPABILITY_SEPARATOR_BATCH,
            Mass::from_milligrams(500_000),
            Mass::from_milligrams(250_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            CAPABILITY_SEPARATOR_BATCH,
            Mass::from_milligrams(750_000),
            Mass::from_milligrams(375_000),
        ),
    ] {
        let definition = registry
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("primitive processing equipment disappeared"));
        assert_eq!(
            resolve_equipment_capability(definition, Condition::PRISTINE, capability),
            Some(CapabilityValue::Mass(pristine))
        );
        assert_eq!(
            resolve_equipment_capability(definition, condition(600_000), capability),
            Some(CapabilityValue::Mass(degraded))
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
        EQUIPMENT_STONE_QUARRY_PICK,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
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
