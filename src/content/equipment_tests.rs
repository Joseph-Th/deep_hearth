//! Built-in equipment capability, recovery, upgrade, and installation-policy tests.

use std::cmp::Ordering;

use crate::capability::CapabilityValue;
use crate::core::quantity::Mass;
use crate::material::{CommodityKey, MaterialInputSpec};

use super::authoring::{INDUSTRIAL_MAINTENANCE_MASS_DIVISOR, condition};
use super::*;
use crate::content::capabilities::{
    CAPABILITY_COOLING_POWER, CAPABILITY_COPPER_HAMMERING_FLOW, CAPABILITY_CRUSHER_BATCH,
    CAPABILITY_CRUSHER_FLOW, CAPABILITY_GRINDER_BATCH, CAPABILITY_GRINDER_FLOW,
    CAPABILITY_HEATING_POWER, CAPABILITY_MANUAL_POWER_OUTPUT, CAPABILITY_MINING_FLOW,
    CAPABILITY_MINING_MAX_BATCH, CAPABILITY_MINING_MAX_HARDNESS,
    CAPABILITY_POWERED_STONE_GRINDING_FLOW, CAPABILITY_POWERED_WOOD_TURNING_FLOW,
    CAPABILITY_SAWING_FLOW, CAPABILITY_SCREEN_BATCH, CAPABILITY_SCREEN_FLOW,
    CAPABILITY_SEPARATOR_BATCH, CAPABILITY_SEPARATOR_FLOW, CAPABILITY_STONE_GRINDING_FLOW,
    CAPABILITY_TREADLE_POWER_OUTPUT, CAPABILITY_WALKING_WHEEL_POWER_OUTPUT,
    CAPABILITY_WOOD_TURNING_FLOW, CAPABILITY_WOODWORKING_FLOW,
};
use crate::content::materials::{
    FORM_BOARD, FORM_FLYWHEEL, FORM_GRINDSTONE_WHEEL, FORM_HANDLE, FORM_INGOT, FORM_REINFORCEMENT,
    FORM_SAW_BLADE, FORM_SCRAP, FORM_SCREEN_PLATE, FORM_TIMBER_RIDDLE_PANEL, FORM_TOOL,
    MATERIAL_COPPER, MATERIAL_STONE, MATERIAL_WOOD,
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
        (
            EQUIPMENT_STONE_ROTARY_QUERN,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
            Mass::from_milligrams(1_400_000),
        ),
        (
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
            Mass::from_milligrams(1_400_000),
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
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(1_600_000),
        ),
        (
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(500_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(500_000),
        ),
        (
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            Mass::from_milligrams(54_000),
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
fn constructible_equipment_uses_assembly_as_its_physical_mass_authority() {
    let registry = build_equipment_registry();
    let mut constructible = 0_usize;

    for definition in registry.definitions() {
        let Some(assembly) = definition.assembly_profile() else {
            continue;
        };
        constructible += 1;
        assert_eq!(
            definition.mass(),
            assembly.input_mass(),
            "constructible equipment {} must derive total mass from its assembly",
            definition.id().value()
        );

        let maintenance = definition
            .maintenance_profile()
            .unwrap_or_else(|| panic!("constructible equipment lost authored maintenance"));
        if maintenance.is_component_replacement() {
            let replacement = maintenance.replacement();
            let embodied_mass = assembly
                .inputs()
                .iter()
                .find(|input| input.commodity() == replacement)
                .map(|input| input.mass())
                .unwrap_or_else(|| {
                    panic!(
                        "equipment {} service component {} is absent from its assembly",
                        definition.id().value(),
                        replacement.value()
                    )
                });
            assert_eq!(
                maintenance.full_service_replacement_mass(),
                embodied_mass,
                "equipment {} service mass must come from its embodied replacement component",
                definition.id().value()
            );
        }
    }

    assert_eq!(constructible, 34);
}

#[test]
fn saw_bench_is_a_distinct_high_throughput_woodworking_provider() {
    let registry = build_equipment_registry();
    let saw = registry
        .get_equipment(EQUIPMENT_TIMBER_FRAME_SAW_BENCH)
        .unwrap_or_else(|| panic!("timber frame saw bench disappeared"));
    let reinforced_adze = registry
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE)
        .unwrap_or_else(|| panic!("reinforced woodworking adze disappeared"));
    assert_eq!(
        saw.capabilities().get_capability(CAPABILITY_SAWING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(40_000)
        ))
    );
    assert!(
        saw.capabilities()
            .get_capability(CAPABILITY_WOODWORKING_FLOW)
            .is_none(),
        "the saw bench must not impersonate a general hewing tool"
    );
    assert!(
        reinforced_adze
            .capabilities()
            .get_capability(CAPABILITY_SAWING_FLOW)
            .is_none(),
        "the adze must not unlock the high-yield sawing recipe"
    );
}

#[test]
fn lathes_specialize_round_timber_work_and_upgrade_into_unattended_turning() {
    let registry = build_equipment_registry();
    let pole = registry
        .get_equipment(EQUIPMENT_TIMBER_SPRING_POLE_LATHE)
        .unwrap_or_else(|| panic!("timber spring-pole lathe disappeared"));
    let powered = registry
        .get_equipment(EQUIPMENT_TIMBER_FLYWHEEL_LATHE)
        .unwrap_or_else(|| panic!("flywheel timber lathe disappeared"));
    let adze = registry
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE)
        .unwrap_or_else(|| panic!("reinforced adze disappeared"));
    let saw = registry
        .get_equipment(EQUIPMENT_TIMBER_SASH_SAWMILL)
        .unwrap_or_else(|| panic!("sash sawmill disappeared"));

    assert_eq!(
        pole.capabilities()
            .get_capability(CAPABILITY_WOOD_TURNING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(25_000)
        ))
    );
    assert_eq!(
        powered
            .capabilities()
            .get_capability(CAPABILITY_POWERED_WOOD_TURNING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(100_000)
        ))
    );
    assert!(
        pole.capabilities()
            .get_capability(CAPABILITY_WOODWORKING_FLOW)
            .is_none()
            && pole
                .capabilities()
                .get_capability(CAPABILITY_SAWING_FLOW)
                .is_none(),
        "turning must remain distinct from general hewing and sawing"
    );
    assert!(
        adze.capabilities()
            .get_capability(CAPABILITY_WOOD_TURNING_FLOW)
            .is_none()
            && saw
                .capabilities()
                .get_capability(CAPABILITY_WOOD_TURNING_FLOW)
                .is_none(),
        "existing woodworking equipment must not silently acquire lathe semantics"
    );

    let upgrade = powered
        .upgrade_profile()
        .unwrap_or_else(|| panic!("flywheel lathe lost its spring-pole upgrade route"));
    assert_eq!(upgrade.from(), EQUIPMENT_TIMBER_SPRING_POLE_LATHE);
    assert_eq!(
        upgrade.additions().input_mass(),
        Mass::from_milligrams(2_920_000)
    );
    assert!(
        upgrade
            .additions()
            .inputs()
            .contains(&MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
                Mass::from_milligrams(900_000),
            ))
    );
    let service = powered
        .maintenance_profile()
        .unwrap_or_else(|| panic!("flywheel lathe lost cutter service"));
    assert_eq!(
        service.replacement(),
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
    );
}

#[test]
fn toolroom_grindstones_specialize_service_recovery_and_preserve_the_treadle_fallback() {
    let registry = build_equipment_registry();
    let treadle = registry
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_GRINDSTONE)
        .unwrap_or_else(|| panic!("treadle grindstone disappeared"));
    let powered = registry
        .get_equipment(EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH)
        .unwrap_or_else(|| panic!("flywheel toolroom grindstone disappeared"));
    let ore_quern = registry
        .get_equipment(EQUIPMENT_STONE_ROTARY_QUERN)
        .unwrap_or_else(|| panic!("stone rotary quern disappeared"));

    assert_eq!(
        treadle
            .capabilities()
            .get_capability(CAPABILITY_STONE_GRINDING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(10_000)
        ))
    );
    assert_eq!(
        powered
            .capabilities()
            .get_capability(CAPABILITY_POWERED_STONE_GRINDING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(40_000)
        ))
    );
    assert!(
        ore_quern
            .capabilities()
            .get_capability(CAPABILITY_STONE_GRINDING_FLOW)
            .is_none(),
        "ore comminution must not silently become toolroom abrasion"
    );
    assert!(
        treadle
            .capabilities()
            .get_capability(CAPABILITY_GRINDER_FLOW)
            .is_none(),
        "the toolroom grindstone must not impersonate an ore grinder"
    );

    let upgrade = powered
        .upgrade_profile()
        .unwrap_or_else(|| panic!("flywheel grindstone lost its treadle upgrade route"));
    assert_eq!(upgrade.from(), EQUIPMENT_TIMBER_TREADLE_GRINDSTONE);
    assert_eq!(
        upgrade.additions().input_mass(),
        Mass::from_milligrams(2_920_000)
    );
    let service = powered
        .maintenance_profile()
        .unwrap_or_else(|| panic!("toolroom grindstone lost wheel replacement service"));
    assert_eq!(
        service.replacement(),
        CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL)
    );
    assert_eq!(
        service.full_service_replacement_mass(),
        Mass::from_milligrams(1_400_000)
    );
}

#[test]
fn timber_riddle_opens_sizing_before_copper_and_plate_upgrade_preserves_the_investment() {
    let registry = build_equipment_registry();
    let timber = registry
        .get_equipment(EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN)
        .unwrap_or_else(|| panic!("timber riddle sizing screen disappeared"));
    let copper = registry
        .get_equipment(EQUIPMENT_COPPER_PLATE_SIZING_SCREEN)
        .unwrap_or_else(|| panic!("copper sizing screen disappeared"));

    for capability in [CAPABILITY_SCREEN_FLOW, CAPABILITY_SCREEN_BATCH] {
        let timber_value = timber
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| panic!("timber riddle lost sizing capability"));
        let copper_value = copper
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| panic!("copper screen lost sizing capability"));
        assert_eq!(
            timber_value.compare(copper_value),
            Some(Ordering::Less),
            "scarce copper must materially improve the existing sizing investment"
        );
    }

    let assembly = timber
        .assembly_profile()
        .unwrap_or_else(|| panic!("timber riddle lost its assembly route"));
    assert_eq!(assembly.input_mass(), Mass::from_milligrams(1_600_000));
    assert_eq!(
        assembly.inputs(),
        &[
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
                Mass::from_milligrams(200_000),
            ),
            MaterialInputSpec::pure(
                CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL),
                Mass::from_milligrams(1_400_000),
            ),
        ]
    );
    let timber_service = timber
        .maintenance_profile()
        .unwrap_or_else(|| panic!("timber riddle lost panel replacement service"));
    assert_eq!(
        timber_service.replacement(),
        CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL)
    );

    let upgrade = copper
        .upgrade_profile()
        .unwrap_or_else(|| panic!("copper screen lost timber-riddle upgrade route"));
    assert_eq!(upgrade.from(), EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN);
    assert_eq!(
        upgrade.additions().inputs(),
        &[MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            Mass::from_milligrams(18_000),
        )]
    );
    let copper_service = copper
        .maintenance_profile()
        .unwrap_or_else(|| panic!("copper sizing screen lost panel replacement service"));
    assert_eq!(
        copper_service.replacement(),
        CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL)
    );
    assert_eq!(copper_service, timber_service);
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
        assert!(!maintenance.is_component_replacement());
        assert!(maintenance.full_service_replacement_mass() > Mass::ZERO);
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
fn industrial_service_preserves_steady_life_attention_savings() {
    use crate::content::processes::{
        PROCESS_CAST_PURE_COPPER, PROCESS_CRUSH_ORE, PROCESS_HEAT_MATERIAL_BATCH,
        PROCESS_REGRIND_COPPER_TAILINGS, PROCESS_SCAVENGE_COPPER_TAILINGS,
        PROCESS_SCREEN_CRUSHED_ORE,
    };

    let registries = crate::content::build_registries();
    let ore = registries.ore_processing();
    let thermal = registries.thermal();
    // Use the highest-wear authored route for machines with multiple processing routes.
    let machine_wear = [
        (
            EQUIPMENT_JAW_CRUSHER,
            ore.get_comminution(PROCESS_CRUSH_ORE)
                .unwrap_or_else(|| panic!("crusher process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
        (
            EQUIPMENT_GRINDING_MILL,
            ore.get_comminution(PROCESS_REGRIND_COPPER_TAILINGS)
                .unwrap_or_else(|| panic!("tailings regrind process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
        (
            EQUIPMENT_DRY_SCREEN,
            ore.get_screening(PROCESS_SCREEN_CRUSHED_ORE)
                .unwrap_or_else(|| panic!("screening process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
        (
            EQUIPMENT_GRAVITY_SEPARATOR,
            ore.get_constituent_separation(PROCESS_SCAVENGE_COPPER_TAILINGS)
                .unwrap_or_else(|| panic!("tailings scavenging process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
        (
            EQUIPMENT_ELECTRIC_FURNACE,
            thermal
                .get_sensible_heating(PROCESS_HEAT_MATERIAL_BATCH)
                .unwrap_or_else(|| panic!("heating process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
        (
            EQUIPMENT_CASTING_MOLD,
            thermal
                .get_casting(PROCESS_CAST_PURE_COPPER)
                .unwrap_or_else(|| panic!("casting process disappeared"))
                .condition_wear_ppm_per_active_tick(),
        ),
    ];
    let component_service = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .and_then(|definition| definition.maintenance_profile())
        .unwrap_or_else(|| panic!("stone pick component service disappeared"));

    for (equipment, wear_per_active_tick) in machine_wear {
        let service = registries
            .equipment()
            .get_equipment(equipment)
            .and_then(|definition| definition.maintenance_profile())
            .unwrap_or_else(|| panic!("industrial service {} disappeared", equipment.value()));
        let duration = u128::from(service.full_service_duration().value());
        let restored = u128::from(service.restored_condition().parts_per_million());
        // Service/work = duration / (restored condition / wear), without floating-point rounding.
        let attention_numerator = duration * u128::from(wear_per_active_tick);
        assert!(
            attention_numerator > 0,
            "industrial upkeep must not be free"
        );
        assert!(
            attention_numerator * 5 < restored,
            "machine {} must spend less than one fifth of its productive lifetime in manual service",
            equipment.value()
        );
        assert!(
            duration
                * u128::from(
                    component_service
                        .full_service_replacement_mass()
                        .milligrams()
                )
                > u128::from(component_service.full_service_duration().value())
                    * u128::from(service.full_service_replacement_mass().milligrams()),
            "bulk industrial service must remain slower per mass than component replacement"
        );
        assert_eq!(service.exertion(), component_service.exertion());
    }
}

#[test]
fn every_builtin_equipment_definition_has_authored_maintenance() {
    let registry = build_equipment_registry();
    for definition in registry.definitions() {
        assert!(
            definition.maintenance_profile().is_some(),
            "built-in equipment {} must have an authored maintenance route",
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
            EQUIPMENT_STONE_ROTARY_QUERN,
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            CAPABILITY_GRINDER_FLOW,
        ),
        (
            EQUIPMENT_STONE_ROTARY_QUERN,
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            CAPABILITY_GRINDER_BATCH,
        ),
        (
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            CAPABILITY_SCREEN_FLOW,
        ),
        (
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            CAPABILITY_SCREEN_BATCH,
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
        (
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            CAPABILITY_WOODWORKING_FLOW,
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
fn settlement_ore_dressing_machines_trade_bulk_material_for_consolidated_capability() {
    let registry = build_equipment_registry();
    let definition = |equipment| {
        registry.get_equipment(equipment).unwrap_or_else(|| {
            panic!(
                "settlement processing equipment {} disappeared",
                equipment.value()
            )
        })
    };
    let capability = |equipment, capability| {
        definition(equipment)
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| {
                panic!(
                    "settlement processing equipment {} lost capability {}",
                    equipment.value(),
                    capability.value()
                )
            })
    };

    let mill = definition(EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL);
    let crusher = definition(EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER);
    let quern = definition(EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN);
    assert!(!mill.requires_structural_support());
    assert_eq!(mill.mass(), Mass::from_milligrams(6_820_000));
    assert_eq!(
        capability(
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            CAPABILITY_CRUSHER_FLOW
        )
        .compare(capability(
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            CAPABILITY_CRUSHER_FLOW
        )),
        Some(Ordering::Less)
    );
    assert_eq!(
        capability(
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            CAPABILITY_GRINDER_FLOW
        )
        .compare(capability(
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            CAPABILITY_GRINDER_FLOW
        )),
        Some(Ordering::Less)
    );
    assert!(
        mill.mass()
            > crusher
                .mass()
                .checked_add(quern.mass())
                .unwrap_or_else(|| panic!("portable comminution mass overflowed"))
    );
    let mill_assembly = mill
        .assembly_profile()
        .unwrap_or_else(|| panic!("settlement comminution mill lost its assembly profile"));
    assert_eq!(
        mill_assembly
            .inputs()
            .iter()
            .find(|input| {
                input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            })
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(20_000))
    );
    let mill_service = mill
        .maintenance_profile()
        .unwrap_or_else(|| panic!("settlement comminution mill lost maintenance"));
    assert_eq!(
        mill_service.replacement(),
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
    );
    assert_eq!(
        mill_service.full_service_replacement_mass(),
        Mass::from_milligrams(3_200_000)
    );
    assert!(
        mill.capabilities()
            .get_capability(CAPABILITY_SCREEN_FLOW)
            .is_none()
    );
    assert!(
        mill.capabilities()
            .get_capability(CAPABILITY_SEPARATOR_FLOW)
            .is_none()
    );

    let table = definition(EQUIPMENT_TIMBER_ORE_DRESSING_TABLE);
    let screen = definition(EQUIPMENT_COPPER_PLATE_SIZING_SCREEN);
    let separator = definition(EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR);
    assert!(!table.requires_structural_support());
    assert_eq!(table.mass(), Mass::from_milligrams(5_038_000));
    assert_eq!(
        capability(EQUIPMENT_COPPER_PLATE_SIZING_SCREEN, CAPABILITY_SCREEN_FLOW).compare(
            capability(EQUIPMENT_TIMBER_ORE_DRESSING_TABLE, CAPABILITY_SCREEN_FLOW)
        ),
        Some(Ordering::Less)
    );
    assert_eq!(
        capability(
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            CAPABILITY_SEPARATOR_FLOW
        )
        .compare(capability(
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
            CAPABILITY_SEPARATOR_FLOW
        )),
        Some(Ordering::Less)
    );
    assert!(
        table.mass()
            > screen
                .mass()
                .checked_add(separator.mass())
                .unwrap_or_else(|| panic!("portable dressing mass overflowed"))
    );
    let table_assembly = table
        .assembly_profile()
        .unwrap_or_else(|| panic!("settlement ore-dressing table lost its assembly profile"));
    for (commodity, expected) in [
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            Mass::from_milligrams(18_000),
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
        ),
    ] {
        assert_eq!(
            table_assembly
                .inputs()
                .iter()
                .find(|input| input.commodity() == commodity)
                .map(|input| input.mass()),
            Some(expected)
        );
    }
    let table_service = table
        .maintenance_profile()
        .unwrap_or_else(|| panic!("settlement ore-dressing table lost maintenance"));
    assert_eq!(
        table_service.replacement(),
        CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL)
    );
    assert_eq!(
        table_service.full_service_replacement_mass(),
        Mass::from_milligrams(1_400_000)
    );
    assert!(
        table
            .capabilities()
            .get_capability(CAPABILITY_CRUSHER_FLOW)
            .is_none()
    );
    assert!(
        table
            .capabilities()
            .get_capability(CAPABILITY_GRINDER_FLOW)
            .is_none()
    );
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
    assert_eq!(assembly.input_mass(), Mass::from_milligrams(3_000_000));
    assert_eq!(
        assembly
            .inputs()
            .iter()
            .find(|input| { input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_FLYWHEEL) })
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(2_000_000))
    );
    assert_eq!(
        assembly
            .inputs()
            .iter()
            .find(|input| input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(800_000))
    );
    assert!(
        assembly
            .inputs()
            .iter()
            .all(|input| input.commodity().material() == MATERIAL_WOOD),
        "the timber treadle must no longer hide a stone-flywheel dependency"
    );
}

#[test]
fn walking_wheel_trades_bulk_timber_for_copper_free_full_body_power() {
    let registry = build_equipment_registry();
    let definition = |equipment| {
        registry.get_equipment(equipment).unwrap_or_else(|| {
            panic!(
                "primitive power equipment {} disappeared",
                equipment.value()
            )
        })
    };
    let power = |equipment, capability| {
        definition(equipment)
            .capabilities()
            .get_capability(capability)
            .unwrap_or_else(|| {
                panic!(
                    "primitive power equipment {} lost capability {}",
                    equipment.value(),
                    capability.value()
                )
            })
    };

    let treadle = definition(EQUIPMENT_TIMBER_TREADLE_DRIVE);
    let walking = definition(EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE);
    let copper = definition(EQUIPMENT_COPPER_REINFORCED_HAND_CRANK);
    assert_eq!(
        power(
            EQUIPMENT_TIMBER_TREADLE_DRIVE,
            CAPABILITY_TREADLE_POWER_OUTPUT
        )
        .compare(power(
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            CAPABILITY_WALKING_WHEEL_POWER_OUTPUT
        )),
        Some(Ordering::Less)
    );
    assert_eq!(
        power(
            EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
            CAPABILITY_WALKING_WHEEL_POWER_OUTPUT
        ),
        power(
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            CAPABILITY_MANUAL_POWER_OUTPUT
        ),
        "the walking wheel buys copper-crank peak power with a much larger timber investment"
    );
    assert_eq!(walking.mass(), Mass::from_milligrams(6_400_000));
    assert!(walking.mass() > treadle.mass());
    assert!(walking.mass() > copper.mass());
    let assembly = walking
        .assembly_profile()
        .unwrap_or_else(|| panic!("walking-wheel drive lost its assembly profile"));
    assert!(assembly.inputs().iter().all(|input| {
        input.commodity().material() == MATERIAL_WOOD
            && input.commodity().material() != MATERIAL_COPPER
    }));
    assert_eq!(
        assembly
            .inputs()
            .iter()
            .find(|input| input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(4_000_000))
    );
    let maintenance = walking
        .maintenance_profile()
        .unwrap_or_else(|| panic!("walking-wheel drive lost maintenance"));
    assert_eq!(
        maintenance.replacement(),
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE)
    );
    assert_eq!(
        maintenance.full_service_replacement_mass(),
        Mass::from_milligrams(400_000)
    );
    assert_eq!(
        resolve_equipment_capability(
            walking,
            condition(500_000),
            CAPABILITY_WALKING_WHEEL_POWER_OUTPUT
        ),
        Some(CapabilityValue::Power(
            crate::core::quantity::Power::from_microwatts(75_000_000)
        ))
    );
}

#[test]
fn treadle_hammer_is_a_bulk_material_copper_working_investment() {
    let registry = build_equipment_registry();
    let hammer = registry
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_HAMMER)
        .unwrap_or_else(|| panic!("timber treadle hammer disappeared"));
    assert_eq!(hammer.mass(), Mass::from_milligrams(4_400_000));
    assert_eq!(
        hammer
            .capabilities()
            .get_capability(CAPABILITY_COPPER_HAMMERING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(400)
        ))
    );
    assert_eq!(
        resolve_equipment_capability(hammer, condition(500_000), CAPABILITY_COPPER_HAMMERING_FLOW),
        Some(CapabilityValue::MassFlow(
            crate::core::quantity::MassFlow::from_milligrams_per_second(200)
        ))
    );
    let assembly = hammer
        .assembly_profile()
        .unwrap_or_else(|| panic!("timber treadle hammer lost its assembly profile"));
    assert!(
        assembly
            .inputs()
            .iter()
            .all(|input| input.commodity().material() != MATERIAL_COPPER),
        "the human-powered hammer must not hide a copper prerequisite before it can improve copper working"
    );
    assert_eq!(
        assembly
            .inputs()
            .iter()
            .find(|input| input.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
            .map(|input| input.mass()),
        Some(Mass::from_milligrams(3_200_000))
    );
    let maintenance = hammer
        .maintenance_profile()
        .unwrap_or_else(|| panic!("timber treadle hammer lost maintenance"));
    assert_eq!(
        maintenance.replacement(),
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL)
    );
    assert_eq!(
        maintenance.full_service_replacement_mass(),
        Mass::from_milligrams(800_000)
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
        (
            EQUIPMENT_STONE_ROTARY_QUERN,
            CAPABILITY_GRINDER_BATCH,
            Mass::from_milligrams(500_000),
            Mass::from_milligrams(250_000),
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            CAPABILITY_GRINDER_BATCH,
            Mass::from_milligrams(750_000),
            Mass::from_milligrams(375_000),
        ),
        (
            EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
            CAPABILITY_SCREEN_BATCH,
            Mass::from_milligrams(250_000),
            Mass::from_milligrams(125_000),
        ),
        (
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            CAPABILITY_SCREEN_BATCH,
            Mass::from_milligrams(500_000),
            Mass::from_milligrams(250_000),
        ),
        (
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            CAPABILITY_CRUSHER_BATCH,
            Mass::from_milligrams(2_000_000),
            Mass::from_milligrams(1_000_000),
        ),
        (
            EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
            CAPABILITY_GRINDER_BATCH,
            Mass::from_milligrams(1_000_000),
            Mass::from_milligrams(500_000),
        ),
        (
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
            CAPABILITY_SCREEN_BATCH,
            Mass::from_milligrams(1_000_000),
            Mass::from_milligrams(500_000),
        ),
        (
            EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
            CAPABILITY_SEPARATOR_BATCH,
            Mass::from_milligrams(1_250_000),
            Mass::from_milligrams(625_000),
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
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        EQUIPMENT_TIMBER_HELVE_HAMMER,
        EQUIPMENT_TIMBER_SASH_SAWMILL,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_STONE_ROTARY_QUERN,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
        EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
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
