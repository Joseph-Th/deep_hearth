//! Primitive treadle and paired-flywheel package coverage.

use super::*;

#[test]
fn treadle_and_paired_flywheel_are_craftable_from_raw_ordinary_materials() {
    let registries = build_registries();
    let mut state = AppState::new(WorldSeed::new(0x1A80_1003));
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("treadle raw-route survival setup failed: {error}"));
    let raw = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(9_000_000))
        .unwrap_or_else(|error| panic!("treadle raw-route source failed: {error}"));
    let shaped = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(9_000_000))
        .unwrap_or_else(|error| panic!("treadle raw-route shaped store failed: {error}"));
    let stone = deposit_lot_for_test(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        Mass::from_milligrams(3_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("treadle raw-route stone failed: {error}"));
    let wood = deposit_lot_for_test(
        &registries,
        &mut state,
        raw,
        CommodityKey::new(MATERIAL_WOOD, FORM_LOG),
        Mass::from_milligrams(6_000_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap_or_else(|error| panic!("treadle raw-route timber failed: {error}"));

    for (process, lot, mass, ticks) in [
        (
            PROCESS_SHAPE_STONE_FLYWHEEL,
            stone,
            Mass::from_milligrams(3_000_000),
            180,
        ),
        (
            PROCESS_SHAPE_WOOD_BOARDS,
            wood,
            Mass::from_milligrams(2_000_000),
            100,
        ),
        (
            PROCESS_SHAPE_WOOD_HANDLE,
            wood,
            Mass::from_milligrams(4_000_000),
            160,
        ),
    ] {
        validate_start_manual_craft(
            &registries,
            &state,
            ManualCraftStartRequest::single(
                process,
                raw,
                MaterialLotSelection::new(lot, mass),
                shaped,
            ),
        )
        .unwrap_or_else(|error| panic!("treadle raw-route craft {process:?} failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("treadle raw-route craft commit failed: {error}"));
        advance_exact(&registries, &mut state, ticks);
    }

    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL))),
        Some(Mass::from_milligrams(2_700_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_BOARD))),
        Some(Mass::from_milligrams(1_600_000))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(shaped)
            .map(|stockpile| stockpile.get_mass(CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE))),
        Some(Mass::from_milligrams(800_000))
    );

    let treadle =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_TIMBER_TREADLE_DRIVE, shaped)
            .unwrap_or_else(|error| panic!("raw-route treadle assembly failed: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("raw-route treadle assembly commit failed: {error}"));
    let paired_store = validate_assemble_energy_store(
        &registries,
        &state,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
        shaped,
    )
    .unwrap_or_else(|error| panic!("raw-route paired flywheel assembly failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("raw-route paired flywheel commit failed: {error}"));
    let requested = Energy::from_nanojoules(500_000_000_000);
    let power = validate_start_manual_power(
        &registries,
        &state,
        ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, treadle, paired_store, requested),
    )
    .unwrap_or_else(|error| panic!("raw-route treadle charging failed: {error}"));
    let duration = power.work().completes_at().value() - state.tick().value();
    power
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("raw-route treadle charging commit failed: {error}"));
    advance_exact(&registries, &mut state, duration);

    assert_eq!(
        state
            .energy()
            .get_store(paired_store)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("raw-route treadle package state audit failed: {error}"));
}

fn assemble_authored_equipment_fixture(
    registries: &Registries,
    state: &mut AppState,
    definition: crate::equipment::EquipmentDefinitionId,
) -> EquipmentId {
    let assembly = registries
        .equipment()
        .get_equipment(definition)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("authored equipment fixture lost its assembly profile"));
    let source = add_solid_stockpile_for_test(state, assembly.input_mass())
        .unwrap_or_else(|error| panic!("authored equipment fixture source failed: {error}"));
    for input in assembly.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("authored equipment fixture material failed: {error}"));
    }
    validate_assemble_equipment(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("authored equipment fixture assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("authored equipment fixture commit failed: {error}"))
}

fn assemble_authored_energy_fixture(
    registries: &Registries,
    state: &mut AppState,
    definition: crate::energy::EnergyStoreDefinitionId,
) -> EnergyStoreId {
    let assembly = registries
        .energy()
        .get_store(definition)
        .and_then(|definition| definition.assembly_profile())
        .unwrap_or_else(|| panic!("authored energy fixture lost its assembly profile"));
    let source = add_solid_stockpile_for_test(state, assembly.input_mass())
        .unwrap_or_else(|error| panic!("authored energy fixture source failed: {error}"));
    for input in assembly.inputs() {
        deposit_lot_for_test(
            registries,
            state,
            source,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap_or_else(|error| panic!("authored energy fixture material failed: {error}"));
    }
    validate_assemble_energy_store(registries, state, definition, source)
        .unwrap_or_else(|error| panic!("authored energy fixture assembly failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("authored energy fixture commit failed: {error}"))
}

#[test]
fn treadle_package_trades_more_material_for_less_copper_free_charging_attention() {
    let registries = build_registries();
    let requested = Energy::from_nanojoules(500_000_000_000);

    let mut hand_state = AppState::new(WorldSeed::new(0x1A80_1001));
    initialize_player_survival(&registries, &mut hand_state)
        .unwrap_or_else(|error| panic!("hand comparison survival setup failed: {error}"));
    let hand_crank = assemble_crank_fixture(
        &registries,
        &mut hand_state,
        EQUIPMENT_STONE_HAND_CRANK,
        false,
    );
    let hand_store = assemble_flywheel_fixture(&registries, &mut hand_state);
    let hand = validate_start_manual_power(
        &registries,
        &hand_state,
        ManualPowerRequest::new(MANUAL_POWER_HAND_CRANK, hand_crank, hand_store, requested),
    )
    .unwrap_or_else(|error| panic!("hand comparison power validation failed: {error}"));

    let mut treadle_state = AppState::new(WorldSeed::new(0x1A80_1002));
    initialize_player_survival(&registries, &mut treadle_state)
        .unwrap_or_else(|error| panic!("treadle comparison survival setup failed: {error}"));
    let treadle = assemble_authored_equipment_fixture(
        &registries,
        &mut treadle_state,
        EQUIPMENT_TIMBER_TREADLE_DRIVE,
    );
    let paired_store = assemble_authored_energy_fixture(
        &registries,
        &mut treadle_state,
        ENERGY_PAIRED_STONE_FLYWHEEL_DRIVE,
    );
    let treadle_work = validate_start_manual_power(
        &registries,
        &treadle_state,
        ManualPowerRequest::new(MANUAL_POWER_FOOT_TREADLE, treadle, paired_store, requested),
    )
    .unwrap_or_else(|error| panic!("treadle comparison power validation failed: {error}"));

    let hand_duration = hand.work().completes_at().value() - hand_state.tick().value();
    let treadle_duration =
        treadle_work.work().completes_at().value() - treadle_state.tick().value();
    assert_eq!(hand_duration, 3);
    assert_eq!(treadle_duration, 2);
    assert!(
        treadle_work.resource_budget().metabolic_energy()
            < hand.resource_budget().metabolic_energy()
    );
    assert!(treadle_work.resource_budget().hydration() < hand.resource_budget().hydration());

    treadle_work
        .commit(&mut treadle_state)
        .unwrap_or_else(|error| panic!("treadle comparison commit failed: {error}"));
    advance_exact(&registries, &mut treadle_state, treadle_duration);
    assert_eq!(
        treadle_state
            .energy()
            .get_store(paired_store)
            .map(EnergyStoreRecord::stored),
        Some(requested)
    );
    validate_loaded_state(&registries, &treadle_state)
        .unwrap_or_else(|error| panic!("treadle comparison trusted-load audit failed: {error}"));
}
