//! Long-horizon constituent-separation conservation and deterministic-replay coverage.

use super::*;

fn run_concentration_soak() -> AppState {
    const OPERATIONS: u64 = 200;
    const BATCH_MILLIGRAMS: u64 = 1_000;

    let total_mass = Mass::from_milligrams(OPERATIONS * BATCH_MILLIGRAMS);
    let composition = MaterialComposition::new(vec![
        CompositionComponent::new(MATERIAL_COPPER, 410_000),
        CompositionComponent::new(MATERIAL_STONE, 370_000),
        CompositionComponent::new(MATERIAL_SLAG, 220_000),
    ])
    .unwrap_or_else(|error| panic!("concentration soak composition failed: {error}"));
    let mut fixture = concentration_fixture(total_mass, composition);
    let initial_matter = calculate_matter_accounting(&fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak matter accounting failed: {error}"))
        .total();
    let initial_energy = fixture
        .state
        .energy()
        .get_store(fixture.energy)
        .unwrap_or_else(|| panic!("concentration soak energy store disappeared"))
        .stored();
    let mut expected_target = Mass::ZERO;
    let mut expected_residue = Mass::ZERO;
    let mut expected_energy = Energy::ZERO;

    for operation in 0..OPERATIONS {
        let batch = Mass::from_milligrams(BATCH_MILLIGRAMS);
        let resolved = resolve_process(&fixture, PROCESS_CONCENTRATE_COPPER, batch);
        expected_target = expected_target
            .checked_add(resolved.target_mass())
            .unwrap_or_else(|| panic!("concentration soak target mass overflowed"));
        expected_residue = expected_residue
            .checked_add(resolved.residue_mass())
            .unwrap_or_else(|| panic!("concentration soak residue mass overflowed"));
        expected_energy = expected_energy
            .checked_add(resolved.required_energy())
            .unwrap_or_else(|| panic!("concentration soak energy accounting overflowed"));
        let duration = resolved.process_resolution().duration();
        validate_start_process_routed(
            &fixture.registries,
            &fixture.state,
            resolved.process_resolution(),
            fixture.source,
            &[
                ProcessOutputRoute::new(
                    ConstituentSeparationProcessDefinition::TARGET_STREAM,
                    fixture.target,
                ),
                ProcessOutputRoute::new(
                    ConstituentSeparationProcessDefinition::RESIDUE_STREAM,
                    fixture.residue,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("concentration soak start failed: {error}"))
        .commit(&mut fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak commit failed: {error}"));

        if operation == OPERATIONS / 2 {
            let encoded =
                serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state))
                    .unwrap_or_else(|error| {
                        panic!("concentration soak serialization failed: {error}")
                    });
            let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
                .unwrap_or_else(|error| panic!("concentration soak decode failed: {error}"));
            fixture.state = decoded
                .into_state(&fixture.registries)
                .unwrap_or_else(|error| panic!("concentration soak resume failed: {error}"));
        }

        for _ in 0..duration.value() {
            let _ = advance_tick(&fixture.registries, &mut fixture.state)
                .unwrap_or_else(|error| panic!("concentration soak completion failed: {error}"));
        }
        if operation.is_multiple_of(25) {
            validate_loaded_state(&fixture.registries, &fixture.state)
                .unwrap_or_else(|error| panic!("concentration soak audit failed: {error}"));
        }
    }

    validate_loaded_state(&fixture.registries, &fixture.state)
        .unwrap_or_else(|error| panic!("concentration soak final audit failed: {error}"));
    assert_eq!(
        calculate_matter_accounting(&fixture.state)
            .unwrap_or_else(|error| panic!("concentration soak final matter failed: {error}"))
            .total(),
        initial_matter
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.source)
            .unwrap_or_else(|| panic!("concentration soak source disappeared"))
            .stored_mass(),
        Mass::ZERO
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.target)
            .unwrap_or_else(|| panic!("concentration soak target disappeared"))
            .stored_mass(),
        expected_target
    );
    assert_eq!(
        fixture
            .state
            .inventory()
            .get_stockpile(fixture.residue)
            .unwrap_or_else(|| panic!("concentration soak residue disappeared"))
            .stored_mass(),
        expected_residue
    );
    assert_eq!(
        expected_target.checked_add(expected_residue),
        Some(total_mass)
    );
    assert_eq!(
        fixture
            .state
            .energy()
            .get_store(fixture.energy)
            .unwrap_or_else(|| panic!("concentration soak energy store disappeared"))
            .stored(),
        initial_energy
            .checked_sub(expected_energy)
            .unwrap_or_else(|| panic!("concentration soak consumed more energy than available"))
    );
    fixture.state
}

#[test]
#[ignore = "long-horizon soak"]
fn concentration_soak_preserves_selectivity_conservation_persistence_and_replay() {
    let first = run_concentration_soak();
    let second = run_concentration_soak();
    assert_eq!(first, second);
}
