//! Contract tests for absolute ticks and relative durations.

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

#[test]
fn calendar_projects_four_equal_seasons_without_state() {
    let calendar = CalendarDefinition::new(100, 86_400, 2, 8);

    assert_eq!(
        calendar.date_at(SimulationTick::ZERO).season(),
        Season::Spring
    );
    assert_eq!(calendar.season_at(SimulationTick::new(400)), Season::Summer);
    assert_eq!(calendar.season_at(SimulationTick::new(800)), Season::Autumn);
    assert_eq!(
        calendar.season_at(SimulationTick::new(1_200)),
        Season::Winter
    );
    assert_eq!(
        calendar.date_at(SimulationTick::new(1_600)),
        CalendarDate {
            year: 2,
            month: 1,
            day: 1,
            day_tick: 0,
            season: Season::Spring,
        }
    );
}

#[test]
fn calendar_exposes_exact_physical_world_time_per_tick() {
    let calendar = CalendarDefinition::new(24_000, 86_400, 8, 12);

    assert_eq!(calendar.physical_seconds_per_day(), 86_400);
    assert_eq!(calendar.physical_tick_duration().microseconds(), 3_600_000);
    assert_eq!(
        calendar
            .physical_tick_duration()
            .span_microseconds(TickSpan::new(calendar.ticks_per_day())),
        86_400_000_000
    );
}
