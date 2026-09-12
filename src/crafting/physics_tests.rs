//! Shared manual-craft schedule physics regressions.

use super::*;

#[test]
fn equipment_schedule_binds_throughput_duration_and_wear_once() {
    let schedule = resolve_manual_craft_equipment_schedule(
        MassFlow::from_milligrams_per_second(10),
        Mass::from_milligrams(100),
        PhysicalTickDuration::from_microseconds(1_000_000),
        100_000,
        Condition::PRISTINE,
    )
    .unwrap_or_else(|error| panic!("bounded craft schedule failed: {error:?}"));

    assert_eq!(schedule.duration(), TickSpan::new(10));
    assert_eq!(schedule.condition_after(), Condition::FAILED);
}

#[test]
fn hand_duration_scales_exact_integral_batches() {
    let batches = NonZeroU64::new(3).unwrap_or_else(|| unreachable!("three is nonzero"));
    assert_eq!(
        resolve_manual_craft_hand_duration(TickSpan::new(7), batches),
        Some(TickSpan::new(21))
    );
}
