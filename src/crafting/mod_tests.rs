//! Contract tests for manual crafting and shaping.

use super::*;
use crate::content::{
    EQUIPMENT_STONE_WOODWORKING_ADZE, FORM_BOARD, FORM_CHEST_BODY, FORM_CHIP, FORM_CRUSHED,
    FORM_DOUBLE_WALL_CHEST_BODY, FORM_HANDLE, FORM_INGOT, FORM_LOG, FORM_LUMP, FORM_NATIVE_METAL,
    FORM_ORE, FORM_REINFORCEMENT, FORM_SCRAP, FORM_TOOL, MATERIAL_COPPER, MATERIAL_STONE,
    MATERIAL_WOOD, PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST, PROCESS_ASSEMBLE_TIMBER_CHEST,
    PROCESS_COLD_WORK_COPPER_REINFORCEMENT, PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
    PROCESS_KNAP_STONE_TOOL, PROCESS_REKNAP_STONE_SCRAP_TOOL, PROCESS_SHAPE_WOOD_BOARDS,
    PROSPECTING_FIELD_INSPECTION, STRUCTURAL_PROFILE_AXIAL_COMPRESSION, build_registries,
};
use crate::core::quantity::{Area, Energy, Force, Length, Temperature, Volume};
use crate::core::state::{StateValidationError, validate_loaded_state};
use crate::core::time::WorldSeed;
use crate::equipment::validate_assemble_equipment;
use crate::geology::{FieldProspectingRequest, validate_start_field_prospecting};
use crate::inventory::{
    MaterialLotId, MaterialLotSelection, add_solid_stockpile_for_test,
    deposit_composed_lot_for_test, deposit_lot_for_test, validate_mount_stockpile,
    validate_unmount_stockpile,
};
use crate::labor::{
    PlayerWorkCommitError, PlayerWorkStartError, PlayerWorkValidationError,
    calculate_player_work_resource_budget,
};
use crate::maintenance::Condition;
use crate::material::{CommodityKey, CompositionComponent, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::production::{
    ProcessDefinition, ProcessId, ProductionAvailabilityChange, ProductionRegistry,
    ProductionSuspensionReason, StartProcessError, validate_start_process,
};
use crate::simulation::advance_tick;
use crate::spatial::{VoxelBounds, VoxelCoord};
use crate::structural::{
    StructuralElementId, StructuralLifecycle, StructuralLoadKind, add_structural_element,
    materialize_structural_element_for_test, validate_activate_structural_element,
    validate_set_structural_load,
};
use crate::survival::{SurvivalExertion, assess_survival, initialize_player_survival};

fn stone_lump() -> CommodityKey {
    CommodityKey::new(MATERIAL_STONE, FORM_LUMP)
}

#[test]
fn woodworking_adze_reduces_board_attention_without_changing_yield_and_replays_exactly() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7020));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("woodworking survival setup failed: {error}"));

    let components = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("woodworking component stockpile failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        components,
        CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
        Mass::from_milligrams(800_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("woodworking stone component failed: {error}"));
    deposit_lot_for_test(
        &registries,
        &mut state,
        components,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        Mass::from_milligrams(200_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("woodworking handle component failed: {error}"));
    let adze = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        components,
    )
    .unwrap_or_else(|error| panic!("woodworking adze assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("woodworking adze assembly commit failed: {error}"));

    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("woodworking source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("woodworking destination failed: {error}"));
    let first_log = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("woodworking first log failed: {error}"));
    let second_log = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("woodworking second log failed: {error}"));

    let hand = resolve_manual_craft(
        &registries,
        &state,
        &ManualCraftRequest::single(
            PROCESS_SHAPE_WOOD_BOARDS,
            source,
            MaterialLotSelection::new(first_log, Mass::from_milligrams(1_000_000)),
        ),
    )
    .unwrap_or_else(|error| panic!("hand board shaping resolution failed: {error}"));
    let tool_request = ManualCraftRequest::single(
        PROCESS_SHAPE_WOOD_BOARDS,
        source,
        MaterialLotSelection::new(second_log, Mass::from_milligrams(1_000_000)),
    )
    .with_equipment(adze);
    let assisted = resolve_manual_craft(&registries, &state, &tool_request)
        .unwrap_or_else(|error| panic!("adze board shaping resolution failed: {error}"));
    assert_eq!(hand.duration(), TickSpan::new(50));
    assert_eq!(assisted.duration(), TickSpan::new(28));
    assert_eq!(assisted.outputs(), hand.outputs());
    assert_eq!(
        assisted
            .outputs()
            .iter()
            .map(|output| (output.commodity(), output.mass()))
            .collect::<Vec<_>>(),
        vec![
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_CHIP),
                Mass::from_milligrams(200_000),
            ),
            (
                CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
                Mass::from_milligrams(800_000),
            ),
        ]
    );

    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("woodworking matter-before audit failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(tool_request, destination),
    )
    .unwrap_or_else(|error| panic!("adze board shaping start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("adze board shaping commit failed: {error}"));
    let active = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("adze board shaping job disappeared"));
    assert_eq!(active.active_duration(), TickSpan::new(28));
    assert_eq!(
        active.equipment_provider().map(|trace| trace.equipment()),
        Some(adze)
    );
    assert_eq!(
        active.equipment_condition_after(),
        Some(Condition::new(972_000).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );

    for _ in 0..7 {
        let outcome = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("woodworking pre-save tick failed: {error}"));
        assert!(outcome.production_completions().is_empty());
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("woodworking serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("woodworking decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("woodworking trusted load failed: {error}"));
    assert_eq!(loaded, state);

    while state.production().get_job(job).is_some() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("woodworking continuation failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("woodworking loaded continuation failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(
        state
            .equipment()
            .get_equipment(adze)
            .map(|record| record.condition()),
        Some(Condition::new(972_000).unwrap_or_else(|error| panic!("condition failed: {error}")))
    );
    let output = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("woodworking destination disappeared"));
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        output.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("woodworking matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("woodworking final state audit failed: {error}"));
}

#[test]
fn manual_craft_registry_rejects_output_that_requires_unauthored_particle_state() {
    let registries = build_registries();
    let process = ProcessId::new(880_001);
    let input = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let output = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let input_mass = Mass::from_milligrams(1);
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new_selected_batch(
        process,
        "particulate manual output fixture",
        Vec::new(),
    ));
    let crafting = CraftingRegistry::new([ManualCraftDefinition::new(
        process,
        input,
        input_mass,
        TickSpan::new(1),
        SurvivalExertion::new(Energy::from_nanojoules(1), Volume::ZERO),
        vec![ManualCraftOutput::new(output, input_mass)],
    )]);

    let result = std::panic::catch_unwind(|| {
        crafting.validate_references(
            &production,
            registries.materials(),
            registries.capabilities(),
        );
    });

    assert!(result.is_err());
}

#[test]
fn manual_craft_definition_rejects_zero_exertion() {
    let result = std::panic::catch_unwind(|| {
        ManualCraftDefinition::new(
            ProcessId::new(880_002),
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1),
            TickSpan::new(1),
            SurvivalExertion::REST,
            vec![ManualCraftOutput::new(
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1),
            )],
        )
    });

    assert!(result.is_err());
}

#[test]
fn native_copper_reinforcement_rejects_ordinary_ore_form_without_inventing_separation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7009));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("native copper survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("native copper source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("native copper destination failed: {error}"));
    let ore = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_ORE),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("ordinary copper ore fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                source,
                MaterialLotSelection::new(ore, Mass::from_milligrams(20_000)),
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::InputCommodityMismatch {
                expected: CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            }
        ))
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::ZERO)
    );
}

#[test]
fn native_copper_reinforcement_filters_contaminated_native_metal() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7010));
    initialize_player_survival(&registries, &mut state).unwrap_or_else(|error| {
        panic!("contaminated native copper survival setup failed: {error}")
    });
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated native copper source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated native copper destination failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("contaminated native copper composition failed: {error}"));
    let contaminated = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        mixed,
    )
    .unwrap_or_else(|error| panic!("contaminated native copper fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
                source,
                MaterialLotSelection::new(contaminated, Mass::from_milligrams(20_000)),
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::InputCompositionMismatch {
                expected: CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn native_copper_reinforcement_skips_contaminated_stock_when_pure_metal_exists() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7011));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mixed-stock craft survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(40_000))
        .unwrap_or_else(|error| panic!("mixed-stock craft source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("mixed-stock craft destination failed: {error}"));
    let mixed = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("mixed-stock craft composition failed: {error}"));
    deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        mixed,
    )
    .unwrap_or_else(|error| panic!("mixed-stock contaminated copper failed: {error}"));
    let pure = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("mixed-stock pure copper failed: {error}"));

    let _validated = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            source,
            MaterialLotSelection::new(pure, Mass::from_milligrams(20_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("mixed-stock manual craft should select pure copper: {error}"));
}

#[test]
fn copper_scrap_rework_is_slower_than_native_work_and_replays_exactly() {
    let registries = build_registries();
    let native = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_REINFORCEMENT)
        .unwrap_or_else(|| panic!("native copper reinforcement process disappeared"));
    let scrap = registries
        .crafting()
        .get_manual(PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT)
        .unwrap_or_else(|| panic!("copper scrap recovery process disappeared"));
    assert!(
        scrap.duration() > native.duration(),
        "irregular scrap rework must cost more player attention than starting from native copper"
    );

    let mut state = AppState::new(WorldSeed::new(0xC4AF_7012));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("scrap recovery survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("scrap recovery source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("scrap recovery destination failed: {error}"));
    let temperature = Temperature::from_millikelvin(320_000);
    let scrap_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        Mass::from_milligrams(20_000),
        temperature,
    )
    .unwrap_or_else(|error| panic!("scrap recovery copper fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("scrap recovery matter-before audit failed: {error}"))
        .total();

    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
            source,
            MaterialLotSelection::new(scrap_lot, Mass::from_milligrams(20_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("scrap recovery validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("scrap recovery commit failed: {error}"));
    for _ in 0..20 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("scrap recovery pre-save tick failed: {error}"));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("scrap recovery serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("scrap recovery decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("scrap recovery trusted load failed: {error}"));
    assert_eq!(loaded, state);

    while state.production().get_job(job).is_some() {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("scrap recovery source continuation failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("scrap recovery loaded continuation failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    let output_lot = state
        .inventory()
        .lot_ids(destination)
        .next()
        .unwrap_or_else(|| panic!("scrap recovery reinforcement lot disappeared"));
    let output = state
        .inventory()
        .get_lot(output_lot)
        .unwrap_or_else(|| panic!("scrap recovery reinforcement record disappeared"));
    assert_eq!(
        output.commodity(),
        CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT)
    );
    assert_eq!(output.mass(), Mass::from_milligrams(20_000));
    assert_eq!(output.temperature(), temperature);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("scrap recovery matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn stone_scrap_reknapping_is_slower_than_fresh_knapping_and_replays_exactly() {
    let registries = build_registries();
    let fresh = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("fresh stone knapping process disappeared"));
    let reknap = registries
        .crafting()
        .get_manual(PROCESS_REKNAP_STONE_SCRAP_TOOL)
        .unwrap_or_else(|| panic!("stone scrap reknapping process disappeared"));
    assert!(
        reknap.duration() > fresh.duration(),
        "irregular spent stone must cost more player attention than fresh lump knapping"
    );
    assert_eq!(reknap.duration().value(), 60);
    assert_eq!(
        reknap.input(),
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP)
    );
    assert_eq!(reknap.input_mass(), Mass::from_milligrams(1_000_000));

    let mut state = AppState::new(WorldSeed::new(0xC4AF_7018));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("stone scrap reknap survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stone scrap reknap source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("stone scrap reknap destination failed: {error}"));
    let temperature = Temperature::from_millikelvin(315_000);
    let scrap_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        temperature,
    )
    .unwrap_or_else(|error| panic!("stone scrap reknap fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("stone scrap reknap matter-before failed: {error}"))
        .total();

    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_REKNAP_STONE_SCRAP_TOOL,
            source,
            MaterialLotSelection::new(scrap_lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stone scrap reknap validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stone scrap reknap commit failed: {error}"));
    for _ in 0..25 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("stone scrap reknap pre-save tick failed: {error}"));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("stone scrap reknap serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("stone scrap reknap decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("stone scrap reknap trusted load failed: {error}"));
    assert_eq!(loaded, state);

    while state.production().get_job(job).is_some() {
        let expected = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("stone scrap reknap source continuation failed: {error}")
        });
        let actual = advance_tick(&registries, &mut loaded).unwrap_or_else(|error| {
            panic!("stone scrap reknap loaded continuation failed: {error}")
        });
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)) }),
        Some(Mass::from_milligrams(800_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| { stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)) }),
        Some(Mass::from_milligrams(200_000))
    );
    assert!(
        state
            .inventory()
            .lot_ids(destination)
            .filter_map(|lot| state.inventory().get_lot(lot))
            .filter(|lot| lot.commodity().material() == MATERIAL_STONE)
            .all(|lot| lot.temperature() == temperature)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.stored_mass()),
        Some(Mass::ZERO)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("stone scrap reknap matter-after failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn stone_scrap_reknapping_rejects_contaminated_scrap_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7019));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("contaminated stone scrap survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("contaminated stone scrap source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("contaminated stone scrap destination failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_STONE, 900_000),
        CompositionComponent::new(MATERIAL_WOOD, 100_000),
    ])
    .unwrap_or_else(|error| panic!("contaminated stone scrap composition failed: {error}"));
    let contaminated = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
        composition,
    )
    .unwrap_or_else(|error| panic!("contaminated stone scrap fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_REKNAP_STONE_SCRAP_TOOL,
                source,
                MaterialLotSelection::new(contaminated, Mass::from_milligrams(1_000_000)),
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::InputCompositionMismatch {
                expected: CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn stone_scrap_reknapping_rejects_mixed_temperatures_without_inventing_heat_exchange() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7020));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mixed-temperature reknap survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("mixed-temperature reknap source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("mixed-temperature reknap destination failed: {error}"));
    let lots = [300_000, 310_000].map(|temperature| {
        deposit_lot_for_test(
            &registries,
            &mut state,
            source,
            CommodityKey::new(MATERIAL_STONE, FORM_SCRAP),
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap_or_else(|error| panic!("mixed-temperature reknap fixture failed: {error}"))
    });
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::new(
                ManualCraftRequest::new(
                    PROCESS_REKNAP_STONE_SCRAP_TOOL,
                    source,
                    lots.into_iter()
                        .map(|lot| MaterialLotSelection::new(lot, Mass::from_milligrams(500_000)))
                        .collect(),
                ),
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::MixedInputTemperature
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn manual_craft_selection_is_not_poisoned_by_unselected_different_temperature_matter() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7021));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("temperature-selection survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("temperature-selection source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("temperature-selection destination failed: {error}"));
    let _cold = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(280_000),
    )
    .unwrap_or_else(|error| panic!("temperature-selection cold fixture failed: {error}"));
    let selected = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(320_000),
    )
    .unwrap_or_else(|error| panic!("temperature-selection hot fixture failed: {error}"));
    let request = ManualCraftRequest::single(
        PROCESS_KNAP_STONE_TOOL,
        source,
        MaterialLotSelection::new(selected, Mass::from_milligrams(1_000_000)),
    );

    let resolution = resolve_manual_craft(&registries, &state, &request)
        .unwrap_or_else(|error| panic!("selected homogeneous craft was rejected: {error}"));
    assert!(
        resolution
            .outputs()
            .iter()
            .all(|output| { output.temperature() == Temperature::from_millikelvin(320_000) })
    );
    let _ = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(request, destination),
    )
    .unwrap_or_else(|error| panic!("selected homogeneous craft admission failed: {error}"));
}

#[test]
fn copper_scrap_rework_rejects_contaminated_scrap_without_mutation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7013));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("contaminated scrap survival setup failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated scrap source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(20_000))
        .unwrap_or_else(|error| panic!("contaminated scrap destination failed: {error}"));
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 900_000),
        CompositionComponent::new(MATERIAL_STONE, 100_000),
    ])
    .unwrap_or_else(|error| panic!("contaminated scrap composition failed: {error}"));
    let contaminated = deposit_composed_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
        composition,
    )
    .unwrap_or_else(|error| panic!("contaminated scrap fixture failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                PROCESS_COLD_WORK_COPPER_SCRAP_REINFORCEMENT,
                source,
                MaterialLotSelection::new(contaminated, Mass::from_milligrams(20_000)),
                destination,
            ),
        )
        .err(),
        Some(StartManualCraftError::Resolution(
            ManualCraftError::InputCompositionMismatch {
                expected: CommodityKey::new(MATERIAL_COPPER, FORM_SCRAP),
            }
        ))
    );
    assert_eq!(state, before);
}

fn make_fixture() -> (
    Registries,
    AppState,
    StockpileId,
    MaterialLotId,
    StockpileId,
) {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7001));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft survival initialization failed: {error}"));
    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft source fixture failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
        .unwrap_or_else(|error| panic!("manual craft destination fixture failed: {error}"));
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("manual craft stone fixture failed: {error}"));
    (registries, state, source, lot, destination)
}

fn active_stockpile_support(registries: &Registries, state: &mut AppState) -> StructuralElementId {
    active_stockpile_support_at(registries, state, 0)
}

fn active_stockpile_support_at(
    registries: &Registries,
    state: &mut AppState,
    x: i64,
) -> StructuralElementId {
    let max_x = x
        .checked_add(1)
        .unwrap_or_else(|| panic!("manual craft support x-coordinate overflowed"));
    let bounds = VoxelBounds::new(VoxelCoord::new(x, 0, 0), VoxelCoord::new(max_x, 1, 1))
        .unwrap_or_else(|error| panic!("manual craft support bounds failed: {error}"));
    let support = add_structural_element(
        registries,
        state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        crate::structural::make_test_structural_geometry(
            bounds,
            Length::from_micrometers(1),
            Area::from_square_millimeters(1_000),
        ),
        true,
    )
    .unwrap_or_else(|error| panic!("manual craft support allocation failed: {error}"));
    materialize_structural_element_for_test(registries, state, support, FORM_LOG);
    let _ = validate_activate_structural_element(registries, state, support)
        .unwrap_or_else(|error| panic!("manual craft support activation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("manual craft support activation commit failed: {error}"));
    support
}

#[test]
fn manual_craft_output_support_failure_pauses_work_and_exertion_until_recovered() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("manual craft destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft destination mount commit failed: {error}"));
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("supported manual craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("supported manual craft start commit failed: {error}"));

    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("supported manual craft active tick failed: {error}"));
    let before_pause = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared before suspension"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("manual craft support failure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft support failure commit failed: {error}"));
    assert_eq!(
        state
            .structures()
            .get_element(support)
            .map(|record| record.lifecycle()),
        Some(StructuralLifecycle::Failed)
    );

    let paused = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft suspension tick failed: {error}"));
    assert!(matches!(
        paused.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended {
            job: suspended_job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            ..
        }] if *suspended_job == job && *stockpile == destination
    ));
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let after_pause = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared after suspension"));
    let physiology = registries.survival().physiology();
    assert_eq!(
        before_pause
            .metabolic_energy()
            .checked_sub(after_pause.metabolic_energy()),
        Some(physiology.basal_energy_cost_per_tick())
    );
    assert_eq!(
        before_pause
            .hydration()
            .checked_sub(after_pause.hydration()),
        Some(physiology.hydration_loss_per_tick())
    );

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!("suspended manual craft destination unmount failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("suspended manual craft destination unmount commit failed: {error}")
        });
    let before_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared before resume"));
    let resumed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft resume tick failed: {error}"));
    assert!(matches!(
        resumed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            reason: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            ..
        }] if *resumed_job == job && *stockpile == destination
    ));
    let after_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared after resume"));
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("manual craft definition disappeared"))
        .exertion();
    assert_eq!(
        before_resume
            .metabolic_energy()
            .checked_sub(after_resume.metabolic_energy()),
        physiology
            .basal_energy_cost_per_tick()
            .checked_add(exertion.energy_cost_per_tick())
    );
    assert_eq!(
        before_resume
            .hydration()
            .checked_sub(after_resume.hydration()),
        physiology
            .hydration_loss_per_tick()
            .checked_add(exertion.hydration_loss_per_tick())
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn suspended_manual_craft_releases_attention_and_waits_while_other_player_work_runs() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work destination mount failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work destination mount commit failed: {error}")
        });
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("manual craft parallel-work start commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft parallel-work active tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("manual craft parallel-work support failure commit failed: {error}")
    });
    let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work suspension tick failed: {error}")
    });
    assert_eq!(state.player_work().active(), None);

    let region = VoxelBounds::new(VoxelCoord::new(10, 0, 0), VoxelCoord::new(11, 1, 1))
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work prospecting bounds failed: {error}")
        });
    validate_start_field_prospecting(
        &registries,
        &state,
        FieldProspectingRequest::new(PROSPECTING_FIELD_INSPECTION, region, MATERIAL_COPPER),
    )
    .unwrap_or_else(|error| panic!("manual craft parallel-work prospecting start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| {
        panic!("manual craft parallel-work prospecting commit failed: {error}")
    });
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work recovery validation failed: {error}")
        })
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("manual craft parallel-work recovery commit failed: {error}")
        });
    let blocked = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
        panic!("manual craft parallel-work blocked-resume tick failed: {error}")
    });
    assert!(matches!(
        blocked.production_availability_changes(),
        [ProductionAvailabilityChange::SuspensionReasonChanged {
            job: changed_job,
            previous: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
        }] if *changed_job == job && *stockpile == destination
    ));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::Prospecting { .. })
    ));

    let prospecting_duration = registries
        .labor()
        .get_prospecting(PROSPECTING_FIELD_INSPECTION)
        .unwrap_or_else(|| panic!("manual craft parallel-work prospecting definition disappeared"))
        .duration()
        .value();
    for _ in 1..prospecting_duration {
        let _ = advance_tick(&registries, &mut state).unwrap_or_else(|error| {
            panic!("manual craft parallel-work prospecting tick failed: {error}")
        });
    }
    assert_eq!(state.player_work().active(), None);

    let resumed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft parallel-work resume tick failed: {error}"));
    assert!(matches!(
        resumed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            ..
        }] if *resumed_job == job
    ));
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn simultaneously_recoverable_manual_crafts_resume_one_at_a_time_in_job_order() {
    let (registries, mut state, source, first_lot, first_destination) = make_fixture();
    let second_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("second manual craft input failed: {error}"));
    let second_destination =
        add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(2_000_000))
            .unwrap_or_else(|error| panic!("second manual craft destination failed: {error}"));
    let first_support = active_stockpile_support_at(&registries, &mut state, 0);
    let second_support = active_stockpile_support_at(&registries, &mut state, 10);
    for (destination, support) in [
        (first_destination, first_support),
        (second_destination, second_support),
    ] {
        let _ = validate_mount_stockpile(&registries, &state, destination, support)
            .unwrap_or_else(|error| panic!("manual craft destination mount failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("manual craft destination mount commit failed: {error}")
            });
    }

    let first = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(first_lot, Mass::from_milligrams(1_000_000)),
            first_destination,
        ),
    )
    .unwrap_or_else(|error| panic!("first serial-resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first serial-resume craft commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("first serial-resume active tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        first_support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("first serial-resume support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("first serial-resume support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("first serial-resume suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    let second = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(second_lot, Mass::from_milligrams(1_000_000)),
            second_destination,
        ),
    )
    .unwrap_or_else(|error| panic!("second serial-resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second serial-resume craft commit failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        second_support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("second serial-resume support failure failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("second serial-resume support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("second serial-resume suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    for destination in [first_destination, second_destination] {
        let _ = validate_unmount_stockpile(&registries, &state, destination)
            .unwrap_or_else(|error| panic!("serial-resume destination recovery failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("serial-resume destination recovery commit failed: {error}")
            });
    }

    let first_resume_at = state.tick();
    let first_remaining = state
        .production()
        .get_job(first)
        .and_then(|job| job.suspension())
        .map(|suspension| suspension.remaining_active_time())
        .unwrap_or_else(|| panic!("first serial-resume job was not suspended before recovery"));
    let first_due = first_resume_at
        .checked_add_span(first_remaining)
        .unwrap_or_else(|| panic!("first serial-resume completion tick overflowed"));
    let recovered = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("serial-resume arbitration tick failed: {error}"));
    assert_eq!(
        recovered.production_availability_changes(),
        &[
            ProductionAvailabilityChange::Resumed {
                job: first,
                reason: ProductionSuspensionReason::OutputSupportUnavailable {
                    stockpile: first_destination,
                },
                resumed_at: first_resume_at,
                scheduled_completion: first_due,
            },
            ProductionAvailabilityChange::SuspensionReasonChanged {
                job: second,
                previous: ProductionSuspensionReason::OutputSupportUnavailable {
                    stockpile: second_destination,
                },
                reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            },
        ]
    );
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job: first })
    );
    assert_eq!(
        state
            .production()
            .get_job(second)
            .and_then(|job| job.suspension())
            .map(|suspension| suspension.reason()),
        Some(ProductionSuspensionReason::PlayerLaborUnavailable)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    while state.production().get_job(first).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("first serial-resume completion failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);
    let second_resume = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("second serial-resume arbitration failed: {error}"));
    assert!(matches!(
        second_resume.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job,
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
            ..
        }] if *job == second
    ));
    assert_eq!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { job: second })
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn one_tick_manual_craft_resume_completes_without_leaking_player_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("one-tick resume destination mount commit failed: {error}"));
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("one-tick resume craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("one-tick resume craft start commit failed: {error}"));
    for _ in 0..39 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("one-tick resume active craft tick failed: {error}"));
    }
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| panic!("one-tick resume support failure validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("one-tick resume support failure commit failed: {error}"));
    let paused = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("one-tick resume suspension tick failed: {error}"));
    assert!(matches!(
        paused.production_availability_changes(),
        [ProductionAvailabilityChange::Suspended {
            job: suspended_job,
            remaining_active_time,
            ..
        }] if *suspended_job == job && *remaining_active_time == TickSpan::new(1)
    ));
    assert_eq!(state.player_work().active(), None);

    let _ = validate_unmount_stockpile(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("one-tick resume recovery validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("one-tick resume recovery commit failed: {error}"));
    let before_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("one-tick resume survival state disappeared"));
    let completed = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("one-tick resume completion tick failed: {error}"));
    assert!(matches!(
        completed.production_availability_changes(),
        [ProductionAvailabilityChange::Resumed {
            job: resumed_job,
            scheduled_completion,
            ..
        }] if *resumed_job == job && *scheduled_completion == state.tick()
    ));
    assert_eq!(
        completed
            .production_completions()
            .iter()
            .map(|completion| completion.job())
            .collect::<Vec<_>>(),
        vec![job]
    );
    assert!(state.production().get_job(job).is_none());
    assert_eq!(state.player_work().active(), None);
    let after_resume = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("one-tick resume post-completion survival state disappeared"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("one-tick resume craft definition disappeared"))
        .exertion();
    assert_eq!(
        before_resume
            .metabolic_energy()
            .checked_sub(after_resume.metabolic_energy()),
        physiology
            .basal_energy_cost_per_tick()
            .checked_add(exertion.energy_cost_per_tick())
    );
    assert_eq!(
        before_resume
            .hydration()
            .checked_sub(after_resume.hydration()),
        physiology
            .hydration_loss_per_tick()
            .checked_add(exertion.hydration_loss_per_tick())
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn stone_knapping_is_timed_conserved_hand_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual craft initial accounting failed: {error}"));
    let survival_before = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state is missing"));
    let resolution = resolve_manual_craft(
        &registries,
        &state,
        &ManualCraftRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
        ),
    )
    .unwrap_or_else(|error| panic!("stone knapping resolution failed: {error}"));
    assert_eq!(resolution.duration(), TickSpan::new(40));
    assert_eq!(
        validate_start_process(&registries, &state, &resolution, source, destination),
        Err(StartProcessError::ManualProcessRequiresPlayerWork {
            process: PROCESS_KNAP_STONE_TOOL,
        })
    );
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stone knapping start failed: {error}"));
    token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stone knapping commit failed: {error}"));
    assert!(matches!(
        state.player_work().active(),
        Some(PlayerWork::ManualProduction { .. })
    ));

    for _ in 0..resolution.duration().value() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("stone knapping tick failed: {error}"));
    }
    assert_eq!(state.player_work().active(), None);

    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("stone knapping destination disappeared"));
    assert_eq!(
        destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_TOOL)),
        Mass::from_milligrams(800_000)
    );
    assert_eq!(
        destination_record.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_CHIP)),
        Mass::from_milligrams(200_000)
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("manual craft final accounting failed: {error}"));
    assert_eq!(matter_before.total(), matter_after.total());
    let survival_after = assess_survival(&registries, &state)
        .unwrap_or_else(|| panic!("manual craft survival state disappeared"));
    let physiology = registries.survival().physiology();
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("stone knapping manual definition disappeared"))
        .exertion();
    assert_eq!(
        survival_before.metabolic_energy().nanojoules()
            - survival_after.metabolic_energy().nanojoules(),
        (physiology.basal_energy_cost_per_tick().nanojoules()
            + exertion.energy_cost_per_tick().nanojoules())
            * u128::from(resolution.duration().value()),
        "manual-craft admission duration must equal the exact number of charged active ticks"
    );
    assert_eq!(
        survival_before.hydration().microliters() - survival_after.hydration().microliters(),
        (physiology.hydration_loss_per_tick().microliters()
            + exertion.hydration_loss_per_tick().microliters())
            * resolution.duration().value(),
        "manual-craft hydration budgeting must match realized active-tick cost"
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("stone knapping final audit failed: {error}"));
}

#[test]
fn manual_craft_requires_enough_metabolic_reserve_to_finish() {
    let (registries, state, source, lot, destination) = make_fixture();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft low-reserve decode failed: {error}"));
    let low_reserve = loaded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("manual craft low-reserve load failed: {error}"));
    let before = low_reserve.clone();

    assert!(matches!(
        validate_start_manual_craft(
            &registries,
            &low_reserve,
            ManualCraftStartRequest::single(
                PROCESS_KNAP_STONE_TOOL,
                source,
                MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
                destination,
            ),
        ),
        Err(StartManualCraftError::Work(
            PlayerWorkStartError::InsufficientMetabolicEnergy { .. }
        ))
    ));
    assert_eq!(low_reserve, before);
}

#[test]
fn manual_craft_commit_rejects_intervening_survival_change() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft survival-stale validation failed: {error}"));
    let expected = state.survival().revision();
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("manual craft survival-stale tick failed: {error}"));
    let before = state.clone();

    assert_eq!(
        token.commit(&mut state),
        Err(ManualCraftCommitError::Work(
            PlayerWorkCommitError::StaleSurvivalRevision {
                expected,
                actual: state.survival().revision(),
            }
        ))
    );
    assert_eq!(state, before);
}

#[test]
fn active_manual_craft_save_requires_enough_metabolic_energy_to_finish() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft save reserve start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft save reserve commit failed: {error}"));
    let record = state
        .production()
        .get_job(job)
        .unwrap_or_else(|| panic!("manual craft save reserve job disappeared"));
    let remaining = TickSpan::new(record.completes_at().value() - state.tick().value());
    let exertion = registries
        .crafting()
        .get_manual(PROCESS_KNAP_STONE_TOOL)
        .unwrap_or_else(|| panic!("manual craft save reserve definition disappeared"))
        .exertion();
    let required = calculate_player_work_resource_budget(
        registries.survival().physiology(),
        exertion,
        remaining,
    )
    .unwrap_or_else(|error| panic!("manual craft save reserve budget failed: {error:?}"))
    .metabolic_energy();
    assert!(required > Energy::from_nanojoules(1));

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft save reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft save reserve decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::InsufficientMetabolicEnergy {
                available: Energy::from_nanojoules(1),
                required,
            }
        )))
    );
}

#[test]
fn suspended_manual_craft_loads_with_depleted_reserves_and_does_not_resume_unsafely() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let support = active_stockpile_support(&registries, &mut state);
    let _ = validate_mount_stockpile(&registries, &state, destination, support)
        .unwrap_or_else(|error| panic!("suspended low-reserve destination mount failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| {
            panic!("suspended low-reserve destination mount commit failed: {error}")
        });
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended low-reserve craft start commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended low-reserve active craft tick failed: {error}"));
    let _ = validate_set_structural_load(
        &registries,
        &state,
        support,
        StructuralLoadKind::Snow,
        Force::from_millinewtons(50_000_000),
    )
    .unwrap_or_else(|error| {
        panic!("suspended low-reserve support failure validation failed: {error}")
    })
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("suspended low-reserve support failure commit failed: {error}"));
    let _ = advance_tick(&registries, &mut state)
        .unwrap_or_else(|error| panic!("suspended low-reserve suspension tick failed: {error}"));
    assert_eq!(state.player_work().active(), None);

    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("suspended low-reserve serialization failed: {error}"));
    encoded["state"]["systems"]["survival"]["player"]["metabolic_energy"] =
        serde_json::json!(1_u64);
    encoded["state"]["systems"]["survival"]["player"]["hydration"] = serde_json::json!(1_u64);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("suspended low-reserve decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended low-reserve state failed trusted load: {error}"));
    assert_eq!(loaded.player_work().active(), None);

    let _ = validate_unmount_stockpile(&registries, &loaded, destination)
        .unwrap_or_else(|error| panic!("suspended low-reserve recovery validation failed: {error}"))
        .commit(&mut loaded)
        .unwrap_or_else(|error| panic!("suspended low-reserve recovery commit failed: {error}"));
    let blocked = advance_tick(&registries, &mut loaded).unwrap_or_else(|error| {
        panic!("suspended low-reserve blocked-resume tick failed: {error}")
    });
    assert!(matches!(
        blocked.production_availability_changes(),
        [ProductionAvailabilityChange::SuspensionReasonChanged {
            job: changed_job,
            previous: ProductionSuspensionReason::OutputSupportUnavailable { stockpile },
            reason: ProductionSuspensionReason::PlayerLaborUnavailable,
        }] if *changed_job == job && *stockpile == destination
    ));
    assert_eq!(loaded.player_work().active(), None);
    assert!(
        loaded
            .production()
            .get_job(job)
            .is_some_and(|record| record.is_suspended())
    );
    assert_eq!(validate_loaded_state(&registries, &loaded), Ok(()));
    let round_trip =
        serde_json::to_vec(&SaveEnvelope::new(&registries, &loaded)).unwrap_or_else(|error| {
            panic!("suspended low-reserve round-trip serialization failed: {error}")
        });
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&round_trip)
        .unwrap_or_else(|error| panic!("suspended low-reserve round-trip decode failed: {error}"));
    decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("suspended low-reserve round-trip load failed: {error}"));
}

#[test]
fn stale_manual_craft_token_reports_labor_revision_conflict_after_prior_work_finishes() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let first = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("first manual craft validation failed: {error}"));
    let stale = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("stale manual craft validation failed: {error}"));
    first
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("first manual craft commit failed: {error}"));
    for _ in 0..40 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("manual craft completion tick failed: {error}"));
    }

    let error = stale
        .commit(&mut state)
        .err()
        .unwrap_or_else(|| panic!("stale manual craft token unexpectedly committed"));

    assert_eq!(
        error,
        ManualCraftCommitError::Work(PlayerWorkCommitError::StaleRevision {
            expected: 0,
            actual: 2,
        })
    );
    assert_eq!(state.player_work().active(), None);
}

#[test]
fn manual_craft_load_audit_rejects_forged_duration() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_KNAP_STONE_TOOL,
            source,
            MaterialLotSelection::new(lot, Mass::from_milligrams(1_000_000)),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("manual craft tamper start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("manual craft tamper commit failed: {error}"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("manual craft tamper serialization failed: {error}"));
    encoded["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["active_duration"] =
        serde_json::json!(41_u64);
    let tampered: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("manual craft tamper decode failed: {error}"));

    assert_eq!(
        tampered.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ManualCraftJob(ManualCraftJobValidationError::DurationMismatch {
                job,
                stored: TickSpan::new(41),
                required: TickSpan::new(40),
            })
        ))
    );

    let mut coordinated = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| {
            panic!("manual craft coordinated tamper serialization failed: {error}")
        });
    coordinated["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["active_duration"] =
        serde_json::json!(39_u64);
    coordinated["state"]["systems"]["production"]["jobs"][job.value().to_string()]["schedule"]["completes_at"] =
        serde_json::json!(39_u64);
    let coordinated: LoadedSaveEnvelope = serde_json::from_value(coordinated)
        .unwrap_or_else(|error| panic!("manual craft coordinated tamper decode failed: {error}"));

    assert_eq!(
        coordinated.into_state(&registries),
        Err(LoadError::InvalidState(
            StateValidationError::ManualCraftJob(ManualCraftJobValidationError::DurationMismatch {
                job,
                stored: TickSpan::new(39),
                required: TickSpan::new(40),
            })
        ))
    );
}

#[test]
fn in_progress_timber_chest_joinery_round_trip_preserves_deterministic_continuation() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7016));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("timber chest joinery survival setup failed: {error}"));
    let chest_mass = Mass::from_milligrams(2_400_000);
    let source = add_solid_stockpile_for_test(&mut state, chest_mass)
        .unwrap_or_else(|error| panic!("timber chest joinery source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, chest_mass)
        .unwrap_or_else(|error| panic!("timber chest joinery destination failed: {error}"));
    let boards = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        chest_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("timber chest joinery board fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("timber chest joinery initial matter audit failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_ASSEMBLE_TIMBER_CHEST,
            source,
            MaterialLotSelection::new(boards, chest_mass),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("timber chest joinery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("timber chest joinery start commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(80))
    );
    for _ in 0..20 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber chest joinery pre-save tick failed: {error}"));
    }

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("timber chest joinery serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("timber chest joinery decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("timber chest joinery trusted load failed: {error}"));
    assert_eq!(loaded, state);

    for _ in 20..80 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("timber chest joinery source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("timber chest joinery loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(state.player_work().active(), None);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_CHEST_BODY))
            }),
        Some(chest_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!(
                "timber chest joinery final matter audit failed: {error}"
            ))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("timber chest joinery final state audit failed: {error}"));
}

#[test]
fn double_wall_chest_joinery_round_trip_preserves_full_cost_and_output() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0xC4AF_7017));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("double-wall chest survival setup failed: {error}"));
    let body_mass = Mass::from_milligrams(4_000_000);
    let source = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall chest source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, body_mass)
        .unwrap_or_else(|error| panic!("double-wall chest destination failed: {error}"));
    let boards = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
        body_mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("double-wall chest board fixture failed: {error}"));
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("double-wall chest matter setup failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::single(
            PROCESS_ASSEMBLE_DOUBLE_WALL_TIMBER_CHEST,
            source,
            MaterialLotSelection::new(boards, body_mass),
            destination,
        ),
    )
    .unwrap_or_else(|error| panic!("double-wall chest joinery start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("double-wall chest joinery commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(120))
    );
    for _ in 0..30 {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("double-wall chest pre-save tick failed: {error}"));
    }
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("double-wall chest serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("double-wall chest decode failed: {error}"));
    let mut loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("double-wall chest trusted load failed: {error}"));
    assert_eq!(loaded, state);

    for _ in 30..120 {
        let expected = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("double-wall chest source tick failed: {error}"));
        let actual = advance_tick(&registries, &mut loaded)
            .unwrap_or_else(|error| panic!("double-wall chest loaded tick failed: {error}"));
        assert_eq!(actual, expected);
    }
    assert_eq!(loaded, state);
    assert_eq!(state.player_work().active(), None);
    assert!(state.production().get_job(job).is_none());
    assert_eq!(
        state
            .inventory()
            .get_stockpile(source)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::ZERO)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(
                    MATERIAL_WOOD,
                    FORM_DOUBLE_WALL_CHEST_BODY,
                ))
            }),
        Some(body_mass)
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("double-wall chest matter final failed: {error}"))
            .total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn repeated_manual_craft_batches_share_one_labor_job_without_discounting_work() {
    let (registries, mut state, source, lot, destination) = make_fixture();
    let merged_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        stone_lump(),
        Mass::from_milligrams(1_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("batch craft second stone fixture failed: {error}"));
    assert_eq!(
        merged_lot, lot,
        "identical manual-craft input must retain one merged persistent lot identity"
    );
    let craft = ManualCraftRequest::single(
        PROCESS_KNAP_STONE_TOOL,
        source,
        MaterialLotSelection::new(lot, Mass::from_milligrams(2_000_000)),
    );
    let resolution = resolve_manual_craft(&registries, &state, &craft)
        .unwrap_or_else(|error| panic!("batch craft resolution failed: {error}"));

    assert_eq!(resolution.input_mass(), Mass::from_milligrams(2_000_000));
    assert_eq!(resolution.duration(), TickSpan::new(80));
    assert_eq!(
        resolution
            .outputs()
            .iter()
            .map(|output| (output.commodity(), output.mass()))
            .collect::<Vec<_>>(),
        vec![
            (
                CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
                Mass::from_milligrams(1_600_000),
            ),
            (
                CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
                Mass::from_milligrams(400_000),
            ),
        ]
    );

    let token = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(craft, destination),
    )
    .unwrap_or_else(|error| panic!("batch craft start failed: {error}"));
    let job = token
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("batch craft commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .map(|record| record.active_duration()),
        Some(TickSpan::new(80))
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("batch craft running audit failed: {error}"));
}
