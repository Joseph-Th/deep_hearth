//! Clock integration contracts for the immutable runtime definitions.

use super::CoreDefinitions;
use crate::core::quantity::Acceleration;
use crate::core::time::{
    CONTROL_TICK_DURATION, CONTROL_TICKS_PER_SIMULATION_TICK, CalendarDefinition,
};

#[test]
fn authored_runtime_clock_matches_exact_control_time() {
    let registries = crate::content::build_registries();
    assert_eq!(
        registries.core().physical_tick_duration().microseconds(),
        CONTROL_TICK_DURATION.microseconds() * CONTROL_TICKS_PER_SIMULATION_TICK,
    );
}

#[test]
fn registry_cannot_mix_a_different_physics_clock_with_fixed_control_motion() {
    let rejected = std::panic::catch_unwind(|| {
        CoreDefinitions::new(
            Acceleration::from_micrometers_per_second_squared(9_806_650),
            CalendarDefinition::new(2_400, 86_400, 8, 12),
        )
    });
    assert!(
        rejected.is_err(),
        "incompatible physical and control clocks must be rejected"
    );
}

#[test]
fn core_accepts_a_calendar_with_matching_physical_tick_and_different_seasons() {
    let core = CoreDefinitions::new(
        Acceleration::from_micrometers_per_second_squared(9_806_650),
        CalendarDefinition::new(24_000, 86_400, 4, 8),
    );
    assert_eq!(core.calendar().days_per_month(), 4);
}
