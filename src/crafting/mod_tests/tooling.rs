//! Woodworking and treadle-tool manual-crafting contracts.

use std::num::NonZeroU64;

use super::*;

#[test]
fn woodworking_adze_reduces_board_attention_without_changing_yield_and_replays_exactly() {
    let registries = build_registries();
    let mut state = AppState::new();
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
    let projected = project_manual_craft_equipment(
        &registries,
        PROCESS_SHAPE_WOOD_BOARDS,
        NonZeroU64::new(1).unwrap_or_else(|| unreachable!("one batch is nonzero")),
        EQUIPMENT_STONE_WOODWORKING_ADZE,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("adze board shaping projection failed: {error}"));
    assert_eq!(hand.duration(), TickSpan::new(50));
    assert_eq!(assisted.duration(), TickSpan::new(28));
    assert_eq!(projected.duration(), assisted.duration());
    assert_eq!(
        projected.condition_after(),
        assisted
            .equipment_condition_after()
            .unwrap_or_else(|| panic!("assisted craft lost equipment outcome"))
    );
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
fn treadle_hammer_reduces_copper_work_attention_without_changing_yield() {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("treadle-hammer survival setup failed: {error}"));

    let components = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(4_400_000))
        .unwrap_or_else(|error| panic!("treadle-hammer component stockpile failed: {error}"));
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_BOARD),
            Mass::from_milligrams(3_200_000),
        ),
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(400_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            components,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("treadle-hammer component failed: {error}"));
    }
    let hammer = validate_assemble_equipment(
        &registries,
        &state,
        EQUIPMENT_TIMBER_TREADLE_HAMMER,
        components,
    )
    .unwrap_or_else(|error| panic!("treadle-hammer assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("treadle-hammer assembly commit failed: {error}"));

    let source = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(40_000))
        .unwrap_or_else(|error| panic!("treadle-hammer copper source failed: {error}"));
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(40_000))
        .unwrap_or_else(|error| panic!("treadle-hammer destination failed: {error}"));
    let hand_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("treadle-hammer first copper lot failed: {error}"));
    let assisted_lot = deposit_lot_for_test(
        &registries,
        &mut state,
        source,
        CommodityKey::new(MATERIAL_COPPER, FORM_NATIVE_METAL),
        Mass::from_milligrams(20_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("treadle-hammer second copper lot failed: {error}"));

    let hand = resolve_manual_craft(
        &registries,
        &state,
        &ManualCraftRequest::single(
            PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
            source,
            MaterialLotSelection::new(hand_lot, Mass::from_milligrams(20_000)),
        ),
    )
    .unwrap_or_else(|error| panic!("hand copper-work resolution failed: {error}"));
    let assisted_request = ManualCraftRequest::single(
        PROCESS_COLD_WORK_COPPER_REINFORCEMENT,
        source,
        MaterialLotSelection::new(assisted_lot, Mass::from_milligrams(20_000)),
    )
    .with_equipment(hammer);
    let assisted = resolve_manual_craft(&registries, &state, &assisted_request)
        .unwrap_or_else(|error| panic!("treadle-hammer copper-work resolution failed: {error}"));
    assert_eq!(hand.duration(), TickSpan::new(40));
    assert_eq!(assisted.duration(), TickSpan::new(14));
    assert_eq!(assisted.outputs(), hand.outputs());

    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("treadle-hammer matter-before audit failed: {error}"))
        .total();
    let job = validate_start_manual_craft(
        &registries,
        &state,
        ManualCraftStartRequest::new(assisted_request, destination),
    )
    .unwrap_or_else(|error| panic!("treadle-hammer start failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("treadle-hammer start commit failed: {error}"));
    assert_eq!(
        state
            .production()
            .get_job(job)
            .and_then(|record| record.equipment_provider())
            .map(|trace| trace.equipment()),
        Some(hammer)
    );

    while state.production().get_job(job).is_some() {
        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("treadle-hammer work tick failed: {error}"));
    }
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_REINFORCEMENT))
            }),
        Some(Mass::from_milligrams(20_000))
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(hammer)
            .map(|record| record.condition()),
        Some(
            Condition::new(998_600)
                .unwrap_or_else(|error| panic!("treadle-hammer condition failed: {error}"))
        )
    );
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("treadle-hammer matter-after audit failed: {error}"))
            .total(),
        matter_before
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("treadle-hammer final state audit failed: {error}"));
}
