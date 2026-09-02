//! Built-in registry assembly, reference integrity, and resolver-ownership tests.

use std::collections::BTreeSet;

use super::*;
use crate::capability::{
    CapabilityComparison, CapabilityDefinition, CapabilityId, CapabilityRegistry,
    CapabilityRequirement, CapabilityValue, CapabilityValueKind,
};
use crate::core::quantity::{
    Energy, Length, Mass, MassFlow, MassSpecificEnergy, Power, Temperature,
};
use crate::core::time::TickSpan;
use crate::energy::{EnergyCarrier, EnergyStoreDefinition, EnergyStoreDefinitionId};
use crate::material::{
    CommodityKey, MaterialAssemblyProfile, MaterialInputSpec, ParticleSizeRange,
};
use crate::ore_processing::{
    ComminutionProcessDefinition, OreProcessingRegistry, PoweredOreProcessProfile,
};
use crate::production::{ProcessDefinition, ProcessId, ProductionRegistry};
use crate::survival::{
    ConsumptionTemperatureRange, FoodCategory, FoodDefinition, SurvivalRegistry,
};
use crate::thermal::{
    CastingPhaseChange, CastingProcessDefinition, MeltingProcessDefinition, PhaseChangeForms,
    PhaseChangeProcessProfile, SensibleHeatingProcessDefinition, ThermalRegistry,
};

const TEST_CAPABILITY: CapabilityId = CapabilityId::new(700_001);
const TEST_PROCESS: ProcessId = ProcessId::new(700_001);
const TEST_MASS_FLOW: CapabilityId = CapabilityId::new(700_002);
const TEST_MAX_BATCH_MASS: CapabilityId = CapabilityId::new(700_003);
const TEST_HEATING_POWER: CapabilityId = CapabilityId::new(700_004);
const TEST_MAX_TEMPERATURE: CapabilityId = CapabilityId::new(700_005);

#[test]
fn built_in_water_has_an_authoritative_liquid_phase_boundary() {
    let registries = build_registries();
    let water = registries
        .materials()
        .get_material(MATERIAL_WATER)
        .unwrap_or_else(|| panic!("built-in water material disappeared"));
    let fusion = water
        .properties()
        .thermal()
        .fusion()
        .unwrap_or_else(|| panic!("built-in liquid water must define fusion physics"));

    assert_eq!(fusion.melting_point(), materials::WATER_MELTING_POINT);
    assert_eq!(
        fusion.latent_heat_j_per_kg(),
        materials::WATER_LATENT_HEAT_OF_FUSION_J_PER_KG
    );
    assert_eq!(
        registries
            .fluid()
            .get_fluid(FLUID_WATER)
            .and_then(|definition| {
                definition.minimum_modeled_temperature(registries.materials())
            }),
        Some(materials::WATER_MELTING_POINT)
    );
}

#[test]
fn preservation_storage_spans_bulk_capacity_and_compact_protection_specialists() {
    let registries = build_registries();
    let rough = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough timber field box disappeared"));
    let bulk = registries
        .storage()
        .get(STORAGE_BULK_TIMBER_PROVISIONS_CRATE)
        .unwrap_or_else(|| panic!("bulk timber provisions crate disappeared"));
    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard timber provisions chest disappeared"));
    let protected = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall timber provisions chest disappeared"));
    let pantry = registries
        .storage()
        .get(STORAGE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated timber pantry disappeared"));
    let crock = registries
        .storage()
        .get(STORAGE_CARVED_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("carved stone provisions crock disappeared"));

    assert_eq!(
        rough.maximum_stockpile_capacity(),
        Mass::from_milligrams(10_000_000)
    );
    assert_eq!(
        bulk.maximum_stockpile_capacity(),
        Mass::from_milligrams(50_000_000)
    );
    assert_eq!(
        standard.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        protected.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        pantry.maximum_stockpile_capacity(),
        Mass::from_milligrams(8_000_000)
    );
    assert_eq!(
        crock.maximum_stockpile_capacity(),
        Mass::from_milligrams(6_000_000)
    );
    assert_eq!(
        rough.storage_profile().preservation_multiplier_ppm(),
        1_250_000
    );
    assert_eq!(
        bulk.storage_profile().preservation_multiplier_ppm(),
        1_500_000
    );
    assert_eq!(
        standard.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        protected.storage_profile().preservation_multiplier_ppm(),
        3_000_000
    );
    assert_eq!(
        pantry.storage_profile().preservation_multiplier_ppm(),
        4_000_000
    );
    assert_eq!(
        crock.storage_profile().preservation_multiplier_ppm(),
        2_500_000
    );

    let rough_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap_or_else(|| panic!("rough timber field box joinery disappeared"));
    let bulk_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_BULK_TIMBER_CRATE)
        .unwrap_or_else(|| panic!("bulk timber crate joinery disappeared"));
    let pantry_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_INSULATED_TIMBER_PANTRY)
        .unwrap_or_else(|| panic!("insulated timber pantry joinery disappeared"));
    let crock_shaping = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_STONE_PROVISIONS_CROCK)
        .unwrap_or_else(|| panic!("stone provisions crock shaping disappeared"));
    let boards = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("timber board shaping disappeared"));
    let board_output = boards
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("board shaping lost board output"));
    let rough_batches = rough_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let bulk_batches = bulk_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let pantry_batches = pantry_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    assert_eq!(rough_batches, 2);
    assert_eq!(bulk_batches, 4);
    assert_eq!(pantry_batches, 6);
    assert_eq!(
        boards.duration().value() * rough_batches + rough_joinery.duration().value(),
        150
    );
    assert_eq!(
        boards.duration().value() * bulk_batches + bulk_joinery.duration().value(),
        290
    );
    assert_eq!(
        boards.duration().value() * pantry_batches + pantry_joinery.duration().value(),
        440
    );
    assert_eq!(boards.input_mass().milligrams() * rough_batches, 2_000_000);
    assert_eq!(boards.input_mass().milligrams() * bulk_batches, 4_000_000);
    assert_eq!(boards.input_mass().milligrams() * pantry_batches, 6_000_000);
    assert_eq!(
        crock_shaping.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
    );
    assert_eq!(crock_shaping.input_mass(), Mass::from_milligrams(3_000_000));
    assert_eq!(crock_shaping.duration(), TickSpan::new(180));
    assert_eq!(
        crock_shaping
            .outputs()
            .iter()
            .find(|output| {
                output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY)
            })
            .map(|output| output.mass()),
        Some(Mass::from_milligrams(2_400_000))
    );
    assert_eq!(
        crock_shaping
            .outputs()
            .iter()
            .find(|output| output.commodity() == CommodityKey::new(MATERIAL_STONE, FORM_CHIP))
            .map(|output| output.mass()),
        Some(Mass::from_milligrams(600_000))
    );
    assert!(
        crock
            .assembly_profile()
            .inputs()
            .iter()
            .all(|input| input.commodity().material() == MATERIAL_STONE)
    );
    assert!(crock.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            > standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(
        crock.storage_profile().preservation_multiplier_ppm()
            < protected.storage_profile().preservation_multiplier_ppm()
    );
    assert!(rough.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        rough.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(rough_joinery.input_mass() < standard.assembly_profile().input_mass());
    assert!(bulk.maximum_stockpile_capacity() > standard.maximum_stockpile_capacity());
    assert!(
        bulk.storage_profile().preservation_multiplier_ppm()
            < standard.storage_profile().preservation_multiplier_ppm()
    );
    assert!(pantry.maximum_stockpile_capacity() < standard.maximum_stockpile_capacity());
    assert!(
        pantry.storage_profile().preservation_multiplier_ppm()
            > protected.storage_profile().preservation_multiplier_ppm()
    );
}

#[test]
fn stone_crock_salvage_returns_exact_reworkable_stone_scrap() {
    let registries = build_registries();
    let salvage = registries
        .crafting()
        .get_manual(PROCESS_SALVAGE_STONE_PROVISIONS_CROCK_BODY)
        .unwrap_or_else(|| panic!("stone crock salvage process disappeared"));
    assert_eq!(
        salvage.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY)
    );
    assert_eq!(salvage.input_mass(), Mass::from_milligrams(2_400_000));
    assert_eq!(salvage.duration(), TickSpan::new(70));
    assert_eq!(salvage.outputs().len(), 1);
    assert_eq!(
        salvage.outputs()[0].commodity(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert_eq!(
        salvage.outputs()[0].mass(),
        Mass::from_milligrams(2_400_000)
    );
    assert!(
        registries
            .crafting()
            .manual_consumers(CommodityKey::new(MATERIAL_STONE, FORM_SCRAP))
            .any(|process| process.process() == PROCESS_REKNAP_STONE_SCRAP_TOOL),
        "crock salvage must return stone to the existing rework economy"
    );
}

#[test]
fn infrastructure_assembly_cannot_hide_perishable_food_from_storage_age() {
    let food = CommodityKey::new(MATERIAL_WOOD, FORM_BOARD);
    let infrastructure = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    let base_survival = survival::build_test_survival_registry();
    let survival = SurvivalRegistry::new(
        base_survival.physiology(),
        [FoodDefinition::new(
            food,
            FoodCategory::Fruit,
            MassSpecificEnergy::from_nanojoules_per_milligram(1),
            0,
            TickSpan::new(10),
            ConsumptionTemperatureRange::new(
                Temperature::from_millikelvin(273_150),
                Temperature::from_millikelvin(333_150),
            ),
        )],
        std::iter::empty(),
    );
    let store = EnergyStoreDefinition::new_with_transfer_limits(
        EnergyStoreDefinitionId::new(990_001),
        "perishable embodiment fixture",
        EnergyCarrier::Mechanical,
        Energy::from_nanojoules(10),
        Power::from_picowatts(1_000),
        Power::from_picowatts(1_000),
    )
    .with_assembly_profile(MaterialAssemblyProfile::new(vec![MaterialInputSpec::pure(
        infrastructure,
        Mass::from_milligrams(1),
    )]));

    let result = std::panic::catch_unwind(|| {
        make_test_registries_with_energy_store_and_survival(store, survival)
    });

    assert!(result.is_err());
}

fn assert_thermal_reference_validation_rejects(thermal: ThermalRegistry) {
    let mut capabilities = CapabilityRegistry::new();
    for (id, name, kind) in [
        (
            TEST_HEATING_POWER,
            "test thermal power",
            CapabilityValueKind::Power,
        ),
        (
            TEST_MAX_TEMPERATURE,
            "test maximum temperature",
            CapabilityValueKind::Temperature,
        ),
        (
            TEST_MAX_BATCH_MASS,
            "test maximum batch mass",
            CapabilityValueKind::Mass,
        ),
    ] {
        capabilities.register_capability(CapabilityDefinition::new(id, name, kind));
    }
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new_selected_batch(
        TEST_PROCESS,
        "invalid phase-change fixture",
        vec![
            CapabilityRequirement::new(
                TEST_HEATING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_picowatts(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
        ],
    ));
    let material_registry = materials::build_material_registry();

    let result = std::panic::catch_unwind(|| {
        thermal.validate_references(&production, &capabilities, &material_registry)
    });

    assert!(result.is_err());
}

fn primitive_commodity_has_root_route(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
) -> bool {
    if roots.contains(&commodity) {
        return true;
    }
    if !visiting.insert(commodity) {
        return false;
    }
    let reachable = registries
        .crafting()
        .definitions()
        .filter(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == commodity)
        })
        .any(|producer| {
            primitive_commodity_has_root_route(registries, producer.input(), roots, visiting)
        });
    assert!(visiting.remove(&commodity));
    reachable
}

fn assert_primitive_commodity_reachable(
    registries: &Registries,
    commodity: CommodityKey,
    roots: &BTreeSet<CommodityKey>,
    visiting: &mut BTreeSet<CommodityKey>,
) {
    assert!(
        primitive_commodity_has_root_route(registries, commodity, roots, visiting),
        "primitive component commodity {} must have at least one acyclic ordinary manual route from an authored primitive root",
        commodity.value()
    );
}

#[test]
fn every_declared_primitive_infrastructure_component_has_a_transitive_runtime_route() {
    let registries = build_registries();
    let roots = BTreeSet::from([
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
    ]);
    let mut required = BTreeSet::new();

    for definition in registries.equipment().definitions() {
        if !definition.has_authored_acquisition_edge() {
            continue;
        }
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
        if let Some(upgrade) = definition.upgrade_profile() {
            let base = registries
                .equipment()
                .get_equipment(upgrade.from())
                .unwrap_or_else(|| unreachable!("registry validation resolves upgrade bases"));
            assert!(
                base.has_authored_acquisition_edge(),
                "equipment upgrade {} starts from base {} with no direct authored acquisition edge",
                definition.id().value(),
                base.id().value()
            );
            required.extend(
                upgrade
                    .additions()
                    .inputs()
                    .iter()
                    .map(MaterialInputSpec::commodity),
            );
        }
    }
    for definition in registries.energy().definitions() {
        if let Some(assembly) = definition.assembly_profile() {
            required.extend(assembly.inputs().iter().map(MaterialInputSpec::commodity));
        }
    }
    for definition in registries.storage().definitions() {
        required.extend(
            definition
                .assembly_profile()
                .inputs()
                .iter()
                .map(MaterialInputSpec::commodity),
        );
    }

    for commodity in required {
        assert_primitive_commodity_reachable(&registries, commodity, &roots, &mut BTreeSet::new());
    }
}

#[test]
fn built_in_preservation_storage_has_a_complete_material_route_and_legible_component() {
    let registries = build_registries();
    let chest = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("built-in provisions chest definition disappeared"));
    assert_eq!(
        chest.maximum_stockpile_capacity(),
        Mass::from_milligrams(20_000_000)
    );
    assert_eq!(
        chest.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        chest.storage_profile().maximum_temperature(),
        Temperature::from_millikelvin(333_150),
        "ordinary timber provisions storage must not act as high-temperature containment"
    );
    assert_eq!(
        chest.assembly_profile().input_mass(),
        Mass::from_milligrams(2_400_000)
    );
    assert_eq!(chest.assembly_profile().inputs().len(), 1);
    let body = chest.assembly_profile().inputs()[0].commodity();
    assert_eq!(body, CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY));
    assert!(
        registries
            .textures()
            .get_commodity_appearance(body)
            .and_then(|binding| binding.object())
            .is_some(),
        "preservation chest body must remain player-legible"
    );
    let chest_process = registries
        .crafting()
        .definitions()
        .find(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == body)
        })
        .unwrap_or_else(|| panic!("provisions chest body lost its ordinary joinery route"));
    assert_eq!(chest_process.process(), PROCESS_ASSEMBLE_TIMBER_CHEST);
    assert_eq!(
        chest_process.input(),
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)
    );
    assert_eq!(chest_process.input_mass(), Mass::from_milligrams(2_400_000));
    assert_eq!(chest_process.duration(), TickSpan::new(80));
    let board = chest_process.input();
    let board_process = registries
        .crafting()
        .definitions()
        .find(|definition| {
            definition
                .outputs()
                .iter()
                .any(|output| output.commodity() == board)
        })
        .unwrap_or_else(|| panic!("provisions chest boards lost their ordinary shaping route"));
    assert_eq!(board_process.process(), PROCESS_SHAPE_WOOD_BOARDS);
    assert_eq!(
        board_process.input(),
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG)
    );
}

#[test]
fn double_wall_preservation_trades_more_timber_and_attention_for_slower_food_aging() {
    let registries = build_registries();
    let standard = registries
        .storage()
        .get(STORAGE_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("standard timber provisions chest disappeared"));
    let insulated = registries
        .storage()
        .get(STORAGE_DOUBLE_WALL_TIMBER_PROVISIONS_CHEST)
        .unwrap_or_else(|| panic!("double-wall timber provisions chest disappeared"));

    assert_eq!(
        insulated.maximum_stockpile_capacity(),
        standard.maximum_stockpile_capacity(),
        "stronger preservation must not silently buy more usable storage capacity"
    );
    assert_eq!(
        insulated.storage_profile().maximum_temperature(),
        standard.storage_profile().maximum_temperature(),
        "double timber walls do not create high-temperature containment"
    );
    assert_eq!(
        standard.storage_profile().preservation_multiplier_ppm(),
        2_000_000
    );
    assert_eq!(
        insulated.storage_profile().preservation_multiplier_ppm(),
        3_000_000
    );
    assert!(insulated.assembly_profile().input_mass() > standard.assembly_profile().input_mass());
    assert_eq!(
        insulated.assembly_profile().inputs(),
        &[MaterialInputSpec::pure(
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            Mass::from_milligrams(4_000_000),
        )]
    );

    let standard_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_TIMBER_CHEST)
        .unwrap_or_else(|| panic!("standard chest joinery disappeared"));
    let insulated_joinery = registries
        .crafting()
        .get_manual(PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST)
        .unwrap_or_else(|| panic!("double-wall chest joinery disappeared"));
    let boards = registries
        .crafting()
        .get_manual(PROCESS_SHAPE_WOOD_BOARDS)
        .unwrap_or_else(|| panic!("timber board shaping disappeared"));
    let board_output = boards
        .outputs()
        .iter()
        .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
        .map(|output| output.mass())
        .unwrap_or_else(|| panic!("board shaping lost board output"));
    let standard_board_batches = standard_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    let insulated_board_batches = insulated_joinery
        .input_mass()
        .milligrams()
        .div_ceil(board_output.milligrams());
    assert_eq!(standard_board_batches, 3);
    assert_eq!(insulated_board_batches, 5);
    let standard_attention =
        boards.duration().value() * standard_board_batches + standard_joinery.duration().value();
    let insulated_attention =
        boards.duration().value() * insulated_board_batches + insulated_joinery.duration().value();
    assert_eq!(standard_attention, 230);
    assert_eq!(insulated_attention, 370);
    assert!(insulated_attention > standard_attention);
    assert_eq!(
        boards.input_mass().milligrams() * standard_board_batches,
        3_000_000
    );
    assert_eq!(
        boards.input_mass().milligrams() * insulated_board_batches,
        5_000_000
    );
    assert!(
        registries
            .textures()
            .get_commodity_appearance(CommodityKey::new(
                MATERIAL_WOOD,
                FORM_DOUBLE_WALL_CHEST_BODY,
            ))
            .and_then(|binding| binding.object())
            .is_some(),
        "double-wall chest body must remain player-legible"
    );
}

#[test]
fn timber_enclosure_salvage_returns_boards_with_explicit_chip_loss() {
    let registries = build_registries();
    for (process, input_form, input_mass, board_mass, duration) in [
        (
            PROCESS_SALVAGE_TIMBER_CHEST_BODY,
            FORM_CHEST_BODY,
            2_400_000,
            1_600_000,
            70,
        ),
        (
            PROCESS_SALVAGE_ROUGH_TIMBER_FIELD_BOX_BODY,
            FORM_ROUGH_BOX_BODY,
            1_600_000,
            800_000,
            50,
        ),
        (
            PROCESS_SALVAGE_DOUBLE_WALL_TIMBER_CHEST_BODY,
            FORM_DOUBLE_WALL_CHEST_BODY,
            4_000_000,
            3_200_000,
            100,
        ),
        (
            PROCESS_SALVAGE_BULK_TIMBER_CRATE_BODY,
            FORM_BULK_CRATE_BODY,
            3_200_000,
            2_400_000,
            80,
        ),
        (
            PROCESS_SALVAGE_INSULATED_TIMBER_PANTRY_BODY,
            FORM_INSULATED_PANTRY_BODY,
            4_800_000,
            4_000_000,
            120,
        ),
    ] {
        let salvage = registries
            .crafting()
            .get_manual(process)
            .unwrap_or_else(|| panic!("timber enclosure salvage process disappeared"));
        assert_eq!(
            salvage.input(),
            CommodityKey::new(MATERIAL_WOOD, input_form)
        );
        assert_eq!(salvage.input_mass(), Mass::from_milligrams(input_mass));
        assert_eq!(salvage.duration(), TickSpan::new(duration));
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))
                .map(|output| output.mass()),
            Some(Mass::from_milligrams(board_mass))
        );
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .find(|output| output.commodity() == CommodityKey::new(MATERIAL_WOOD, FORM_CHIP))
                .map(|output| output.mass()),
            Some(Mass::from_milligrams(800_000))
        );
        assert_eq!(
            salvage
                .outputs()
                .iter()
                .map(|output| output.mass().milligrams())
                .sum::<u64>(),
            input_mass,
            "manual enclosure salvage must conserve every milligram"
        );
    }
}

#[test]
fn primitive_flywheel_loses_stored_rotation_without_erasing_short_work_windows() {
    let registries = build_registries();
    let flywheel = registries
        .energy()
        .get_store(ENERGY_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("built-in stone flywheel definition disappeared"));
    let loss = flywheel.passive_dissipation_power();

    assert_eq!(loss, Power::from_microwatts(50_000));
    assert!(
        !loss.is_zero(),
        "a physical flywheel must not retain rotation forever"
    );
    assert!(
        loss < flywheel.max_input_power() && loss < flywheel.max_output_power(),
        "passive flywheel drag must remain a background loss rather than dominate active transfer"
    );
    let loss_per_tick = crate::energy::integrate_power(
        loss,
        TickSpan::new(1),
        registries.core().physical_tick_duration(),
        crate::energy::PowerRemainder::ZERO,
    )
    .unwrap_or_else(|error| panic!("stone flywheel loss integration failed: {error}"));
    assert_eq!(
        loss_per_tick.remainder(),
        crate::energy::PowerRemainder::ZERO
    );
    assert_eq!(loss_per_tick.energy(), Energy::from_nanojoules(180_000_000));
    assert!(
        flywheel.capacity().nanojoules() / loss_per_tick.energy().nanojoules() > 2_000,
        "full charge must survive long enough for near-term primitive work instead of becoming a reaction-time tax"
    );
}

#[test]
fn built_in_workshop_energy_buffers_have_coherent_transfer_and_recovery_rates() {
    let registries = build_registries();
    let electrical = registries
        .energy()
        .get_store(ENERGY_ELECTRICAL_BUFFER)
        .unwrap_or_else(|| panic!("built-in electrical buffer disappeared"));
    let thermal = registries
        .energy()
        .get_store(ENERGY_THERMAL_SINK)
        .unwrap_or_else(|| panic!("built-in thermal sink disappeared"));

    assert_eq!(
        electrical.max_input_power(),
        Power::from_microwatts(1_000_000_000_000),
        "an electrical buffer must accept recharge as well as provide stored power"
    );
    assert_eq!(electrical.max_output_power(), electrical.max_input_power());
    assert_eq!(
        thermal.max_input_power(),
        Power::from_microwatts(1_000_000_000_000)
    );
    assert_eq!(
        thermal.passive_dissipation_power(),
        Power::from_microwatts(100_000_000_000)
    );
    assert!(
        thermal.passive_dissipation_power() < thermal.max_input_power(),
        "passive heat rejection must recover the finite sink more slowly than active casting can fill it"
    );
}

#[test]
fn built_in_protein_options_trade_immediate_density_for_storage_resilience() {
    let registries = build_registries();
    let meat = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_MEAT, FORM_FOOD))
        .unwrap_or_else(|| panic!("built-in meat food definition disappeared"));
    let legumes = *registries
        .survival()
        .get_food(CommodityKey::new(MATERIAL_LEGUMES, FORM_FOOD))
        .unwrap_or_else(|| panic!("built-in legume food definition disappeared"));

    assert_eq!(meat.category(), FoodCategory::Protein);
    assert_eq!(legumes.category(), FoodCategory::Protein);
    assert!(meat.dietary_energy() > legumes.dietary_energy());
    assert!(meat.hydration_microliters_per_milligram() > 0);
    assert_eq!(legumes.hydration_microliters_per_milligram(), 0);
    assert!(legumes.shelf_life() > meat.shelf_life());
}

#[test]
fn built_in_world_time_scale_and_gravity_are_stable() {
    let registries = build_registries();

    assert_eq!(
        registries.core().physical_tick_duration().microseconds(),
        3_600_000
    );
    assert_eq!(
        registries.core().gravity().micrometers_per_second_squared(),
        DEFAULT_GRAVITY_MICROMETERS_PER_SECOND_SQUARED
    );
    assert_eq!(
        registries.core().calendar().ticks_per_day(),
        DEFAULT_TICKS_PER_DAY
    );
    assert_eq!(
        registries.core().calendar().physical_seconds_per_day(),
        DEFAULT_PHYSICAL_SECONDS_PER_DAY
    );
}

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
        EQUIPMENT_STONE_SEPARATOR,
        EQUIPMENT_STONE_ROTARY_QUERN,
        EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
        EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
        EQUIPMENT_COPPER_REINFORCED_PICK,
        EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
        EQUIPMENT_STONE_QUARRY_PICK,
        EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
        EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
        EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
        EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
        EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
    ] {
        assert!(registries.equipment().get_equipment(equipment).is_some());
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
        ENERGY_STONE_FLYWHEEL_DRIVE,
        ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
    ] {
        assert!(registries.energy().get_store(energy).is_some());
    }
    for process in [
        PROCESS_CRUSH_ORE,
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
        PROCESS_COLD_WORK_COPPER_SAW_BLADE,
        PROCESS_SAW_WOOD_BOARDS,
        PROCESS_HEAT_MATERIAL_BATCH,
        PROCESS_HAND_BREAK_ORE,
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

    let compact = registries
        .energy()
        .get_store(ENERGY_COPPER_BANDED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("copper-banded flywheel disappeared"));
    let bulk = registries
        .energy()
        .get_store(ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE)
        .unwrap_or_else(|| panic!("paired stone flywheel disappeared"));
    assert!(bulk.capacity() > compact.capacity());
    assert!(bulk.max_input_power() < compact.max_input_power());
    assert!(bulk.passive_dissipation_power() > compact.passive_dissipation_power());
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

    assert_eq!(sorting.residue_output_form(), grinding.input_form());
    assert_eq!(grinding.output_form(), screening.input_form());
    assert_eq!(screening.output_form(), regrinding.input_form());
    assert_eq!(regrinding.output_form(), concentration.input_form());
    assert_ne!(
        concentration.residue_output_form(),
        concentration.input_form(),
        "concentration must terminate the current-tier reprocessing route instead of feeding itself"
    );

    let concentration_range = concentration
        .input_particle_size_range()
        .unwrap_or_else(|| panic!("built-in concentration lost its liberation envelope"));
    assert_eq!(
        regrinding.output_particle_size(),
        concentration_range,
        "screen oversize must have a real regrind route into concentration-sized feed"
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
fn built_in_thermal_process_discovery_leaves_dynamic_limits_to_resolvers() {
    let registries = build_registries();
    for (process, transfer_power_capability) in [
        (
            PROCESS_HEAT_MATERIAL_BATCH,
            super::capabilities::CAPABILITY_HEATING_POWER,
        ),
        (
            PROCESS_MELT_PURE_COPPER,
            super::capabilities::CAPABILITY_HEATING_POWER,
        ),
        (
            PROCESS_CAST_PURE_COPPER,
            super::capabilities::CAPABILITY_COOLING_POWER,
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
                    super::capabilities::CAPABILITY_THERMAL_MAX_TEMPERATURE,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
                ),
                CapabilityRequirement::new(
                    super::capabilities::CAPABILITY_THERMAL_BATCH,
                    CapabilityComparison::AtLeast,
                    CapabilityValue::Mass(Mass::from_milligrams(1)),
                ),
            ],
            "generic thermal provider discovery must not duplicate operation-specific physical limits"
        );
    }
}

#[test]
fn built_in_texture_bindings_resolve_for_material_forms_and_equipment() {
    let registries = build_registries();
    let textures = registries.textures();
    let baked = textures.bake_texture_array();

    for (commodity, block, object) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Some(BLOCK_TIMBER),
            OBJECT_LOG,
        ),
        (
            CommodityKey::new(MATERIAL_CHARCOAL, FORM_LUMP),
            Some(BLOCK_CHARCOAL),
            OBJECT_CHARCOAL,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
            Some(BLOCK_COPPER_ORE),
            OBJECT_COPPER_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
            None,
            OBJECT_CRUSHED_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_CONCENTRATE),
            None,
            OBJECT_CRUSHED_ORE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_INGOT),
            Some(BLOCK_COPPER),
            OBJECT_COPPER_INGOT,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT),
            None,
            OBJECT_COPPER_REINFORCEMENT,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            None,
            OBJECT_NATIVE_COPPER,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            None,
            OBJECT_COPPER_SCRAP,
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_LUMP),
            Some(BLOCK_SLAG),
            OBJECT_SLAG,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_CRUSHED),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_CRUSHED),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_SLAG, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_CLAY, FORM_TAILINGS),
            None,
            OBJECT_TAILINGS,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            None,
            OBJECT_STONE_LUMP,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            None,
            OBJECT_STONE_TOOL,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            None,
            OBJECT_STONE_CHIP,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            None,
            OBJECT_STONE_FLYWHEEL,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            None,
            OBJECT_WOOD_HANDLE,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY),
            None,
            OBJECT_TIMBER_CHEST_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_DOUBLE_WALL_CHEST_BODY),
            None,
            OBJECT_DOUBLE_WALL_TIMBER_CHEST_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_ROUGH_BOX_BODY),
            None,
            OBJECT_ROUGH_TIMBER_FIELD_BOX_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BULK_CRATE_BODY),
            None,
            OBJECT_BULK_TIMBER_CRATE_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_INSULATED_PANTRY_BODY),
            None,
            OBJECT_INSULATED_TIMBER_PANTRY_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_STONE_CROCK_BODY),
            None,
            OBJECT_STONE_PROVISIONS_CROCK_BODY,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SCREEN_PLATE),
            None,
            OBJECT_COPPER_SCREEN_PLATE,
        ),
        (
            CommodityKey::new(MATERIAL_COPPER, FORM_SAW_BLADE),
            None,
            OBJECT_COPPER_SAW_BLADE,
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_SCRAP),
            None,
            OBJECT_WOOD_CHIP,
        ),
    ] {
        let binding = match textures.get_commodity_appearance(commodity) {
            Some(binding) => binding,
            None => panic!("missing commodity appearance {}", commodity.value()),
        };
        assert_eq!(binding.block(), block);
        assert_eq!(binding.object(), Some(object));
        if let Some(block) = block {
            let baked_block = match baked.get_block(block) {
                Some(block) => block,
                None => panic!("missing baked block appearance {}", block.value()),
            };
            let authored_block = match textures.get_block(block) {
                Some(block) => block,
                None => panic!("missing authored block appearance {}", block.value()),
            };
            let top_texture = authored_block.texture(crate::texture::CubeFace::Top);
            assert_eq!(
                baked_block.texture(crate::texture::CubeFace::Top),
                match baked.get_descriptor(top_texture) {
                    Some(descriptor) => descriptor,
                    None => panic!("missing baked texture {}", top_texture.value()),
                }
            );
        }
    }

    for (equipment, object) in [
        (EQUIPMENT_JAW_CRUSHER, OBJECT_JAW_CRUSHER),
        (EQUIPMENT_ELECTRIC_FURNACE, OBJECT_ELECTRIC_FURNACE),
        (EQUIPMENT_CASTING_MOLD, OBJECT_CASTING_MOLD),
        (EQUIPMENT_DRY_SCREEN, OBJECT_DRY_SCREEN),
        (EQUIPMENT_GRINDING_MILL, OBJECT_GRINDING_MILL),
        (EQUIPMENT_GRAVITY_SEPARATOR, OBJECT_GRAVITY_SEPARATOR),
        (EQUIPMENT_STONE_PICK, OBJECT_STONE_PICK),
        (EQUIPMENT_STONE_HAND_CRANK, OBJECT_STONE_HAND_CRANK),
        (EQUIPMENT_STONE_QUARRY_PICK, OBJECT_STONE_QUARRY_PICK),
        (EQUIPMENT_TIMBER_TREADLE_DRIVE, OBJECT_TIMBER_TREADLE_DRIVE),
        (EQUIPMENT_STONE_CRUSHER, OBJECT_STONE_CRUSHER),
        (EQUIPMENT_STONE_SEPARATOR, OBJECT_STONE_SEPARATOR),
        (EQUIPMENT_STONE_ROTARY_QUERN, OBJECT_STONE_ROTARY_QUERN),
        (
            EQUIPMENT_STONE_GEOLOGICAL_HAMMER,
            OBJECT_STONE_GEOLOGICAL_HAMMER,
        ),
        (
            EQUIPMENT_COPPER_PLATE_SIZING_SCREEN,
            OBJECT_COPPER_PLATE_SIZING_SCREEN,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_PICK,
            OBJECT_COPPER_REINFORCED_PICK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_HAND_CRANK,
            OBJECT_COPPER_REINFORCED_HAND_CRANK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_QUARRY_PICK,
            OBJECT_COPPER_REINFORCED_STONE_QUARRY_PICK,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_CRUSHER,
            OBJECT_COPPER_REINFORCED_STONE_CRUSHER,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_SEPARATOR,
            OBJECT_COPPER_REINFORCED_STONE_SEPARATOR,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
            OBJECT_COPPER_REINFORCED_STONE_ROTARY_QUERN,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
            OBJECT_COPPER_REINFORCED_GEOLOGICAL_HAMMER,
        ),
        (
            EQUIPMENT_STONE_WOODWORKING_ADZE,
            OBJECT_STONE_WOODWORKING_ADZE,
        ),
        (
            EQUIPMENT_COPPER_REINFORCED_WOODWORKING_ADZE,
            OBJECT_COPPER_REINFORCED_WOODWORKING_ADZE,
        ),
        (
            EQUIPMENT_TIMBER_FRAME_SAW_BENCH,
            OBJECT_TIMBER_FRAME_SAW_BENCH,
        ),
    ] {
        let binding = match textures.get_equipment_appearance(equipment) {
            Some(binding) => binding,
            None => panic!("missing equipment appearance {}", equipment.value()),
        };
        assert_eq!(binding.object(), object);
        let appearance = match textures.get_object(object) {
            Some(appearance) => appearance,
            None => panic!("missing object appearance {}", object.value()),
        };
        for texture in appearance.textures() {
            assert!(baked.get_descriptor(*texture).is_some());
        }
        let baked_object = match baked.get_object(object) {
            Some(appearance) => appearance,
            None => panic!("missing baked object appearance {}", object.value()),
        };
        assert_eq!(baked_object.textures().len(), appearance.textures().len());
    }
}

#[test]
fn every_supported_separation_residue_host_has_legible_crushed_and_tailings_appearances() {
    let registries = build_registries();
    let textures = registries.textures();

    for material in [MATERIAL_STONE, MATERIAL_CLAY, MATERIAL_SLAG] {
        for form in [FORM_CRUSHED, FORM_TAILINGS] {
            let commodity = CommodityKey::new(material, form);
            assert!(
                registries.materials().has_commodity(commodity),
                "supported separation residue host {} lost authored form {}",
                material.value(),
                form.value()
            );
            assert!(
                textures
                    .get_commodity_appearance(commodity)
                    .and_then(|binding| binding.object())
                    .is_some(),
                "supported separation residue host {} form {} must remain player-legible",
                material.value(),
                form.value()
            );
        }
    }
}

#[test]
fn every_builtin_maintenance_spent_output_has_a_player_legible_appearance() {
    let registries = build_registries();
    let textures = registries.textures();

    for definition in registries.equipment().definitions() {
        let Some(maintenance) = definition.maintenance_profile() else {
            continue;
        };
        let spent = maintenance.spent();
        assert!(
            textures
                .get_commodity_appearance(spent)
                .and_then(|binding| binding.object())
                .is_some(),
            "equipment {} maintenance spent commodity {} must remain player-legible",
            definition.id().value(),
            spent.value()
        );
    }
}

fn process_registry_domains(
    capabilities: CapabilityRegistry,
    ore_processing: OreProcessingRegistry,
    thermal: ThermalRegistry,
    production: ProductionRegistry,
) -> RegistryDomains {
    RegistryDomains {
        energy: empty_energy_registry(),
        fluid: fluid::build_fluid_registry(),
        capabilities,
        crafting: crate::crafting::CraftingRegistry::new(std::iter::empty()),
        labor: labor::empty_labor_registry(),
        equipment: empty_equipment_registry(),
        storage: crate::inventory::StorageRegistry::new(std::iter::empty()),
        structural: structural::build_structural_registry(),
        materials: materials::build_material_registry(),
        mining: crate::mining::MiningRegistry::new(std::iter::empty()),
        ore_processing,
        thermal,
        production,
        survival: survival::build_survival_registry(),
        presentation: RegistryPresentation {
            textures: empty_texture_registry(),
            shaders: empty_shader_registry(),
        },
    }
}

#[test]
fn missing_process_capability_reference_is_rejected_during_registry_assembly() {
    let process = ProcessDefinition::new(
        TEST_PROCESS,
        "test capability process",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        )],
        vec![CapabilityRequirement::new(
            TEST_CAPABILITY,
            CapabilityComparison::AtLeast,
            CapabilityValue::Temperature(Temperature::from_millikelvin(500_000)),
        )],
    );
    let mut production = ProductionRegistry::new();
    production.register_process(process);

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(
                CapabilityRegistry::new(),
                OreProcessingRegistry::new(std::iter::empty()),
                empty_thermal_registry(),
                production,
            ),
        )
    });

    assert!(result.is_err());
}

#[test]
fn process_without_physical_resolver_semantics_is_rejected_during_registry_assembly() {
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new(
        TEST_PROCESS,
        "orphan physical process fixture",
        vec![MaterialInputSpec::new(
            CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
            Mass::from_milligrams(1),
        )],
        Vec::new(),
    ));

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(
                CapabilityRegistry::new(),
                OreProcessingRegistry::new(std::iter::empty()),
                empty_thermal_registry(),
                production,
            ),
        )
    });

    assert!(result.is_err());
}

#[test]
fn process_cannot_own_multiple_physical_resolver_semantics() {
    let mut capabilities = CapabilityRegistry::new();
    for (id, name, kind) in [
        (
            TEST_MASS_FLOW,
            "test mass flow",
            CapabilityValueKind::MassFlow,
        ),
        (
            TEST_MAX_BATCH_MASS,
            "test maximum batch mass",
            CapabilityValueKind::Mass,
        ),
        (
            TEST_HEATING_POWER,
            "test heating power",
            CapabilityValueKind::Power,
        ),
        (
            TEST_MAX_TEMPERATURE,
            "test maximum temperature",
            CapabilityValueKind::Temperature,
        ),
    ] {
        capabilities.register_capability(CapabilityDefinition::new(id, name, kind));
    }
    let process = ProcessDefinition::new_selected_batch(
        TEST_PROCESS,
        "ambiguous physical resolver fixture",
        vec![
            CapabilityRequirement::new(
                TEST_MASS_FLOW,
                CapabilityComparison::AtLeast,
                CapabilityValue::MassFlow(MassFlow::from_milligrams_per_second(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_BATCH_MASS,
                CapabilityComparison::AtLeast,
                CapabilityValue::Mass(Mass::from_milligrams(1)),
            ),
            CapabilityRequirement::new(
                TEST_HEATING_POWER,
                CapabilityComparison::AtLeast,
                CapabilityValue::Power(Power::from_picowatts(1)),
            ),
            CapabilityRequirement::new(
                TEST_MAX_TEMPERATURE,
                CapabilityComparison::AtLeast,
                CapabilityValue::Temperature(Temperature::from_millikelvin(1)),
            ),
        ],
    );
    let mut production = ProductionRegistry::new();
    production.register_process(process);
    let ore_processing = OreProcessingRegistry::new([ComminutionProcessDefinition::new(
        TEST_PROCESS,
        FORM_ORE,
        FORM_CRUSHED,
        match ParticleSizeRange::new(
            Length::from_micrometers(1),
            Length::from_micrometers(20_000),
        ) {
            Ok(range) => range,
            Err(error) => panic!("comminution particle-size fixture failed: {error}"),
        },
        PoweredOreProcessProfile::new(
            TEST_MASS_FLOW,
            TEST_MAX_BATCH_MASS,
            EnergyCarrier::Mechanical,
            MassSpecificEnergy::from_nanojoules_per_milligram(1),
            1,
        ),
    )]);
    let thermal = ThermalRegistry::new(
        [SensibleHeatingProcessDefinition::new(
            TEST_PROCESS,
            TEST_HEATING_POWER,
            TEST_MAX_TEMPERATURE,
            TEST_MAX_BATCH_MASS,
            EnergyCarrier::Electrical,
            1,
        )],
        std::iter::empty(),
        std::iter::empty(),
    );

    let result = std::panic::catch_unwind(|| {
        Registries::new(
            REGISTRY_SCHEMA_VERSION,
            build_core_definitions(),
            process_registry_domains(capabilities, ore_processing, thermal, production),
        )
    });

    assert!(result.is_err());
}
