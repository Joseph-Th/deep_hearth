//! Long-horizon prospecting index, persistence, and deterministic-replay coverage.

use super::*;

#[cfg(feature = "test-soak")]
fn run_prospecting_soak() -> AppState {
    let registries = build_registries();
    let mut state = AppState::new();
    let method = registries
        .labor()
        .get_prospecting(PROSPECTING_LOCAL_TRANSECT)
        .copied()
        .unwrap_or_else(|| panic!("local-transect prospecting definition disappeared"));
    assert_eq!(method.evidence(), GeologicalEvidenceKind::SurfaceExposure);
    for step in 0_u32..2_000 {
        let x = i64::from(step % 64);
        let material = if step.is_multiple_of(2) {
            MATERIAL_COPPER
        } else {
            MATERIAL_SLAG
        };
        let region = line_bounds(x, x + 2);
        assert_eq!(method.resolve_region_observation_count(region), Ok(1));
        let upper = method
            .abundance_uncertainty_ppm()
            .saturating_add((step.wrapping_mul(7919)) % 900_000)
            .min(1_000_000);
        let resolution = make_test_prospecting_resolution(
            region,
            method.evidence(),
            vec![estimate(material, 0, upper)],
        );
        record(&registries, &mut state, resolution);
        if let Err(error) = advance_tick(&registries, &mut state) {
            panic!("prospecting soak tick failed at step {step}: {error}");
        }
        if step.is_multiple_of(97)
            && let Err(error) = validate_loaded_state(&registries, &state)
        {
            panic!("prospecting soak exhaustive audit failed at step {step}: {error}");
        }
    }
    assert_eq!(state.geological_knowledge().observations().count(), 2_000);
    let full_region = bounds(0, 65);
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), full_region, MATERIAL_COPPER,)
            .observations()
            .len(),
        1_000
    );
    assert_eq!(
        assess_geological_knowledge(state.geological_knowledge(), full_region, MATERIAL_SLAG)
            .observations()
            .len(),
        1_000
    );
    state
}

#[cfg(feature = "test-soak")]
#[test]
#[ignore = "long-horizon soak"]
fn prospecting_soak_preserves_indexes_persistence_invariants_and_replay() {
    let first = run_prospecting_soak();
    let second = run_prospecting_soak();
    assert_eq!(first, second);
}
