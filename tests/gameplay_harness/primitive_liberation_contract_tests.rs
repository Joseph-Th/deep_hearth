//! Primitive ore-liberation content topology contracts.

use deep_hearth::content::{
    EQUIPMENT_COPPER_PLATE_SIZING_SCREEN, EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
    EQUIPMENT_STONE_ROTARY_QUERN, EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN, FORM_BOARD, FORM_CHIP,
    FORM_EXHAUSTED_TAILINGS, FORM_REINFORCEMENT, FORM_SCRAP, FORM_SCREEN_PLATE, FORM_TAILINGS,
    FORM_TIMBER_RIDDLE_PANEL, MATERIAL_COPPER, MATERIAL_WOOD, PROCESS_CONCENTRATE_COPPER,
    PROCESS_FINE_GRIND_SCREEN_OVERSIZE, PROCESS_GRIND_CRUSHED_ORE,
    PROCESS_PIERCE_COPPER_SCREEN_PLATE, PROCESS_REGRIND_COPPER_TAILINGS,
    PROCESS_SCAVENGE_COPPER_TAILINGS, PROCESS_SCREEN_CRUSHED_ORE,
    PROCESS_SHAPE_TIMBER_RIDDLE_PANEL, build_registries,
};
use deep_hearth::material::CommodityKey;

use super::catalog::{ProcessResolverKind, process_catalog_entries};

#[test]
fn primitive_liberation_content_closes_the_pre_smelting_processing_gap() {
    let registries = build_registries();
    let riddle_panel = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_TIMBER_RIDDLE_PANEL)
        .unwrap_or_else(|| panic!("timber riddle-panel craft disappeared"));
    assert_eq!(
        riddle_panel.input(),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
    );
    let panel_output = riddle_panel
        .outputs()
        .iter()
        .find(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_TIMBER_RIDDLE_PANEL)
        })
        .unwrap_or_else(|| panic!("timber riddle-panel craft lost its panel output"));
    let chip_output = riddle_panel
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))
        .unwrap_or_else(|| panic!("timber riddle-panel craft lost its chip byproduct"));
    assert!(!panel_output.mass().is_zero());
    assert_eq!(
        panel_output.mass().checked_add(chip_output.mass()),
        Some(riddle_panel.input_mass()),
        "riddle-panel shaping must conserve timber while making the sizing aperture explicit"
    );

    let timber_screen = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN)
        .unwrap_or_else(|| panic!("timber riddle sizing screen disappeared"));
    assert!(timber_screen.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().any(|input| {
            input.commodity() == panel_output.commodity() && input.mass() == panel_output.mass()
        })
    }));

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
    assert_eq!(
        screen.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN),
        "scarce copper should improve the already-built sizing tool rather than discard it"
    );
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

    let primary_concentration = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("primary copper concentration disappeared"));
    let tailings_regrind = registries
        .ore_processing()
        .get_comminution(PROCESS_REGRIND_COPPER_TAILINGS)
        .unwrap_or_else(|| panic!("tailings regrind disappeared"));
    let scavenger = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SCAVENGE_COPPER_TAILINGS)
        .unwrap_or_else(|| panic!("tailings scavenger disappeared"));
    assert_eq!(primary_concentration.residue_output_form(), FORM_TAILINGS);
    assert_eq!(tailings_regrind.input_form(), FORM_TAILINGS);
    assert_eq!(tailings_regrind.output_form(), FORM_TAILINGS);
    assert_eq!(scavenger.input_form(), FORM_TAILINGS);
    assert_eq!(scavenger.residue_output_form(), FORM_EXHAUSTED_TAILINGS);
    let primary_range = primary_concentration
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("primary concentration lost its particle envelope"));
    let scavenger_range = scavenger
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("tailings scavenger lost its particle envelope"));
    assert!(
        scavenger_range.maximum_diameter() < primary_range.minimum_diameter(),
        "tailings scavenging must require a genuinely finer liberation step"
    );
    assert_eq!(tailings_regrind.output_particle_size(), scavenger_range);
    assert!(
        scavenger.target_recovery_ppm() < primary_concentration.target_recovery_ppm(),
        "scavenging must remain secondary recovery rather than replace primary concentration"
    );

    let catalog = process_catalog_entries(&registries);
    for process in [
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_CONCENTRATE_COPPER,
        PROCESS_REGRIND_COPPER_TAILINGS,
        PROCESS_SCAVENGE_COPPER_TAILINGS,
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
