//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use crate::core::quantity::{Mass, MassFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};

use super::{MassFlowDurationError, calculate_mass_flow_duration_ceiling};

#[test]
fn mass_flow_duration_returns_first_tick_that_can_finish_batch() {
    let tick_duration = PhysicalTickDuration::from_microseconds(50_000);
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(30),
            Mass::from_milligrams(3),
            tick_duration,
        ),
        Ok(TickSpan::new(2))
    );
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(60),
            Mass::from_milligrams(3),
            tick_duration,
        ),
        Ok(TickSpan::new(1))
    );
}

#[test]
fn mass_flow_duration_rejects_zero_rate_and_preserves_zero_mass() {
    let tick_duration = PhysicalTickDuration::from_microseconds(50_000);
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::ZERO,
            Mass::from_milligrams(1),
            tick_duration,
        ),
        Err(MassFlowDurationError::ZeroRate)
    );
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            MassFlow::from_milligrams_per_second(1),
            Mass::ZERO,
            tick_duration,
        ),
        Ok(TickSpan::ZERO)
    );
}
