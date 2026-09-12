//! Contract tests for shared mass-flow duration and capacity arithmetic.

use crate::core::quantity::{Mass, MassFlow};
use crate::core::time::{PhysicalTickDuration, TickSpan};

use super::{
    MassFlowDurationError, calculate_mass_flow_capacity, calculate_mass_flow_duration_ceiling,
};

#[test]
fn duration_returns_first_tick_that_can_finish_batch() {
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
fn duration_rejects_zero_rate_and_preserves_zero_mass() {
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

#[test]
fn capacity_is_the_exact_whole_mass_inverse_of_duration() {
    let tick_duration = PhysicalTickDuration::from_microseconds(50_000);
    let rate = MassFlow::from_milligrams_per_second(30);
    assert_eq!(
        calculate_mass_flow_capacity(rate, TickSpan::new(1), tick_duration),
        Mass::from_milligrams(1)
    );
    let two_tick_capacity = calculate_mass_flow_capacity(rate, TickSpan::new(2), tick_duration);
    assert_eq!(two_tick_capacity, Mass::from_milligrams(3));
    assert_eq!(
        calculate_mass_flow_duration_ceiling(rate, two_tick_capacity, tick_duration),
        Ok(TickSpan::new(2))
    );
    assert_eq!(
        calculate_mass_flow_duration_ceiling(
            rate,
            Mass::from_milligrams(two_tick_capacity.milligrams() + 1),
            tick_duration,
        ),
        Ok(TickSpan::new(3))
    );
}

#[test]
fn capacity_clamps_only_beyond_single_batch_representation() {
    assert_eq!(
        calculate_mass_flow_capacity(
            MassFlow::from_milligrams_per_second(u64::MAX),
            TickSpan::new(u64::MAX),
            PhysicalTickDuration::from_microseconds(u64::MAX),
        ),
        Mass::from_milligrams(u64::MAX)
    );
}
