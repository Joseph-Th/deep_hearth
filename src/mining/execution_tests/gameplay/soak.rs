//! Long-horizon mining depletion, conservation, persistence, and replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
fn run_mining_soak() -> AppState {
    let registries = build_registries();
    let mut state = AppState::new();
    initialize_player_survival(&registries, &mut state)
        .unwrap_or_else(|error| panic!("mining soak survival initialization failed: {error}"));
    let pick = assemble_pick_for_test(&registries, &mut state);
    let destination = add_solid_stockpile_for_test(&mut state, Mass::from_milligrams(1_000_000))
        .unwrap_or_else(|error| panic!("mining soak destination failed: {error}"));
    let deposit = insert_known_deposit(&registries, &mut state, deposit_spec())
        .unwrap_or_else(|error| panic!("mining soak deposit failed: {error}"));
    let initial_matter = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("mining soak matter accounting failed: {error}"))
        .total();
    let initial_energy = calculate_explicit_energy_accounting(&registries, &state)
        .unwrap_or_else(|error| panic!("mining soak energy accounting failed: {error}"))
        .total()
        .unwrap_or_else(|| panic!("mining soak energy total overflowed"));

    for step in 0_u64..1_000 {
        let job = validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(1_000),
        )
        .unwrap_or_else(|error| panic!("mining soak start failed at step {step}: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("mining soak start commit failed at step {step}: {error}"));

        if step == 500 {
            let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
                .unwrap_or_else(|error| panic!("mining soak save failed: {error}"));
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("mining soak decode failed: {error}"));
            state = decoded
                .into_state(&registries)
                .unwrap_or_else(|error| panic!("mining soak active-job load failed: {error}"));
        }

        let job_record = state
            .mining()
            .get_job(job)
            .unwrap_or_else(|| panic!("mining soak job disappeared at step {step}"));
        let duration = job_record
            .completes_at()
            .value()
            .checked_sub(job_record.started_at().value())
            .unwrap_or_else(|| panic!("mining soak duration underflowed at step {step}"));
        assert!(duration > 0);
        for _ in 0..duration {
            let _ = advance_tick(&registries, &mut state)
                .unwrap_or_else(|error| panic!("mining soak tick failed at step {step}: {error}"));
        }
        validate_claim_mining_output(&registries, &state, job)
            .unwrap_or_else(|error| panic!("mining soak claim failed at step {step}: {error}"))
            .commit(&mut state)
            .unwrap_or_else(|error| {
                panic!("mining soak claim commit failed at step {step}: {error}")
            });

        if step.is_multiple_of(97) {
            validate_loaded_state(&registries, &state).unwrap_or_else(|error| {
                panic!("mining soak exhaustive audit failed at step {step}: {error}")
            });
            assert_eq!(
                calculate_matter_accounting(&state)
                    .unwrap_or_else(|error| panic!("mining soak matter audit failed: {error}"))
                    .total(),
                initial_matter
            );
            assert_eq!(
                calculate_explicit_energy_accounting(&registries, &state)
                    .unwrap_or_else(|error| panic!("mining soak energy audit failed: {error}"))
                    .total(),
                Some(initial_energy)
            );
        }
    }

    assert_eq!(
        state
            .geology()
            .get_deposit(deposit)
            .unwrap_or_else(|| panic!("mining soak deposit disappeared"))
            .lifecycle(),
        GeologicalDepositLifecycle::Depleted
    );
    let destination_record = state
        .inventory()
        .get_stockpile(destination)
        .unwrap_or_else(|| panic!("mining soak destination disappeared"));
    assert_eq!(
        destination_record.stored_mass(),
        Mass::from_milligrams(1_000_000)
    );
    assert_eq!(state.inventory().lot_ids(destination).count(), 1);
    assert_eq!(state.mining().jobs().count(), 0);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state)
            .unwrap_or_else(|error| panic!("mining soak final matter audit failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        calculate_explicit_energy_accounting(&registries, &state)
            .unwrap_or_else(|error| panic!("mining soak final energy audit failed: {error}"))
            .total(),
        Some(initial_energy)
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn mining_soak_preserves_depletion_conservation_persistence_and_replay() {
    let first = run_mining_soak();
    let second = run_mining_soak();

    assert_eq!(first, second);
}
