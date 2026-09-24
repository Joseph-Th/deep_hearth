//! Long-horizon comminution conservation and deterministic-replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
fn run_comminution_soak() -> AppState {
    let fixture = make_fixture(Mass::from_milligrams(300), Condition::PRISTINE);
    let initial_matter = matter_total(&fixture.state);
    let mut state = fixture.state.clone();
    for step in 0..300_u64 {
        let resolved = match resolve_mass(&fixture, &state, Mass::from_milligrams(1)) {
            Ok(resolved) => resolved,
            Err(error) => panic!("comminution soak resolution failed at step {step}: {error}"),
        };
        let duration = resolved.process_resolution().duration();
        let token = match validate_start_process(
            &fixture.registries,
            &state,
            resolved.process_resolution(),
            fixture.source,
            fixture.destination,
        ) {
            Ok(token) => token,
            Err(error) => panic!("comminution soak start failed at step {step}: {error}"),
        };
        if let Err(error) = token.commit(&mut state) {
            panic!("comminution soak commit failed at step {step}: {error}");
        }
        finish_job(&fixture.registries, &mut state, duration);
        if step.is_multiple_of(47) {
            assert_eq!(validate_loaded_state(&fixture.registries, &state), Ok(()));
            assert_eq!(matter_total(&state), initial_matter);
        }
    }
    assert_eq!(matter_total(&state), initial_matter);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(fixture.destination)
            .map(|stockpile| {
                stockpile.get_mass(CommodityKey::new(MATERIAL_COPPER, FORM_CRUSHED))
            }),
        Some(Mass::from_milligrams(300))
    );
    assert_eq!(
        state
            .energy()
            .get_store(fixture.energy_store)
            .map(|store| store.stored()),
        Some(Energy::from_nanojoules(970_000))
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn comminution_soak_preserves_matter_and_deterministic_replay() {
    let first = run_comminution_soak();
    let second = run_comminution_soak();
    assert_eq!(first, second);
}
