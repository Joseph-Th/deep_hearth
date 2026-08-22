//! Tests for the sibling schedule module; isolated so test-only edits do not invalidate production builds.

use super::*;

fn nonzero(value: u64) -> NonZeroU64 {
    match NonZeroU64::new(value) {
        Some(value) => value,
        None => panic!("test interval must be nonzero"),
    }
}

#[test]
fn cadence_due_ticks_are_derived_without_mutable_scheduler_state() {
    let schedule = match PeriodicSchedule::new(nonzero(10), 3) {
        Ok(schedule) => schedule,
        Err(error) => panic!("schedule fixture failed: {error}"),
    };

    assert!(!schedule.is_due(SimulationTick::new(2)));
    assert!(schedule.is_due(SimulationTick::new(3)));
    assert!(schedule.is_due(SimulationTick::new(13)));
    assert_eq!(
        schedule.next_due_at_or_after(SimulationTick::new(4)),
        Ok(SimulationTick::new(13))
    );
    assert_eq!(
        schedule.next_due_after(SimulationTick::new(13)),
        Ok(SimulationTick::new(23))
    );
}

#[test]
fn deserialization_cannot_construct_phase_outside_interval() {
    let encoded = br#"{"interval_ticks":10,"phase_tick":10}"#;
    let result: Result<PeriodicSchedule, _> = serde_json::from_slice(encoded);

    assert!(result.is_err());
}

#[test]
fn next_due_reports_authoritative_tick_overflow() {
    let schedule = match PeriodicSchedule::new(nonzero(10), 9) {
        Ok(schedule) => schedule,
        Err(error) => panic!("schedule fixture failed: {error}"),
    };
    let tick = SimulationTick::new(u64::MAX - 1);

    assert_eq!(
        schedule.next_due_after(tick),
        Err(ScheduleAdvanceError::TickOverflow {
            from: SimulationTick::new(u64::MAX),
        })
    );
}
