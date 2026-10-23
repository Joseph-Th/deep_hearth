//! Exact built-in material facts for non-copper engineering metals.

use super::*;
use crate::content::textures::build_texture_registry;
use crate::content::{
    PROCESS_CAST_IRON_INGOT, PROCESS_CAST_TIN_INGOT, PROCESS_CAST_ZINC_INGOT,
    PROCESS_MELT_RECYCLED_IRON, PROCESS_MELT_RECYCLED_TIN, PROCESS_MELT_RECYCLED_ZINC,
    build_registries,
};
use crate::registry::{CommoditySource, CommodityUse};

#[test]
fn iron_tin_and_zinc_have_distinct_physical_properties_and_recoverable_forms() {
    let materials = build_material_registry();
    for (id, name, density, heat, melting, latent, compression, tension) in [
        (
            MATERIAL_IRON,
            "iron",
            7_870,
            449,
            1_811_000,
            247_000,
            250_000,
            200_000,
        ),
        (
            MATERIAL_TIN,
            "tin",
            7_310,
            227,
            505_080,
            59_000,
            15_000,
            50_000,
        ),
        (
            MATERIAL_ZINC,
            "zinc",
            7_140,
            388,
            692_680,
            112_000,
            37_000,
            108_000,
        ),
    ] {
        let def = materials.get_material(id).unwrap();
        assert_eq!(def.name(), name);
        let props = def.properties();
        assert_eq!(props.density_kg_per_m3(), density);
        assert_eq!(props.thermal().specific_heat_j_per_kg_k(), heat);
        assert_eq!(
            props.thermal().melting_point().unwrap().millikelvin(),
            melting
        );
        assert_eq!(
            props.thermal().fusion().unwrap().latent_heat_j_per_kg(),
            latent
        );
        let strength = props.structural().unwrap();
        assert_eq!(strength.compressive_strength_kpa(), compression);
        assert_eq!(strength.tensile_strength_kpa(), tension);
        for form in [FORM_INGOT, FORM_BLOCK, FORM_SCRAP, FORM_CHIP] {
            let key = CommodityKey::new(id, form);
            assert!(materials.has_commodity(key));
            assert_eq!(
                materials.is_residual_commodity(key),
                [FORM_SCRAP, FORM_CHIP].contains(&form)
            );
        }
    }
}

#[test]
fn all_three_metal_scrap_streams_have_remelting_and_ingot_casting_processes() {
    let registries = build_registries();
    for (material, remelt, cast) in [
        (
            MATERIAL_IRON,
            PROCESS_MELT_RECYCLED_IRON,
            PROCESS_CAST_IRON_INGOT,
        ),
        (
            MATERIAL_TIN,
            PROCESS_MELT_RECYCLED_TIN,
            PROCESS_CAST_TIN_INGOT,
        ),
        (
            MATERIAL_ZINC,
            PROCESS_MELT_RECYCLED_ZINC,
            PROCESS_CAST_ZINC_INGOT,
        ),
    ] {
        let process = registries.thermal().get_melting(remelt).unwrap();
        assert_eq!(process.material(), material);
        assert_eq!(
            process.solid_forms(),
            &[FORM_INGOT, FORM_CHIP, FORM_SCRAP, FORM_BLOCK]
        );
        assert_eq!(process.liquid_form(), FORM_MOLTEN);
        assert!(registries.production().get_process(remelt).is_some());
        assert!(registries.production().get_process(cast).is_some());
        let casting = registries.thermal().get_casting(cast).unwrap();
        assert_eq!(casting.material(), material);
        assert_eq!(casting.liquid_form(), FORM_MOLTEN);
        assert_eq!(casting.solid_form(), FORM_INGOT);
        assert_eq!(casting.output_temperature().millikelvin(), 293_150);
        let scrap = registries
            .commodity_handbook_entry(CommodityKey::new(material, FORM_SCRAP))
            .unwrap();
        assert!(scrap.uses().iter().any(|usage| matches!(
            usage, CommodityUse::ThermalPhaseChange { process } if *process == remelt
        )));
        let molten = registries
            .commodity_handbook_entry(CommodityKey::new(material, FORM_MOLTEN))
            .unwrap();
        assert!(molten.sources().iter().any(|source| matches!(
            source, CommoditySource::ThermalPhaseChange { process } if *process == remelt
        )));
        assert!(molten.uses().iter().any(|usage| matches!(
            usage, CommodityUse::ThermalPhaseChange { process } if *process == cast
        )));
        let ingot = registries
            .commodity_handbook_entry(CommodityKey::new(material, FORM_INGOT))
            .unwrap();
        assert!(ingot.sources().iter().any(|source| matches!(
            source, CommoditySource::ThermalPhaseChange { process } if *process == cast
        )));
    }
}

#[test]
fn oxide_and_sulfide_ores_are_not_elemental_melting_inputs() {
    let registries = build_registries();
    for (ore, metal, process_id) in [
        (MATERIAL_HEMATITE, MATERIAL_IRON, PROCESS_MELT_RECYCLED_IRON),
        (
            MATERIAL_CASSITERITE,
            MATERIAL_TIN,
            PROCESS_MELT_RECYCLED_TIN,
        ),
        (
            MATERIAL_SPHALERITE,
            MATERIAL_ZINC,
            PROCESS_MELT_RECYCLED_ZINC,
        ),
    ] {
        assert!(
            registries
                .materials()
                .has_commodity(CommodityKey::new(ore, FORM_ORE))
        );
        assert!(
            registries
                .materials()
                .has_commodity(CommodityKey::new(ore, FORM_CRUSHED))
        );
        assert!(
            !registries
                .materials()
                .has_commodity(CommodityKey::new(ore, FORM_MOLTEN))
        );
        assert_eq!(
            registries
                .thermal()
                .get_melting(process_id)
                .unwrap()
                .material(),
            metal
        );
        assert_ne!(ore, metal);
    }
}

#[test]
fn recovered_iron_has_a_conserved_toolhead_path_and_a_repairable_mining_tool() {
    use crate::capability::CapabilityValue;
    use crate::content::capabilities::{
        CAPABILITY_METAL_ABRASIVE_FINISHING_FLOW, CAPABILITY_MINING_FLOW,
        CAPABILITY_MINING_MAX_HARDNESS,
    };
    use crate::content::{
        EQUIPMENT_IRON_PICK, EQUIPMENT_STONE_PICK, EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        PROCESS_FINISH_IRON_PICK_HEAD_FROM_INGOT, PROCESS_FINISH_IRON_PICK_HEAD_FROM_SCRAP,
    };
    use crate::core::quantity::{Mass, Pressure};
    use crate::crafting::ManualCraftOutput;

    let registry = build_registries();
    for (process, input_form) in [
        (PROCESS_FINISH_IRON_PICK_HEAD_FROM_SCRAP, FORM_SCRAP),
        (PROCESS_FINISH_IRON_PICK_HEAD_FROM_INGOT, FORM_INGOT),
    ] {
        let recipe = registry.crafting().get_manual(process).unwrap();
        assert_eq!(recipe.input(), CommodityKey::new(MATERIAL_IRON, input_form));
        assert_eq!(recipe.input_mass(), Mass::from_milligrams(1_000_000));
        assert_eq!(
            recipe.outputs(),
            &[
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_IRON, FORM_TOOL),
                    Mass::from_milligrams(800_000)
                ),
                ManualCraftOutput::new(
                    CommodityKey::new(MATERIAL_IRON, FORM_CHIP),
                    Mass::from_milligrams(200_000)
                ),
            ]
        );
        let profile = recipe.equipment_profile().unwrap();
        assert!(profile.requires_equipment());
        assert_eq!(
            profile.mass_flow_capability(),
            CAPABILITY_METAL_ABRASIVE_FINISHING_FLOW
        );
        assert!(registry.production().get_process(process).is_some());
    }
    let grinder = registry
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_GRINDSTONE)
        .unwrap();
    assert!(
        grinder
            .capabilities()
            .get_capability(CAPABILITY_METAL_ABRASIVE_FINISHING_FLOW)
            .is_some()
    );
    let pick = registry
        .equipment()
        .get_equipment(EQUIPMENT_IRON_PICK)
        .unwrap();
    let stone = registry
        .equipment()
        .get_equipment(EQUIPMENT_STONE_PICK)
        .unwrap();
    assert_eq!(pick.mass(), Mass::from_milligrams(1_000_000));
    assert!(matches!(
        pick.capabilities().get_capability(CAPABILITY_MINING_MAX_HARDNESS),
        Some(CapabilityValue::Pressure(value)) if value == Pressure::from_pascals(900_000_000)
    ));
    assert!(matches!(
        pick.capabilities().get_capability(CAPABILITY_MINING_FLOW),
        Some(CapabilityValue::MassFlow(value)) if value.milligrams_per_second() == 55_000
    ));
    assert_ne!(pick.capabilities(), stone.capabilities());
    let assembly = pick.assembly_profile().unwrap();
    assert!(assembly.inputs().iter().any(|input| input.commodity()
        == CommodityKey::new(MATERIAL_IRON, FORM_TOOL)
        && input.mass() == Mass::from_milligrams(800_000)));
    let maintenance = pick.maintenance_profile().unwrap();
    assert_eq!(
        maintenance.replacement(),
        CommodityKey::new(MATERIAL_IRON, FORM_TOOL)
    );
    assert_eq!(
        registry
            .textures()
            .get_equipment_appearance(EQUIPMENT_IRON_PICK)
            .is_some(),
        true
    );
    let head = registry
        .textures()
        .get_commodity_appearance(CommodityKey::new(MATERIAL_IRON, FORM_TOOL))
        .unwrap();
    assert!(
        registry
            .textures()
            .get_object(head.object().unwrap())
            .is_some()
    );
}

#[test]
fn industrial_temperature_range_admits_iron_while_primitive_furnaces_do_not() {
    use crate::capability::CapabilityValue;
    use crate::content::capabilities::CAPABILITY_THERMAL_MAX_TEMPERATURE;
    use crate::content::{
        EQUIPMENT_CASTING_MOLD, EQUIPMENT_ELECTRIC_FURNACE, EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
    };
    let registry = build_registries();
    let temp = |equipment| {
        let value = registry
            .equipment()
            .get_equipment(equipment)
            .unwrap()
            .capabilities()
            .get_capability(CAPABILITY_THERMAL_MAX_TEMPERATURE)
            .unwrap();
        match value {
            CapabilityValue::Temperature(temperature) => temperature,
            other => panic!("expected temperature limit, got {other:?}"),
        }
    };
    let iron = registry
        .materials()
        .get_material(MATERIAL_IRON)
        .unwrap()
        .properties()
        .thermal()
        .melting_point()
        .unwrap();
    assert!(
        temp(EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE) < iron,
        "primitive furnace cannot directly remelt iron"
    );
    assert!(
        temp(EQUIPMENT_ELECTRIC_FURNACE) > iron,
        "industrial furnace must reach actual iron fusion temperature"
    );
    assert!(
        temp(EQUIPMENT_CASTING_MOLD) > iron,
        "industrial mold must tolerate newly molten iron"
    );
}

#[test]
fn metal_stock_and_excavated_scrap_have_separate_renderable_object_bindings() {
    let materials = build_material_registry();
    let textures = build_texture_registry();
    for metal in [MATERIAL_IRON, MATERIAL_TIN, MATERIAL_ZINC] {
        let binding = |form| {
            textures
                .get_commodity_appearance(CommodityKey::new(metal, form))
                .unwrap()
        };
        let stock = binding(FORM_BLOCK);
        let scrap = binding(FORM_SCRAP);
        assert_eq!(stock.object(), binding(FORM_INGOT).object());
        assert_eq!(scrap.object(), binding(FORM_CHIP).object());
        assert_ne!(stock.object(), scrap.object());
        assert!(textures.get_object(stock.object().unwrap()).is_some());
        assert!(textures.get_object(scrap.object().unwrap()).is_some());
        assert!(materials.is_residual_commodity(CommodityKey::new(metal, FORM_SCRAP)));
    }
}
