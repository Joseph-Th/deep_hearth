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
