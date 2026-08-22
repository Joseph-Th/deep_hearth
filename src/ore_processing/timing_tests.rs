//! Tests for the sibling timing module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn power_limited_time_is_active_time_for_condition_wear() {
    let timing = OreProcessActiveTiming::new(TickSpan::new(1), TickSpan::new(6));
    let after = timing
        .condition_after(1_000, Condition::PRISTINE)
        .unwrap_or_else(|error| panic!("power-limited wear calculation failed: {error}"));
    let expected = Condition::new(994_000)
        .unwrap_or_else(|error| panic!("expected condition fixture failed: {error}"));

    assert_eq!(timing.duration(), TickSpan::new(6));
    assert_eq!(after, expected);
}
