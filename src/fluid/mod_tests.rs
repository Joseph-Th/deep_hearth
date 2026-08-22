//! Tests for the sibling mod module; isolated so test-only edits do not invalidate production builds.

use super::*;

const fn twentieth_second_tick() -> PhysicalTickDuration {
    PhysicalTickDuration::from_microseconds(50_000)
}

#[test]
fn fractional_flow_is_conserved_across_repeated_ticks() {
    let tick_duration = twentieth_second_tick();
    let mut remainder = FlowRemainder::ZERO;
    let mut volume = Volume::ZERO;

    for _ in 0..20 {
        let result = match integrate_flow(
            VolumetricFlow::from_microliters_per_second(1),
            TickSpan::new(1),
            tick_duration,
            remainder,
        ) {
            Ok(result) => result,
            Err(error) => panic!("flow integration failed: {error}"),
        };
        volume = match volume.checked_add(result.volume()) {
            Some(value) => value,
            None => panic!("test volume accumulation overflowed"),
        };
        remainder = result.remainder();
    }

    assert_eq!(volume, Volume::from_microliters(1));
    assert_eq!(remainder, FlowRemainder::ZERO);
}

#[test]
fn whole_second_flow_matches_authored_rate_exactly() {
    let result = match integrate_flow(
        VolumetricFlow::from_microliters_per_second(25_000),
        TickSpan::new(20),
        twentieth_second_tick(),
        FlowRemainder::ZERO,
    ) {
        Ok(result) => result,
        Err(error) => panic!("flow integration failed: {error}"),
    };

    assert_eq!(result.volume(), Volume::from_microliters(25_000));
    assert_eq!(result.remainder(), FlowRemainder::ZERO);
}
