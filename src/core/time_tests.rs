//! Tests for the sibling time module; isolated so test-only edits do not invalidate production builds.

use super::*;

#[test]
fn absolute_tick_and_relative_span_add_without_wraparound() {
    assert_eq!(
        SimulationTick::new(10).checked_add_span(TickSpan::new(7)),
        Some(SimulationTick::new(17))
    );
    assert_eq!(
        SimulationTick::new(u64::MAX).checked_add_span(TickSpan::new(1)),
        None
    );
}
