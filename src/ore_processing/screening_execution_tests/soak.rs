//! Long-horizon screening conservation and deterministic-replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
fn run_screening_soak() -> AppState {
    const OPERATIONS: u64 = 300;
    const BATCH_MILLIGRAMS: u64 = 10;
    let registries = registries(Length::from_micrometers(2_000));
    let mut state = AppState::new();
    let total_mass = Mass::from_milligrams(OPERATIONS * BATCH_MILLIGRAMS);
    let source = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak source failed: {error}"));
    let undersize = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak undersize storage failed: {error}"));
    let oversize = add_solid_stockpile_for_test(&mut state, total_mass)
        .unwrap_or_else(|error| panic!("screening soak oversize storage failed: {error}"));
    let input = MaterialLotSpec::with_composition_and_particle_size(
        CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED),
        total_mass,
        TEMPERATURE,
        composition(),
        distribution(),
    )
    .unwrap_or_else(|error| panic!("screening soak input failed: {error}"));
    let lot = deposit_lot_spec_for_test(&registries, &mut state, source, input)
        .unwrap_or_else(|error| panic!("screening soak lot seed failed: {error}"));
    let equipment = add_equipment(&registries, &mut state, SCREEN, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("screening soak equipment failed: {error}"));
    let energy = add_energy_store_with_initial_for_fixture(
        &registries,
        &mut state,
        ENERGY_STORE,
        Energy::from_nanojoules(1_000_000),
    )
    .unwrap_or_else(|error| panic!("screening soak energy failed: {error}"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("screening soak matter accounting failed: {error}"))
        .total();

    for operation in 0..OPERATIONS {
        let selection = [MaterialLotSelection::new(
            lot,
            Mass::from_milligrams(BATCH_MILLIGRAMS),
        )];
        let resolved = resolve_screening_process(
            &registries,
            &state,
            ScreeningRequest::new(PROCESS, source, &selection, equipment, energy),
        )
        .unwrap_or_else(|error| panic!("screening soak resolution failed: {error}"));
        assert_eq!(resolved.undersize_mass(), Mass::from_milligrams(4));
        assert_eq!(resolved.oversize_mass(), Mass::from_milligrams(6));
        assert_eq!(resolved.process_resolution().duration(), TickSpan::new(1));
        let start = validate_start_process_routed(
            &registries,
            &state,
            resolved.process_resolution(),
            source,
            &[
                ProcessOutputRoute::new(ScreeningProcessDefinition::UNDERSIZE_STREAM, undersize),
                ProcessOutputRoute::new(ScreeningProcessDefinition::OVERSIZE_STREAM, oversize),
            ],
        )
        .unwrap_or_else(|error| panic!("screening soak start failed: {error}"));
        start
            .commit(&mut state)
            .unwrap_or_else(|error| panic!("screening soak commit failed: {error}"));

        if operation == OPERATIONS / 2 {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| panic!("screening soak serialization failed: {error}"));
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("screening soak decode failed: {error}"));
            state = decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("screening soak resume failed: {error}"));
        }

        let _ = advance_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("screening soak completion failed: {error}"));
        if operation % 25 == 0 {
            validate_loaded_state(&registries, &state)
                .unwrap_or_else(|error| panic!("screening soak audit failed: {error}"));
        }
    }

    validate_loaded_state(&registries, &state)
        .unwrap_or_else(|error| panic!("screening soak final audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("screening soak final matter failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        state
            .energy()
            .get_store(energy)
            .unwrap_or_else(|| panic!("screening soak energy store disappeared"))
            .stored(),
        Energy::from_nanojoules(700_000)
    );
    assert_eq!(
        state
            .equipment()
            .get_equipment(equipment)
            .unwrap_or_else(|| panic!("screening soak equipment disappeared"))
            .condition(),
        Condition::new(700_000)
            .unwrap_or_else(|error| panic!("screening soak final condition failed: {error}"))
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(undersize)
            .unwrap_or_else(|| panic!("screening soak undersize storage disappeared"))
            .stored_mass(),
        Mass::from_milligrams(1_200)
    );
    assert_eq!(
        state
            .inventory()
            .get_stockpile(oversize)
            .unwrap_or_else(|| panic!("screening soak oversize storage disappeared"))
            .stored_mass(),
        Mass::from_milligrams(1_800)
    );
    assert_eq!(state.inventory().lot_ids(undersize).count(), 1);
    assert_eq!(state.inventory().lot_ids(oversize).count(), 1);
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn screening_soak_preserves_conservation_persistence_and_replay() {
    let first = run_screening_soak();
    let second = run_screening_soak();
    assert_eq!(first, second);
}
