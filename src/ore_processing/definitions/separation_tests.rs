//! Exact inverse batch-recovery projections at the represented mass boundary.

use super::*;

#[test]
fn inverse_batch_recovery_finds_feasible_feed_even_when_doubling_would_overflow() {
    let target = Mass::from_milligrams(u64::MAX / 2);
    let batch_limit = Mass::from_milligrams(1_000_001);
    let constituent_ppm = 999_999;
    let recovery_ppm = 1_000_000;

    let minimum = minimum_batched_homogeneous_feed_mass_for_target_recovery(
        target,
        constituent_ppm,
        recovery_ppm,
        batch_limit,
    )
    .expect("a representable feed should recover half the maximum represented mass");
    assert!(
        project_batched_homogeneous_target_recovery(
            minimum,
            constituent_ppm,
            recovery_ppm,
            batch_limit,
        )
        .is_some_and(|recovered| recovered >= target)
    );
    let previous = minimum
        .checked_sub(Mass::from_milligrams(1))
        .expect("positive target needs positive feed");
    assert!(
        project_batched_homogeneous_target_recovery(
            previous,
            constituent_ppm,
            recovery_ppm,
            batch_limit,
        )
        .is_some_and(|recovered| recovered < target),
        "the result must be the least feasible feed, not merely a feasible upper bound"
    );
}

#[test]
fn inverse_batch_recovery_rejects_target_beyond_all_representable_feed() {
    let target = Mass::from_milligrams(u64::MAX / 2 + 1);
    let batch_limit = Mass::from_milligrams(2);
    assert_eq!(
        minimum_batched_homogeneous_feed_mass_for_target_recovery(
            target,
            999_999,
            1_000_000,
            batch_limit,
        ),
        None,
    );
}
