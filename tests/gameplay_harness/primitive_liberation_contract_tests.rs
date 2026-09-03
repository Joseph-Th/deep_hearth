//! Primitive ore-liberation content topology contracts.

use deep_hearth::content::{
    EQUIPMENT_COPPER_PLATE_SIZING_SCREEN, EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    EQUIPMENT_STONE_ROTARY_QUERN, FORM_REINFORCEMENT, FORM_SCRAP, FORM_SCREEN_PLATE,
    MATERIAL_COPPER, PROCESS_CONCENTRATE_COPPER, PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
    PROCESS_GRIND_CRUSHED_ORE, PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_SCREEN_CRUSHED_ORE,
    build_registries,
};
use deep_hearth::material::CommodityKey;

use super::catalog::{ProcessResolverKind, process_catalog_entries};

#[test]
fn primitive_liberation_content_closes_the_pre_smelting_processing_gap() {
    let registries = build_registries();
    let plate = registries
        .crafting()
        .get_manual(PROCESS_PIERCE_COPPER_SCREEN_PLATE)
        .unwrap_or_else(|| panic!("copper sizing-plate craft disappeared"));
    assert_eq!(
        plate.input(),
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
    );
    assert!(!plate.input_mass().is_zero());
    assert_eq!(
        plate
            .outputs()
            .iter()
            .map(|output| output.mass())
            .try_fold(deep_hearth::core::quantity::Mass::ZERO, |total, mass| total
                .checked_add(mass)),
        Some(plate.input_mass()),
        "sizing-plate piercing must conserve copper between the plate and offcut scrap"
    );
    let plate_output = plate
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE))
        .unwrap_or_else(|| panic!("sizing-plate craft lost its screen-plate output"));
    assert!(!plate_output.mass().is_zero());
    assert!(plate.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP)
            && !output.mass().is_zero()
    }));

    let screen = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_PLATE_SIZING_SCREEN)
        .unwrap_or_else(|| panic!("primitive sizing screen disappeared"));
    assert!(screen.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().any(|input| {
            input.commodity() == plate_output.commodity() && input.mass() == plate_output.mass()
        })
    }));
    let quern = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_ROTARY_QUERN)
        .unwrap_or_else(|| panic!("stone rotary quern disappeared"));
    let reinforced = registries
        .equipment()
        .get_equipment(EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN)
        .unwrap_or_else(|| panic!("reinforced stone rotary quern disappeared"));
    assert!(quern.assembly_profile().is_some());
    assert_eq!(
        reinforced.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_STONE_ROTARY_QUERN)
    );

    let catalog = process_catalog_entries(&registries);
    for process in [
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_CONCENTRATE_COPPER,
    ] {
        let entry = catalog
            .iter()
            .find(|entry| entry.process == process)
            .unwrap_or_else(|| panic!("primitive liberation process disappeared from catalog"));
        assert!(
            entry.nominal_provider_count >= 2,
            "process {} must have both primitive and later machinery available through canonical capability discovery",
            process.value()
        );
        assert!(entry.compatible_energy_store_count > 0);
        assert!(!matches!(
            entry.resolver,
            ProcessResolverKind::ManualCraft
                | ProcessResolverKind::ManualComminution
                | ProcessResolverKind::ManualSeparation
        ));
    }
}
