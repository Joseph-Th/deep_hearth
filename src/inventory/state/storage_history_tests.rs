//! Focused regressions for rational storage-exposure projection semantics.

use super::*;

#[test]
fn equal_current_age_with_different_projection_phase_is_not_equivalent() {
    let preservation_multiplier_ppm = 3_000_000;
    let at = SimulationTick::new(1);
    let from_origin = MaterialStorageHistory::new(SimulationTick::ZERO);
    let rebased_same_age = MaterialStorageHistory::with_ambient_age_parts(333_334, at);

    assert_eq!(
        from_origin.project(at, preservation_multiplier_ppm),
        rebased_same_age.project(at, preservation_multiplier_ppm)
    );
    assert_eq!(
        from_origin.is_projection_equivalent(rebased_same_age, at, preservation_multiplier_ppm),
        Some(false),
        "equal rounded age is insufficient when rational projection phases differ"
    );
    assert_ne!(
        from_origin.project(SimulationTick::new(2), preservation_multiplier_ppm),
        rebased_same_age.project(SimulationTick::new(2), preservation_multiplier_ppm),
        "different phases must demonstrate a future projection divergence"
    );
}

#[test]
fn equal_age_with_equal_projection_phase_remains_equivalent() {
    let preservation_multiplier_ppm = 3_000_000;
    let at = SimulationTick::new(3);
    let from_origin = MaterialStorageHistory::new(SimulationTick::ZERO);
    let rebased_same_phase = MaterialStorageHistory::with_ambient_age_parts(1_000_000, at);

    assert_eq!(
        from_origin.is_projection_equivalent(rebased_same_phase, at, preservation_multiplier_ppm),
        Some(true)
    );
    for future in 3..=9 {
        assert_eq!(
            from_origin.project(SimulationTick::new(future), preservation_multiplier_ppm),
            rebased_same_phase.project(SimulationTick::new(future), preservation_multiplier_ppm),
            "equivalent projection phases must remain equal at tick {future}"
        );
    }
}

#[test]
fn projected_age_horizon_returns_first_tick_that_reaches_target() {
    let preservation_multiplier_ppm = 3_000_004;
    let history = MaterialStorageHistory::new(SimulationTick::ZERO);
    let at = SimulationTick::new(1);
    let target_age_parts = 1_000_000;

    assert_eq!(
        history.ticks_until_projected_age(at, preservation_multiplier_ppm, target_age_parts),
        Some(TickSpan::new(3))
    );
    assert!(
        history
            .project(SimulationTick::new(3), preservation_multiplier_ppm)
            .is_some_and(|age| age < target_age_parts)
    );
    assert!(
        history
            .project(SimulationTick::new(4), preservation_multiplier_ppm)
            .is_some_and(|age| age >= target_age_parts)
    );
}
