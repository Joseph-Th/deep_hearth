//! Manual-craft registry, copper/stone recovery, contamination, and thermal-selection contracts.

use super::*;

#[test]
fn manual_craft_registry_rejects_output_that_requires_unauthored_particle_state() {
    let registries = build_registries();
    let process = ProcessId::new(880_001);
    let input = CommodityKey::new(MATERIAL_COPPER, FORM_INGOT);
    let output = CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED);
    let input_mass = Mass::from_milligrams(1);
    let mut production = ProductionRegistry::new();
    production.register_process(ProcessDefinition::new(
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
