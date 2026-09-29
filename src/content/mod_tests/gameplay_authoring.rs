//! Built-in gameplay authoring and ordinary progression contract tests.

use super::*;
use crate::crafting::ManualCraftOutput;

#[test]
fn phase_change_definitions_require_authored_phase_directions() {
    for thermal in [
        ThermalRegistry::new(
            std::iter::empty(),
            [MeltingProcessDefinition::new(
                TEST_PROCESS,
                PhaseChangeProcessProfile::new(
                    TEST_HEATING_POWER,
                    TEST_MAX_TEMPERATURE,
                    TEST_MAX_BATCH_MASS,
                    EnergyCarrier::Electrical,
                    1,
                ),
                MATERIAL_COPPER,
                vec![FORM_MOLTEN],
                FORM_MOLTEN,
            )],
            std::iter::empty(),
        ),
        ThermalRegistry::new(
            std::iter::empty(),
            [MeltingProcessDefinition::new(
                TEST_PROCESS,
                PhaseChangeProcessProfile::new(
                    TEST_HEATING_POWER,
                    TEST_MAX_TEMPERATURE,
                    TEST_MAX_BATCH_MASS,
                    EnergyCarrier::Electrical,
                    1,
                ),
                MATERIAL_COPPER,
                vec![FORM_INGOT],
                FORM_INGOT,
            )],
            std::iter::empty(),
        ),
        ThermalRegistry::new(
            std::iter::empty(),
            std::iter::empty(),
            [CastingProcessDefinition::new(
                TEST_PROCESS,
                PhaseChangeProcessProfile::new(
                    TEST_HEATING_POWER,
                    TEST_MAX_TEMPERATURE,
                    TEST_MAX_BATCH_MASS,
                    EnergyCarrier::Thermal,
                    1,
                ),
                MATERIAL_COPPER,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_INGOT, FORM_INGOT),
                    Temperature::from_millikelvin(300_000),
                ),
            )],
        ),
        ThermalRegistry::new(
            std::iter::empty(),
            std::iter::empty(),
            [CastingProcessDefinition::new(
                TEST_PROCESS,
                PhaseChangeProcessProfile::new(
                    TEST_HEATING_POWER,
                    TEST_MAX_TEMPERATURE,
                    TEST_MAX_BATCH_MASS,
                    EnergyCarrier::Thermal,
                    1,
                ),
                MATERIAL_COPPER,
                CastingPhaseChange::new(
                    PhaseChangeForms::new(FORM_MOLTEN, FORM_MOLTEN),
                    Temperature::from_millikelvin(300_000),
                ),
            )],
        ),
    ] {
        assert_thermal_reference_validation_rejects(thermal);
    }
}

#[test]
fn clay_infrastructure_adds_copper_sparing_storage_and_casting_with_reclaimable_upkeep() {
    let registries = build_registries();

    let casting_bed = registries
        .equipment()
        .get_equipment(EQUIPMENT_CLAY_FACED_STONE_CASTING_BED)
        .unwrap_or_else(|| panic!("clay-faced stone casting bed disappeared"));
    let copper_mold = registries
        .equipment()
        .get_equipment(EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD)
        .unwrap_or_else(|| panic!("four-cavity stone ingot mold disappeared"));
    assert_eq!(
        casting_bed
            .capabilities()
            .get_capability(capabilities::CAPABILITY_THERMAL_BATCH),
        Some(CapabilityValue::Mass(Mass::from_milligrams(80_000)))
    );
    assert_eq!(
        casting_bed
            .capabilities()
            .get_capability(capabilities::CAPABILITY_COOLING_POWER),
        Some(CapabilityValue::Power(Power::from_microwatts(250_000_000)))
    );
    assert_eq!(
        copper_mold
            .capabilities()
            .get_capability(capabilities::CAPABILITY_COOLING_POWER),
        Some(CapabilityValue::Power(Power::from_microwatts(400_000_000)))
    );
    let casting_assembly = casting_bed
        .assembly_profile()
        .unwrap_or_else(|| panic!("clay-faced casting bed lost its assembly profile"));
    assert!(
        casting_assembly
            .inputs()
            .iter()
            .all(|input| { input.commodity().material() != MATERIAL_COPPER })
    );
    assert!(casting_assembly.inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
            && input.mass() == Mass::from_milligrams(6_000_000)
    }));
    assert!(casting_assembly.inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER)
            && input.mass() == Mass::from_milligrams(2_000_000)
    }));
    let casting_service = casting_bed
        .maintenance_profile()
        .unwrap_or_else(|| panic!("clay-faced casting bed lost its binder service"));
    assert!(casting_service.is_component_replacement());
    assert_eq!(
        casting_service.replacement(),
        CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER)
    );
    assert_eq!(
        casting_service.full_service_replacement_mass(),
        Mass::from_milligrams(2_000_000)
    );
    assert_eq!(
        casting_service.spent(),
        CommodityKey::new(MATERIAL_CLAY, FORM_SCRAP)
    );

    let binder = registries
        .crafting()
        .get_manual(PROCESS_PACK_CLAY_BINDER)
        .unwrap_or_else(|| panic!("packed clay binder preparation disappeared"));
    assert_eq!(binder.input(), CommodityKey::new(MATERIAL_CLAY, FORM_LUMP));
    assert_eq!(binder.input_mass(), Mass::from_milligrams(500_000));
    assert_eq!(
        binder.outputs(),
        &[crate::crafting::ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER),
            Mass::from_milligrams(500_000),
        )]
    );
    let reclaim = registries
        .crafting()
        .get_manual(PROCESS_RECONDITION_CLAY_BINDER)
        .unwrap_or_else(|| panic!("spent clay binder reclamation disappeared"));
    assert_eq!(
        reclaim.input(),
        CommodityKey::new(MATERIAL_CLAY, FORM_SCRAP)
    );
    assert_eq!(reclaim.input_mass(), Mass::from_milligrams(1_000_000));
    assert_eq!(
        reclaim
            .outputs()
            .iter()
            .map(|output| output.mass().milligrams())
            .sum::<u64>(),
        1_000_000
    );
    assert!(reclaim.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_CLAY, FORM_PACKED_CLAY_BINDER)
            && output.mass() == Mass::from_milligrams(900_000)
    }));
    assert!(reclaim.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_CLAY, FORM_CHIP)
            && output.mass() == Mass::from_milligrams(100_000)
    }));
}

#[test]
fn settlement_batch_foundry_and_channel_sampling_add_distinct_connected_progression() {
    let registries = build_registries();

    let winding = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING)
        .unwrap_or_else(|| panic!("copper electrical-winding craft disappeared"));
    assert_eq!(
        winding.input(),
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
    );
    assert_eq!(winding.input_mass(), Mass::from_milligrams(60_000));
    assert_eq!(
        winding
            .outputs()
            .iter()
            .map(|output| output.mass().milligrams())
            .sum::<u64>(),
        60_000,
        "electrical winding fabrication must conserve its copper input"
    );
    assert!(winding.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING)
            && output.mass() == crafted_parts::COPPER_ELECTRICAL_WINDING_MASS
    }));
    let powered_winding = registries
        .crafting()
        .get_powered(PROCESS_POWER_HAMMER_COPPER_ELECTRICAL_WINDING)
        .unwrap_or_else(|| panic!("powered electrical-winding route disappeared"));
    assert_eq!(
        powered_winding.transform(),
        PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING
    );

    let dynamo = registries
        .equipment()
        .get_equipment(EQUIPMENT_DOUBLE_WOUND_TREADLE_DYNAMO)
        .unwrap_or_else(|| panic!("double-wound treadle dynamo disappeared"));
    assert_eq!(
        dynamo.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_TIMBER_TREADLE_DYNAMO)
    );
    assert_eq!(
        dynamo
            .capabilities()
            .get_capability(capabilities::CAPABILITY_TREADLE_DYNAMO_OUTPUT),
        Some(CapabilityValue::Power(Power::from_microwatts(150_000_000)))
    );

    for (equipment, from, transfer_capability, transfer_power) in [
        (
            EQUIPMENT_FOUR_POT_ARC_CRUCIBLE_FURNACE,
            EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
            capabilities::CAPABILITY_HEATING_POWER,
            300_000_000,
        ),
        (
            EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD,
            EQUIPMENT_STONE_INGOT_MOLD,
            capabilities::CAPABILITY_COOLING_POWER,
            400_000_000,
        ),
    ] {
        let definition = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("settlement foundry equipment disappeared"));
        assert_eq!(
            definition.upgrade_profile().map(|upgrade| upgrade.from()),
            Some(from)
        );
        assert_eq!(
            definition
                .capabilities()
                .get_capability(capabilities::CAPABILITY_THERMAL_BATCH),
            Some(CapabilityValue::Mass(Mass::from_milligrams(80_000)))
        );
        assert_eq!(
            definition
                .capabilities()
                .get_capability(transfer_capability),
            Some(CapabilityValue::Power(Power::from_microwatts(
                transfer_power
            )))
        );
    }

    let electrical = registries
        .energy()
        .get_store(ENERGY_COPPER_RACK_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("settlement electrical buffer disappeared"));
    assert_eq!(
        electrical.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
    );
    assert_eq!(
        electrical.capacity(),
        Energy::from_nanojoules(60_000_000_000_000)
    );
    assert_eq!(
        electrical.max_input_power(),
        Power::from_microwatts(150_000_000)
    );
    assert_eq!(
        electrical.max_output_power(),
        Power::from_microwatts(300_000_000)
    );

    let thermal = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_THERMAL_SINK)
        .unwrap_or_else(|| panic!("settlement thermal sink disappeared"));
    assert_eq!(
        thermal.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(ENERGY_STONE_THERMAL_SINK)
    );
    assert_eq!(
        thermal.capacity(),
        Energy::from_nanojoules(200_000_000_000_000)
    );
    assert_eq!(
        thermal.max_input_power(),
        Power::from_microwatts(400_000_000)
    );
    assert_eq!(
        thermal.passive_dissipation_power(),
        Power::from_microwatts(20_000_000)
    );

    let survey = registries
        .labor()
        .get_prospecting(PROSPECTING_CHANNEL_COMPOSITE_SURVEY)
        .unwrap_or_else(|| panic!("channel-composite survey disappeared"));
    assert_eq!(survey.maximum_region_voxels(), 16);
    assert_eq!(survey.abundance_uncertainty_ppm(), 12_500);
    assert_eq!(
        survey.spatial_resolution(),
        crate::labor::ProspectingSpatialResolution::AggregateRegion
    );
    assert_eq!(survey.excavation_hardness_resolution(), None);
    assert_eq!(survey.resource_mass_resolution(), None);
    let instrument = survey
        .equipment()
        .unwrap_or_else(|| panic!("channel-composite survey lost its physical instrument"));
    assert_eq!(
        instrument.primary(),
        EQUIPMENT_TIMBER_CHANNEL_SAMPLING_FRAME
    );
    assert_eq!(instrument.alternative(), None);

    let crock = registries
        .storage()
        .get(STORAGE_COPPER_BANDED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("copper-banded provisions crock disappeared"));
    let pantry = registries
        .storage()
        .get(STORAGE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated timber pantry disappeared"));
    assert_eq!(
        crock.maximum_stockpile_capacity(),
        Mass::from_milligrams(6_000_000)
    );
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            > pantry.storage_profile().preservation_multiplier_ppm(),
        "copper-banded crock must buy preservation rather than bulk capacity"
    );
    assert!(crock.maximum_stockpile_capacity() < pantry.maximum_stockpile_capacity());
    assert!(crock.assembly_profile().inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
            && input.mass() == Mass::from_milligrams(40_000)
    }));
}

#[test]
fn wire_drawbench_and_shallow_core_drill_are_specialist_settlement_investments() {
    let registries = build_registries();

    let hammered = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING)
        .unwrap_or_else(|| panic!("hammered electrical-winding route disappeared"));
    let drawn = registries
        .crafting()
        .get_manual(PROCESS_DRAW_COPPER_ELECTRICAL_WINDING)
        .unwrap_or_else(|| panic!("drawn electrical-winding route disappeared"));
    assert_eq!(hammered.input(), drawn.input());
    assert_eq!(hammered.input_mass(), drawn.input_mass());
    let hammered_winding = hammered
        .outputs()
        .iter()
        .find(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING)
        })
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("hammered winding output disappeared"));
    let drawn_winding = drawn
        .outputs()
        .iter()
        .find(|output| {
            output.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_ELECTRICAL_WINDING)
        })
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("drawn winding output disappeared"));
    assert_eq!(
        hammered_winding,
        crafted_parts::COPPER_ELECTRICAL_WINDING_MASS
    );
    assert_eq!(drawn_winding, Mass::from_milligrams(60_000));
    assert!(drawn_winding > hammered_winding);
    assert!(
        drawn
            .outputs()
            .iter()
            .all(|output| { output.commodity() != CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP) }),
        "specialist wire drawing should preserve copper that the general hammer route trims as scrap"
    );
    let draw_profile = drawn
        .equipment_profile()
        .unwrap_or_else(|| panic!("wire-drawing route lost its required equipment"));
    assert!(draw_profile.requires_equipment());
    assert_eq!(
        draw_profile.mass_flow_capability(),
        capabilities::CAPABILITY_COPPER_WIRE_DRAWING_FLOW
    );

    let drawplate = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_DRAWPLATE)
        .unwrap_or_else(|| panic!("stone drawplate fabrication disappeared"));
    assert_eq!(
        drawplate.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
    );
    assert!(drawplate.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE)
            && output.mass() == crafted_parts::STONE_DRAWPLATE_MASS
    }));
    assert_eq!(
        drawplate
            .outputs()
            .iter()
            .map(|output| output.mass().milligrams())
            .sum::<u64>(),
        drawplate.input_mass().milligrams()
    );

    let drawbench = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_WIRE_DRAWBENCH)
        .unwrap_or_else(|| panic!("wire drawbench disappeared"));
    assert_eq!(
        drawbench
            .capabilities()
            .get_capability(capabilities::CAPABILITY_COPPER_WIRE_DRAWING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(600)
        ))
    );
    assert_eq!(
        drawbench
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))
    );
    let one = std::num::NonZeroU64::new(1)
        .unwrap_or_else(|| unreachable!("one winding is a nonzero batch"));
    let hammer_work = crate::crafting::project_manual_craft_equipment(
        &registries,
        PROCESS_COLD_WORK_COPPER_ELECTRICAL_WINDING,
        one,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        crate::maintenance::Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("hammer winding projection failed: {error}"));
    let draw_work = crate::crafting::project_manual_craft_equipment(
        &registries,
        PROCESS_DRAW_COPPER_ELECTRICAL_WINDING,
        one,
        EQUIPMENT_TIMBER_WIRE_DRAWBENCH,
        crate::maintenance::Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("drawbench winding projection failed: {error}"));
    assert!(
        draw_work.duration() < hammer_work.duration(),
        "specialist drawbench must repay setup with lower active winding time"
    );

    let powered_drawing = registries
        .crafting()
        .get_powered(PROCESS_POWER_DRAW_COPPER_ELECTRICAL_WINDING)
        .unwrap_or_else(|| panic!("powered wire-drawing route disappeared"));
    assert_eq!(
        powered_drawing.transform(),
        PROCESS_DRAW_COPPER_ELECTRICAL_WINDING
    );
    let flywheel_drawbench = registries
        .equipment()
        .get_equipment(EQUIPMENT_FLYWHEEL_WIRE_DRAWBENCH)
        .unwrap_or_else(|| panic!("flywheel wire drawbench disappeared"));
    assert_eq!(
        flywheel_drawbench
            .upgrade_profile()
            .map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_TIMBER_WIRE_DRAWBENCH)
    );
    assert_eq!(
        flywheel_drawbench
            .capabilities()
            .get_capability(capabilities::CAPABILITY_COPPER_WIRE_DRAWING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(800)
        ))
    );
    assert_eq!(
        flywheel_drawbench
            .capabilities()
            .get_capability(capabilities::CAPABILITY_POWERED_COPPER_WIRE_DRAWING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(2_000)
        ))
    );
    assert_eq!(
        flywheel_drawbench
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_DRAWPLATE))
    );
    let upgrade = flywheel_drawbench
        .upgrade_profile()
        .unwrap_or_else(|| panic!("flywheel drawbench lost additive upgrade route"));
    assert!(upgrade.additions().inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL)
            && input.mass() == crafted_parts::STONE_FLYWHEEL_MASS
    }));

    let detailed = registries
        .labor()
        .get_prospecting(PROSPECTING_DETAILED_FIELD_SURVEY)
        .unwrap_or_else(|| panic!("detailed field survey disappeared"));
    let channel = registries
        .labor()
        .get_prospecting(PROSPECTING_CHANNEL_COMPOSITE_SURVEY)
        .unwrap_or_else(|| panic!("channel-composite survey disappeared"));
    let core = registries
        .labor()
        .get_prospecting(PROSPECTING_SHALLOW_CORE_SURVEY)
        .unwrap_or_else(|| panic!("shallow-core survey disappeared"));
    assert_eq!(
        core.evidence(),
        crate::geology::GeologicalEvidenceKind::CoreSample
    );
    assert_eq!(core.maximum_region_voxels(), 1);
    assert!(core.duration() > detailed.duration());
    assert!(core.abundance_uncertainty_ppm() < detailed.abundance_uncertainty_ppm());
    assert!(core.abundance_uncertainty_ppm() < channel.abundance_uncertainty_ppm());
    assert!(core.excavation_hardness_resolution() < detailed.excavation_hardness_resolution());
    assert!(core.resource_mass_resolution() < detailed.resource_mass_resolution());
    let core_instrument = core
        .equipment()
        .unwrap_or_else(|| panic!("shallow-core survey lost its instrument"));
    assert_eq!(
        core_instrument.primary(),
        EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL
    );
    assert_eq!(core_instrument.alternative(), None);

    let drill = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TRIPOD_CORE_DRILL)
        .unwrap_or_else(|| panic!("tripod core drill disappeared"));
    let drill_assembly = drill
        .assembly_profile()
        .unwrap_or_else(|| panic!("tripod core drill lost its assembly"));
    assert!(drill_assembly.inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
            && input.mass() == crafted_parts::STONE_DRILL_BIT_MASS
    }));
    assert!(drill_assembly.inputs().iter().any(|input| {
        input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL)
            && input.mass() == crafted_parts::STONE_FLYWHEEL_MASS
    }));
    assert_eq!(
        drill
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
    );
}

#[test]
fn built_in_workshop_ids_resolve_canonical_gameplay_content() {
    let registries = build_registries();

    for equipment in [
        EQUIPMENT_JAW_CRUSHER,
        EQUIPMENT_ELECTRIC_FURNACE,
        EQUIPMENT_CASTING_MOLD,
        EQUIPMENT_DRY_SCREEN,
        EQUIPMENT_GRAVITY_SEPARATOR,
        EQUIPMENT_GRINDING_MILL,
        EQUIPMENT_STONE_CRUSHER,
        EQUIPMENT_STONE_DIGGING_SHOVEL,
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_STONE_ROTARY_QUERN,
        EQUIPMENT_STONE_MORTAR_AND_PESTLE,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        EQUIPMENT_TIMBER_SPINDLE_DRILL,
        EQUIPMENT_TIMBER_SPRING_POLE_LATHE,
        EQUIPMENT_TIMBER_FLYWHEEL_LATHE,
        EQUIPMENT_TIMBER_TREADLE_GRINDSTONE,
        EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH,
        EQUIPMENT_TIMBER_RIDDLE_SIZING_SCREEN,
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_QUARRY_PICK,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        EQUIPMENT_TIMBER_WALKING_WHEEL_DRIVE,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
        EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL,
        EQUIPMENT_TIMBER_ORE_DRESSING_TABLE,
        EQUIPMENT_STONE_COBBING_HAMMER,
        EQUIPMENT_TIMBER_DRESSING_BENCH,
        EQUIPMENT_TIMBER_TREADLE_DYNAMO,
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_STONE_INGOT_MOLD,
    ] {
        assert!(registries.equipment().get_equipment(equipment).is_some());
    }

    let piercing = registries
        .crafting()
        .get_manual(PROCESS_PIERCE_COPPER_SCREEN_PLATE)
        .unwrap_or_else(|| panic!("copper screen-plate piercing disappeared"));
    let piercing_profile = piercing
        .equipment_profile()
        .unwrap_or_else(|| panic!("copper screen-plate piercing lost its physical drill"));
    assert!(piercing_profile.requires_equipment());
    assert_eq!(
        piercing_profile.mass_flow_capability(),
        capabilities::CAPABILITY_COPPER_PIERCING_FLOW
    );
    let drill = registries
        .equipment()
        .get_equipment(EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL)
        .unwrap_or_else(|| panic!("stone-flywheel pump drill disappeared"));
    assert_eq!(
        drill
            .capabilities()
            .get_capability(capabilities::CAPABILITY_COPPER_PIERCING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(250)
        ))
    );
    assert!(drill.assembly_profile().is_some_and(|assembly| {
        let has_flywheel = assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL)
                && input.mass() == crafted_parts::STONE_FLYWHEEL_MASS
        });
        let has_bit = assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
                && input.mass() == crafted_parts::STONE_DRILL_BIT_MASS
        });
        has_flywheel && has_bit
    }));
    let drill_maintenance = drill
        .maintenance_profile()
        .unwrap_or_else(|| panic!("pump drill lost replaceable bit maintenance"));
    assert_eq!(
        drill_maintenance.replacement(),
        CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
    );
    assert_eq!(
        drill_maintenance.full_service_replacement_mass(),
        crafted_parts::STONE_DRILL_BIT_MASS
    );
    let bit = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_DRILL_BIT)
        .unwrap_or_else(|| panic!("knapped pump-drill bit recipe disappeared"));
    assert_eq!(bit.input_mass(), Mass::from_milligrams(200_000));
    assert!(bit.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
            && output.mass() == crafted_parts::STONE_DRILL_BIT_MASS
    }));
    assert!(bit.outputs().iter().any(|output| {
        output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
            && output.mass() == Mass::from_milligrams(100_000)
    }));
    let recovered_bit = registries
        .crafting()
        .get_manual(PROCESS_DRESS_STONE_CHIP_DRILL_BIT)
        .unwrap_or_else(|| panic!("stone-chip drill-bit recovery route disappeared"));
    assert_eq!(
        recovered_bit.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP)
    );
    assert_eq!(
        recovered_bit.input_mass(),
        crafted_parts::STONE_DRILL_BIT_MASS
    );
    assert_eq!(recovered_bit.duration(), TickSpan::new(12));
    assert_eq!(recovered_bit.outputs().len(), 1);
    assert_eq!(
        recovered_bit.outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
    );
    assert_eq!(
        recovered_bit.outputs()[0].mass(),
        crafted_parts::STONE_DRILL_BIT_MASS
    );
    let bit_producers = registries
        .crafting()
        .manual_producers(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
        .map(crate::crafting::ManualCraftDefinition::process)
        .collect::<Vec<_>>();
    assert_eq!(
        bit_producers,
        vec![
            PROCESS_KNAP_STONE_DRILL_BIT,
            PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
            PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
        ]
    );
    let one = std::num::NonZeroU64::new(1)
        .unwrap_or_else(|| unreachable!("one screen plate is a nonzero batch"));
    assert_eq!(
        crate::crafting::project_manual_craft_hand_work(
            &registries,
            PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            one,
        ),
        Err(
            crate::crafting::ManualCraftHandProjectionError::EquipmentRequired {
                process: PROCESS_PIERCE_COPPER_SCREEN_PLATE,
            }
        )
    );
    let drilled = crate::crafting::project_manual_craft_equipment(
        &registries,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        one,
        EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL,
        crate::maintenance::Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("pump-drill screen-plate projection failed: {error}"));
    assert_eq!(drilled.duration(), TickSpan::new(23));
    assert!(drilled.condition_after() < crate::maintenance::Condition::PRISTINE);
    let spindle = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_SPINDLE_DRILL)
        .unwrap_or_else(|| panic!("timber spindle drill disappeared"));
    assert_eq!(
        spindle.upgrade_profile().map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_STONE_FLYWHEEL_PUMP_DRILL)
    );
    assert_eq!(
        spindle
            .capabilities()
            .get_capability(capabilities::CAPABILITY_POWERED_COPPER_PIERCING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(1_500)
        ))
    );
    assert!(spindle.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL)
                && input.mass() == crafted_parts::STONE_FLYWHEEL_MASS
        }) && assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT)
                && input.mass() == crafted_parts::STONE_DRILL_BIT_MASS
        })
    }));
    assert_eq!(
        spindle
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_DRILL_BIT))
    );

    for process in [PROCESS_SHAPE_WOOD_HANDLE, PROCESS_SHAPE_TIMBER_FLYWHEEL] {
        let turning = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("manual turning transform {} disappeared", process.value()));
        assert_eq!(
            turning
                .equipment_profile()
                .map(crate::crafting::ManualCraftEquipmentProfile::mass_flow_capability),
            Some(capabilities::CAPABILITY_WOOD_TURNING_FLOW)
        );
    }
    let pole_lathe = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_SPRING_POLE_LATHE)
        .unwrap_or_else(|| panic!("spring-pole lathe disappeared"));
    assert_eq!(
        pole_lathe
            .capabilities()
            .get_capability(capabilities::CAPABILITY_WOOD_TURNING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(25_000)
        ))
    );
    let flywheel_lathe = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FLYWHEEL_LATHE)
        .unwrap_or_else(|| panic!("flywheel lathe disappeared"));
    assert_eq!(
        flywheel_lathe
            .upgrade_profile()
            .map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_TIMBER_SPRING_POLE_LATHE)
    );
    assert_eq!(
        flywheel_lathe
            .capabilities()
            .get_capability(capabilities::CAPABILITY_POWERED_WOOD_TURNING_FLOW),
        Some(CapabilityValue::MassFlow(
            MassFlow::from_milligrams_per_second(100_000)
        ))
    );
    assert_eq!(
        flywheel_lathe
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_TOOL))
    );
    for process in [
        PROCESS_POWER_TURN_WOOD_HANDLE,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
    ] {
        let turning = registries
            .crafting()
            .get_powered(process)
            .unwrap_or_else(|| panic!("powered turning process {} disappeared", process.value()));
        assert_eq!(
            turning.mass_flow_capability(),
            capabilities::CAPABILITY_POWERED_WOOD_TURNING_FLOW
        );
        assert_eq!(turning.energy_carrier(), EnergyCarrier::Mechanical);
    }

    let grindstone = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_GRINDSTONE)
        .unwrap_or_else(|| panic!("treadle grindstone disappeared"));
    assert_eq!(
        grindstone
            .maintenance_profile()
            .map(|maintenance| maintenance.replacement()),
        Some(CommodityKey::new(MATERIAL_STONE, FORM_GRINDSTONE_WHEEL))
    );
    let grinding_bench = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FLYWHEEL_GRINDING_BENCH)
        .unwrap_or_else(|| panic!("flywheel grinding bench disappeared"));
    assert_eq!(
        grinding_bench
            .upgrade_profile()
            .map(|upgrade| upgrade.from()),
        Some(EQUIPMENT_TIMBER_TREADLE_GRINDSTONE)
    );
    for process in [
        PROCESS_GRIND_STONE_SCRAP_TOOL,
        PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
    ] {
        let grinding = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("manual grinding process {} disappeared", process.value()));
        assert_eq!(
            grinding
                .equipment_profile()
                .map(crate::crafting::ManualCraftEquipmentProfile::mass_flow_capability),
            Some(capabilities::CAPABILITY_STONE_GRINDING_FLOW)
        );
    }
    for process in [
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
    ] {
        let grinding = registries
            .crafting()
            .get_powered(process)
            .unwrap_or_else(|| panic!("powered grinding process {} disappeared", process.value()));
        assert_eq!(
            grinding.mass_flow_capability(),
            capabilities::CAPABILITY_POWERED_STONE_GRINDING_FLOW
        );
        assert_eq!(grinding.energy_carrier(), EnergyCarrier::Mechanical);
    }

    for prospecting in [
        PROSPECTING_REGIONAL_RECONNAISSANCE,
        PROSPECTING_LOCAL_TRANSECT,
        PROSPECTING_FIELD_INSPECTION,
        PROSPECTING_DETAILED_FIELD_SURVEY,
        PROSPECTING_INDEXED_CHANNEL_SURVEY,
    ] {
        assert!(registries.labor().get_prospecting(prospecting).is_some());
    }
    for energy in [
        ENERGY_MECHANICAL_SMALL_DRIVE,
        ENERGY_MECHANICAL_LARGE_DRIVE,
        ENERGY_ELECTRICAL_BUFFER,
        ENERGY_THERMAL_SINK,
        ENERGY_TIMBER_FLYWHEEL_DRIVE,
        ENERGY_STONE_FLYWHEEL_DRIVE,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        ENERGY_TIMBER_FRAME_FLYWHEEL_BANK,
    ] {
        assert!(registries.energy().get_store(energy).is_some());
    }
    for process in [
        PROCESS_CRUSH_ORE,
        PROCESS_DRESS_STONE_CHIP_DRILL_BIT,
        PROCESS_KNAP_STONE_DRILL_BIT,
        PROCESS_SHAPE_STONE_GRINDSTONE_WHEEL,
        PROCESS_GRIND_STONE_SCRAP_TOOL,
        PROCESS_GRIND_STONE_SCRAP_DRILL_BIT,
        PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX,
        PROCESS_ASSEMBLE_BULK_TIMBER_CRATE,
        PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
        PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY,
        PROCESS_SHAPE_STONE_PROVISIONS_CROCK,
        PROCESS_SALVAGE_TIMBER_CHEST_BODY,
        PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
        PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
        PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
        PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
        PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY,
        PROCESS_REKNAP_STONE_SCRAP_TOOL,
        PROCESS_MELT_PURE_COPPER,
        PROCESS_CAST_PURE_COPPER,
        PROCESS_SCREEN_CRUSHED_ORE,
        PROCESS_GRIND_CRUSHED_ORE,
        PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
        PROCESS_PIERCE_COPPER_SCREEN_PLATE,
        PROCESS_POWER_DRILL_COPPER_SCREEN_PLATE,
        PROCESS_POWER_GRIND_STONE_SCRAP_TOOL,
        PROCESS_POWER_GRIND_STONE_SCRAP_DRILL_BIT,
        PROCESS_POWER_TURN_WOOD_HANDLE,
        PROCESS_POWER_TURN_TIMBER_FLYWHEEL,
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
        PROCESS_REWORK_WOOD_SCRAP_HANDLE,
        PROCESS_RECOVER_WOOD_SCRAP_BOARDS,
        PROCESS_REGRIND_COPPER_TAILINGS,
        PROCESS_SCAVENGE_COPPER_TAILINGS,
        PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE,
        PROCESS_SAW_WOOD_BOARDS,
        PROCESS_SHAPE_TIMBER_FLYWHEEL,
        PROCESS_SHAPE_TIMBER_RIDDLE_PANEL,
        PROCESS_HEAT_MATERIAL_BATCH,
        PROCESS_HAND_BREAK_ORE,
        PROCESS_HAND_GRIND_CRUSHED_ORE,
        PROCESS_HAND_SORT_NATIVE_COPPER,
        PROCESS_SEPARATE_NATIVE_COPPER,
        PROCESS_CONCENTRATE_COPPER,
    ] {
        assert!(registries.production().get_process(process).is_some());
    }
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_CRUSH_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_comminution(PROCESS_REGRIND_COPPER_TAILINGS)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_manual_comminution(PROCESS_HAND_GRIND_CRUSHED_ORE)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_SCAVENGE_COPPER_TAILINGS)
            .is_some()
    );
    assert!(
        registries
            .ore_processing()
            .get_constituent_separation(PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE)
            .is_some()
    );
    assert!(
        registries
            .thermal()
            .get_sensible_heating(PROCESS_HEAT_MATERIAL_BATCH)
            .is_some()
    );
    assert!(
        registries
            .thermal()
            .get_melting(PROCESS_MELT_PURE_COPPER)
            .is_some()
    );
    assert!(
        registries
            .thermal()
            .get_casting(PROCESS_CAST_PURE_COPPER)
            .is_some()
    );
    let melting = registries
        .thermal()
        .get_melting(PROCESS_MELT_PURE_COPPER)
        .unwrap_or_else(|| panic!("built-in copper melting definition disappeared"));
    assert_eq!(melting.material(), MATERIAL_COPPER);
    assert_eq!(
        melting.solid_forms(),
        &[
            FORM_INGOT,
            FORM_CHIP,
            FORM_REINFORCEMENT,
            FORM_NATIVE_METAL,
            FORM_SCRAP
        ]
    );
    assert_eq!(melting.liquid_form(), FORM_MOLTEN);
    let casting = registries
        .thermal()
        .get_casting(PROCESS_CAST_PURE_COPPER)
        .unwrap_or_else(|| panic!("built-in copper casting definition disappeared"));
    assert_eq!(casting.material(), MATERIAL_COPPER);
}

#[test]
fn primitive_power_content_exposes_distinct_copper_and_bulk_material_routes() {
    let registries = build_registries();
    let hand = registries
        .labor()
        .get_manual_power(MANUAL_POWER_HAND_CRANK)
        .copied()
        .unwrap_or_else(|| panic!("hand-crank labor method disappeared"));
    let treadle = registries
        .labor()
        .get_manual_power(MANUAL_POWER_FOOT_TREADLE)
        .copied()
        .unwrap_or_else(|| panic!("foot-treadle labor method disappeared"));
    let walking = registries
        .labor()
        .get_manual_power(MANUAL_POWER_WALKING_WHEEL)
        .copied()
        .unwrap_or_else(|| panic!("walking-wheel labor method disappeared"));
    assert_eq!(
        hand.power_capability(),
        capabilities::CAPABILITY_MANUAL_POWER_OUTPUT
    );
    assert_eq!(
        treadle.power_capability(),
        capabilities::CAPABILITY_TREADLE_POWER_OUTPUT
    );
    assert!(treadle.metabolic_efficiency_ppm() > hand.metabolic_efficiency_ppm());
    assert!(
        treadle.condition_wear_ppm_per_active_tick() < hand.condition_wear_ppm_per_active_tick()
    );
    assert_eq!(
        walking.power_capability(),
        capabilities::CAPABILITY_WALKING_WHEEL_POWER_OUTPUT
    );
    assert!(walking.metabolic_efficiency_ppm() > treadle.metabolic_efficiency_ppm());
    assert!(
        walking.condition_wear_ppm_per_active_tick() < treadle.condition_wear_ppm_per_active_tick()
    );

    let compact = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("copper-banded flywheel disappeared"));
    let timber = registries
        .energy()
        .get_store(ENERGY_TIMBER_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("timber flywheel disappeared"));
    let stone = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("stone flywheel disappeared"));
    let bulk = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("paired stone flywheel disappeared"));
    let settlement = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .unwrap_or_else(|| panic!("settlement flywheel bank disappeared"));
    assert!(bulk.capacity() > compact.capacity());
    assert!(settlement.capacity() > bulk.capacity());
    assert_eq!(
        settlement.capacity(),
        Energy::from_nanojoules(5_000_000_000_000)
    );
    assert_eq!(
        settlement.max_input_power(),
        Power::from_microwatts(150_000_000)
    );
    assert_eq!(settlement.max_output_power(), bulk.max_output_power());
    assert_eq!(
        settlement
            .assembly_profile()
            .map(MaterialAssemblyProfile::input_mass),
        Some(Mass::from_milligrams(13_000_000))
    );
    assert_eq!(
        bulk.max_input_power(),
        compact.max_input_power(),
        "paired flywheels share one shaft speed, so doubling the wheels doubles capacity without changing the input power limit"
    );
    assert!(bulk.passive_dissipation_power() > compact.passive_dissipation_power());
    assert!(timber.capacity() < stone.capacity());
    assert!(timber.max_output_power() < stone.max_output_power());
    assert_eq!(
        timber.max_input_power(),
        Power::from_microwatts(100_000_000),
        "the timber accumulator must accept the treadle's full nominal charge rate"
    );
    assert!(timber.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().all(|input| {
            input.commodity().material() == MATERIAL_WOOD
                && input.commodity() != CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
        })
    }));
    assert!(bulk.has_authored_assembly_edge());
    assert!(compact.assembly_profile().is_some_and(|assembly| {
        assembly.inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
        })
    }));
    assert!(bulk.assembly_profile().is_some_and(|assembly| {
        assembly
            .inputs()
            .iter()
            .all(|input| input.commodity().material() != MATERIAL_COPPER)
    }));
    assert!(settlement.assembly_profile().is_some_and(|assembly| {
        assembly
            .inputs()
            .iter()
            .all(|input| input.commodity().material() != MATERIAL_COPPER)
    }));
}

#[test]
fn settlement_flywheel_bank_closes_full_batch_comminution_energy_envelope() {
    let registries = build_registries();
    let bank = registries
        .energy()
        .get_store(ENERGY_TIMBER_FRAME_FLYWHEEL_BANK)
        .unwrap_or_else(|| panic!("settlement flywheel bank disappeared"));
    let primitive_bulk_drive = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("paired primitive flywheel disappeared"));
    let mill = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_FRAME_COMMINUTION_MILL)
        .unwrap_or_else(|| panic!("settlement comminution mill disappeared"));

    for (process, batch_capability) in [
        (PROCESS_CRUSH_ORE, capabilities::CAPABILITY_CRUSHER_BATCH),
        (
            PROCESS_GRIND_CRUSHED_ORE,
            capabilities::CAPABILITY_GRINDER_BATCH,
        ),
        (
            PROCESS_FINE_GRIND_SCREEN_OVERSIZE,
            capabilities::CAPABILITY_GRINDER_BATCH,
        ),
        (
            PROCESS_REGRIND_COPPER_TAILINGS,
            capabilities::CAPABILITY_GRINDER_BATCH,
        ),
    ] {
        let CapabilityValue::Mass(maximum_batch) = mill
            .capabilities()
            .get_capability(batch_capability)
            .unwrap_or_else(|| {
                panic!(
                    "settlement comminution mill lost batch capability {}",
                    batch_capability.value()
                )
            })
        else {
            panic!("settlement comminution batch capability changed physical kind");
        };
        let definition = registries
            .ore_processing()
            .get_comminution(process)
            .unwrap_or_else(|| panic!("comminution process {} disappeared", process.value()));
        let required = crate::energy::calculate_mass_specific_energy(
            maximum_batch,
            definition.specific_energy(),
        );
        assert!(
            required <= bank.capacity(),
            "settlement bank must power process {} at the mill's authored maximum batch",
            process.value()
        );
    }

    let regrind = registries
        .ore_processing()
        .get_comminution(PROCESS_REGRIND_COPPER_TAILINGS)
        .unwrap_or_else(|| panic!("tailings regrind process disappeared"));
    let CapabilityValue::Mass(regrind_batch) = mill
        .capabilities()
        .get_capability(capabilities::CAPABILITY_GRINDER_BATCH)
        .unwrap_or_else(|| panic!("settlement mill lost grinder batch capability"))
    else {
        panic!("settlement grinder batch capability changed physical kind");
    };
    let worst_case =
        crate::energy::calculate_mass_specific_energy(regrind_batch, regrind.specific_energy());
    assert_eq!(worst_case, bank.capacity());
    assert!(
        primitive_bulk_drive.capacity() < worst_case,
        "the settlement bank must close a real ordinary-play capacity gap beyond the primitive paired flywheel"
    );
}

#[test]
fn built_in_manual_ore_processing_is_a_complete_bounded_fallback() {
    let registries = build_registries();
    let breaking = registries
        .ore_processing()
        .get_manual_comminution(PROCESS_HAND_BREAK_ORE)
        .unwrap_or_else(|| panic!("built-in manual ore breaking disappeared"));
    let sorting = registries
        .ore_processing()
        .get_manual_constituent_separation(PROCESS_HAND_SORT_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("built-in manual native-copper sorting disappeared"));
    let powered_sorting = registries
        .ore_processing()
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("built-in powered native-copper sorting disappeared"));
    let powered_breaking = registries
        .ore_processing()
        .get_comminution(PROCESS_CRUSH_ORE)
        .unwrap_or_else(|| panic!("built-in powered ore crushing disappeared"));

    assert_eq!(breaking.input_form(), FORM_ORE);
    assert_eq!(breaking.output_form(), FORM_CRUSHED);
    assert_eq!(sorting.input_form(), breaking.output_form());
    assert_eq!(
        sorting.input_particle_size_range(),
        breaking.output_particle_size(),
        "hand breaking must produce exactly the visible-piece envelope accepted by hand sorting"
    );
    assert!(
        breaking.output_particle_size().minimum_diameter()
            > powered_breaking.output_particle_size().minimum_diameter(),
        "hand breaking should retain coarser sortable pieces instead of duplicating powered crusher fines"
    );
    assert_eq!(
        breaking.output_particle_size().maximum_diameter(),
        powered_breaking.output_particle_size().maximum_diameter()
    );
    assert_eq!(sorting.target_output_form(), FORM_NATIVE_METAL);
    assert_eq!(sorting.residue_output_form(), FORM_CRUSHED);
    assert_eq!(breaking.max_batch_mass(), Mass::from_milligrams(100_000));
    assert_eq!(
        breaking.processing_rate(),
        MassFlow::from_milligrams_per_second(250)
    );
    assert_eq!(sorting.max_batch_mass(), Mass::from_milligrams(200_000));
    assert_eq!(
        sorting.processing_rate(),
        MassFlow::from_milligrams_per_second(500)
    );
    assert_eq!(sorting.target_recovery_ppm(), 650_000);
    assert_eq!(powered_sorting.target_recovery_ppm(), 900_000);
    assert!(sorting.target_recovery_ppm() < powered_sorting.target_recovery_ppm());
    assert_eq!(
        breaking
            .operating_profile()
            .equipment_profile()
            .map(|profile| profile.mass_flow_capability()),
        Some(super::capabilities::CAPABILITY_COBBING_FLOW)
    );
    assert_eq!(
        sorting
            .operating_profile()
            .equipment_profile()
            .map(|profile| profile.mass_flow_capability()),
        Some(super::capabilities::CAPABILITY_ORE_PICKING_FLOW)
    );
    for process in [PROCESS_HAND_BREAK_ORE, PROCESS_HAND_SORT_NATIVE_COPPER] {
        let production = registries
            .production()
            .get_process(process)
            .unwrap_or_else(|| panic!("built-in manual ore process disappeared"));
        assert!(production.capability_requirements().is_empty());
        assert!(registries.manual_process_exertion(process).is_some());
    }
}

#[test]
fn retained_primitive_residue_has_one_coherent_later_concentration_route() {
    let registries = build_registries();
    let ore = registries.ore_processing();
    let sorting = ore
        .get_constituent_separation(PROCESS_SEPARATE_NATIVE_COPPER)
        .unwrap_or_else(|| panic!("built-in primitive sorting definition disappeared"));
    let grinding = ore
        .get_comminution(PROCESS_GRIND_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("built-in grinding definition disappeared"));
    let screening = ore
        .get_screening(PROCESS_SCREEN_CRUSHED_ORE)
        .unwrap_or_else(|| panic!("built-in screening definition disappeared"));
    let regrinding = ore
        .get_comminution(PROCESS_FINE_GRIND_SCREEN_OVERSIZE)
        .unwrap_or_else(|| panic!("built-in fine-grinding definition disappeared"));
    let concentration = ore
        .get_constituent_separation(PROCESS_CONCENTRATE_COPPER)
        .unwrap_or_else(|| panic!("built-in concentration definition disappeared"));
    let tailings_regrind = ore
        .get_comminution(PROCESS_REGRIND_COPPER_TAILINGS)
        .unwrap_or_else(|| panic!("built-in tailings regrind definition disappeared"));
    let scavenger = ore
        .get_constituent_separation(PROCESS_SCAVENGE_COPPER_TAILINGS)
        .unwrap_or_else(|| panic!("built-in tailings scavenger definition disappeared"));
    let cleaning = ore
        .get_constituent_separation(PROCESS_CLEAN_NATIVE_COPPER_CONCENTRATE)
        .unwrap_or_else(|| panic!("built-in concentrate-cleaning definition disappeared"));

    assert_eq!(sorting.residue_output_form(), grinding.input_form());
    assert_eq!(grinding.output_form(), screening.input_form());
    assert_eq!(screening.output_form(), regrinding.input_form());
    assert_eq!(regrinding.output_form(), concentration.input_form());
    assert_ne!(
        concentration.residue_output_form(),
        concentration.input_form(),
        "concentration must terminate the current-tier reprocessing route instead of feeding itself"
    );
    assert_eq!(
        concentration.residue_output_form(),
        tailings_regrind.input_form(),
        "first-pass tailings must be the explicit feed for the later finer liberation step"
    );
    assert_eq!(
        concentration.target_output_form(),
        cleaning.input_form(),
        "primary concentrate must feed the ordinary native-copper cleanup stage"
    );
    assert_eq!(
        scavenger.target_output_form(),
        cleaning.input_form(),
        "scavenged concentrate must converge on the same ordinary cleanup stage"
    );
    assert_eq!(cleaning.target_output_form(), FORM_NATIVE_METAL);
    assert_eq!(cleaning.residue_output_form(), FORM_EXHAUSTED_TAILINGS);
    assert_eq!(cleaning.non_target_recovery_ppm(), 0);
    assert_eq!(cleaning.target_recovery_ppm(), 900_000);
    assert_eq!(tailings_regrind.output_form(), scavenger.input_form());
    assert_eq!(scavenger.residue_output_form(), FORM_EXHAUSTED_TAILINGS);
    assert_ne!(
        scavenger.residue_output_form(),
        scavenger.input_form(),
        "the scavenger pass must terminate in exhausted tailings rather than permitting an identical loop"
    );
    assert!(
        scavenger.target_recovery_ppm() < concentration.target_recovery_ppm(),
        "the tailings scavenger must remain a lower-recovery cleanup pass rather than a better primary separator"
    );
    assert!(
        scavenger.non_target_recovery_ppm() < concentration.non_target_recovery_ppm(),
        "the scavenger pass should carry less gangue into its small recovered stream"
    );

    let concentration_range = concentration
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("built-in concentration lost its liberation envelope"));
    assert_eq!(
        regrinding.output_particle_size(),
        concentration_range,
        "screen oversize must have a real regrind route into concentration-sized feed"
    );
    let scavenger_range = scavenger
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("built-in tailings scavenger lost its liberation envelope"));
    assert_eq!(
        tailings_regrind.output_particle_size(),
        scavenger_range,
        "tailings must receive additional fine grinding before the scavenger pass can accept them"
    );
    assert!(
        scavenger_range.maximum_diameter() < concentration_range.minimum_diameter(),
        "scavenger feed must be distinctly finer than the primary concentration envelope"
    );
    let regrind_feed = regrinding
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("built-in fine grinding lost its oversize feed envelope"));
    assert!(
        regrind_feed.minimum_diameter() > screening.aperture(),
        "fine grinding must consume only screen oversize rather than repeating work on accepted fines"
    );

    let mut has_direct_fines = false;
    let mut has_regrind_oversize = false;
    for class in grinding.output_particle_size_distribution().classes() {
        let range = class.range();
        if range.maximum_diameter() <= screening.aperture() {
            has_direct_fines = true;
            assert!(
                range.minimum_diameter() >= concentration_range.minimum_diameter()
                    && range.maximum_diameter() <= concentration_range.maximum_diameter(),
                "screen undersize from ordinary grinding must already fit concentration's authored feed envelope"
            );
        } else {
            has_regrind_oversize = true;
            assert!(range.minimum_diameter() > screening.aperture());
            assert!(
                range.minimum_diameter() >= regrind_feed.minimum_diameter()
                    && range.maximum_diameter() <= regrind_feed.maximum_diameter(),
                "screen oversize must fit the authored fine-grinding feed envelope"
            );
        }
    }
    assert!(
        has_direct_fines && has_regrind_oversize,
        "ordinary grinding must create both immediately usable fines and physically necessary oversize rework"
    );
}

#[test]
fn first_foundry_content_forms_an_ordinary_electrical_casting_chain() {
    let registries = build_registries();
    let method = registries
        .labor()
        .get_manual_power(MANUAL_POWER_TREADLE_DYNAMO)
        .unwrap_or_else(|| panic!("first-foundry electrical labor method disappeared"));
    assert_eq!(method.carrier(), EnergyCarrier::Electrical);
    assert_eq!(
        method.power_capability(),
        capabilities::CAPABILITY_TREADLE_DYNAMO_OUTPUT
    );

    let dynamo = registries
        .equipment()
        .get_equipment(EQUIPMENT_TIMBER_TREADLE_DYNAMO)
        .unwrap_or_else(|| panic!("first-foundry dynamo disappeared"));
    assert!(!dynamo.requires_structural_support());
    assert!(dynamo.assembly_profile().is_some());
    let dynamo_upgrade = dynamo
        .upgrade_profile()
        .unwrap_or_else(|| panic!("first-foundry dynamo lost treadle conversion route"));
    assert_eq!(dynamo_upgrade.from(), EQUIPMENT_TIMBER_TREADLE_DRIVE);
    assert_eq!(
        dynamo
            .capabilities()
            .get_capability(capabilities::CAPABILITY_TREADLE_POWER_OUTPUT),
        Some(CapabilityValue::Power(Power::from_microwatts(100_000_000)))
    );
    assert_eq!(
        dynamo
            .capabilities()
            .get_capability(capabilities::CAPABILITY_TREADLE_DYNAMO_OUTPUT),
        Some(CapabilityValue::Power(Power::from_microwatts(100_000_000)))
    );

    for equipment in [
        EQUIPMENT_STONE_ARC_CRUCIBLE_FURNACE,
        EQUIPMENT_STONE_INGOT_MOLD,
    ] {
        let definition = registries
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("first-foundry thermal equipment disappeared"));
        assert!(!definition.requires_structural_support());
        assert!(definition.assembly_profile().is_some());
        assert_eq!(
            definition
                .capabilities()
                .get_capability(capabilities::CAPABILITY_THERMAL_BATCH),
            Some(CapabilityValue::Mass(Mass::from_milligrams(20_000)))
        );
        assert_eq!(
            definition
                .capabilities()
                .get_capability(capabilities::CAPABILITY_THERMAL_MAX_TEMPERATURE),
            Some(CapabilityValue::Temperature(Temperature::from_millikelvin(
                1_450_000
            )))
        );
    }

    let batch_mold = registries
        .equipment()
        .get_equipment(EQUIPMENT_FOUR_CAVITY_STONE_INGOT_MOLD)
        .unwrap_or_else(|| panic!("settlement batch mold disappeared"));
    let batch_mold_upgrade = batch_mold
        .upgrade_profile()
        .unwrap_or_else(|| panic!("settlement batch mold lost first-foundry upgrade route"));
    assert_eq!(batch_mold_upgrade.from(), EQUIPMENT_STONE_INGOT_MOLD);
    assert!(
        batch_mold_upgrade.additions().inputs().iter().any(|input| {
            input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
                && input.mass() == Mass::from_milligrams(80_000)
        }),
        "settlement batch mold must consume four first-foundry copper ingots"
    );
    assert!(
        batch_mold
            .assembly_profile()
            .is_some_and(|profile| profile.inputs().iter().any(|input| {
                input.commodity() == CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
                    && input.mass() == Mass::from_milligrams(80_000)
            })),
        "fresh settlement batch mold assembly must preserve the cast-ingot material bill"
    );

    let electrical = registries
        .energy()
        .get_store(ENERGY_COPPER_PLATE_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("first-foundry electrical buffer disappeared"));
    assert_eq!(electrical.carrier(), EnergyCarrier::Electrical);
    assert_eq!(
        electrical.capacity(),
        Energy::from_nanojoules(15_000_000_000_000)
    );
    assert_eq!(
        electrical.max_input_power(),
        Power::from_microwatts(100_000_000)
    );
    assert_eq!(
        electrical.max_output_power(),
        Power::from_microwatts(100_000_000)
    );
    assert!(electrical.assembly_profile().is_some());

    let thermal = registries
        .energy()
        .get_store(ENERGY_STONE_THERMAL_SINK)
        .unwrap_or_else(|| panic!("first-foundry thermal sink disappeared"));
    assert_eq!(thermal.carrier(), EnergyCarrier::Thermal);
    assert_eq!(
        thermal.capacity(),
        Energy::from_nanojoules(50_000_000_000_000)
    );
    assert_eq!(
        thermal.max_input_power(),
        Power::from_microwatts(200_000_000)
    );
    assert_eq!(thermal.max_output_power(), Power::ZERO);
    assert_eq!(
        thermal.passive_dissipation_power(),
        Power::from_microwatts(20_000_000)
    );
    assert!(thermal.assembly_profile().is_some());

    let rework = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_INGOT_REINFORCEMENT)
        .unwrap_or_else(|| panic!("first-foundry ingot rework disappeared"));
    assert_eq!(
        rework.input(),
        CommodityKey::new(MATERIAL_COPPER, FORM_INGOT)
    );
    assert_eq!(rework.input_mass(), Mass::from_milligrams(20_000));
    assert_eq!(rework.duration(), TickSpan::new(15));
    assert_eq!(
        rework.outputs(),
        &[ManualCraftOutput::new(
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            Mass::from_milligrams(20_000),
        )]
    );
}

#[test]
fn built_in_thermal_process_discovery_leaves_dynamic_limits_to_resolvers() {
    let registries = build_registries();
    for (process, transfer_power_capability) in [
        (
            PROCESS_HEAT_MATERIAL_BATCH,
            super::super::capabilities::CAPABILITY_HEATING_POWER,
        ),
        (
            PROCESS_MELT_PURE_COPPER,
            super::super::capabilities::CAPABILITY_HEATING_POWER,
        ),
        (
            PROCESS_CAST_PURE_COPPER,
            super::super::capabilities::CAPABILITY_COOLING_POWER,
        ),
    ] {
        let process = registries
            .production()
            .get_process(process)
            .unwrap_or_else(|| panic!("built-in thermal process disappeared"));
        assert_eq!(
            process.capability_requirements(),
            &[
                CapabilityRequirement::new(
                    transfer_power_capability,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Power(Power::from_picowatts(1)),
                ),
                CapabilityRequirement::new(
                    super::super::capabilities::CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
                ),
                CapabilityRequirement::new(
                    super::super::capabilities::CAPABILITY_THERMAL_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
            "generic thermal provider discovery must not duplicate operation-specific physical limits"
        );
    }
}
