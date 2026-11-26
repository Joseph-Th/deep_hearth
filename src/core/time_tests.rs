//! Contract tests for absolute ticks and relative durations.

use super::*;

#[test]
fn control_clock_is_exactly_twenty_hertz_and_maps_to_slow_boundaries() {
    assert_eq!(CONTROL_TICK_DURATION.microseconds(), 50_000);
    assert_eq!(CONTROL_TICKS_PER_SIMULATION_TICK, 72);
    assert_eq!(ControlTick::new(71).simulation_tick(), SimulationTick::ZERO);
    assert!(!ControlTick::new(71).is_simulation_boundary());
    assert_eq!(
        ControlTick::new(72).simulation_tick(),
        SimulationTick::new(1)
    );
    assert!(ControlTick::new(72).is_simulation_boundary());
    assert_eq!(
        ControlTick::from_simulation_tick(SimulationTick::new(2)),
        Some(ControlTick::new(144))
    );
    assert_eq!(ControlTick::MAX_COMPLETE.checked_next(), None);
    assert_eq!(
        ControlTick::MAX_COMPLETE.simulation_tick(),
        SimulationTick::MAX_CONTROL_ALIGNED
    );
}

#[test]
fn scheduled_simulation_time_cannot_exceed_the_last_control_boundary() {
    let last = SimulationTick::MAX_CONTROL_ALIGNED.value();
    assert_eq!(
        SimulationTick::new(last - 2).checked_add_executable_span(TickSpan::new(2)),
        Some(SimulationTick::MAX_CONTROL_ALIGNED),
    );
    assert_eq!(
        SimulationTick::new(last - 2).checked_add_executable_span(TickSpan::new(3)),
        None,
        "a tick beyond the last 20 Hz boundary cannot ever be executed",
    );
    assert_eq!(
        SimulationTick::new(last).checked_add_span(TickSpan::new(1)),
        Some(SimulationTick::new(last + 1)),
        "future policy deadlines retain general checked arithmetic",
    );
}

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
fn tick_spans_subtract_without_underflow() {
    assert_eq!(
        TickSpan::new(7).checked_sub(TickSpan::new(3)),
        Some(TickSpan::new(4))
    );
    assert_eq!(
        TickSpan::new(3).checked_sub(TickSpan::new(3)),
        Some(TickSpan::ZERO)
    );
    assert_eq!(TickSpan::new(3).checked_sub(TickSpan::new(7)), None);
}

#[test]
fn absolute_ticks_subtract_into_relative_spans_without_underflow() {
    assert_eq!(
        SimulationTick::new(17).checked_duration_since(SimulationTick::new(10)),
        Some(TickSpan::new(7))
    );
    assert_eq!(
        SimulationTick::new(10).checked_duration_since(SimulationTick::new(10)),
        Some(TickSpan::ZERO)
    );
    assert_eq!(
        SimulationTick::new(10).checked_duration_since(SimulationTick::new(17)),
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
fn daylight_phase_durations_and_forecasts_match_every_tick_of_short_and_uneven_days() {
    let short = CalendarDefinition::new(3, 3, 1, 4);
    assert_eq!(
        (0..3)
            .map(|at| short.day_phase_at(SimulationTick::new(at)))
            .collect::<Vec<_>>(),
        [DayPhase::Morning, DayPhase::Afternoon, DayPhase::Evening]
    );
    assert_eq!(
        short.ticks_until_day_phase(SimulationTick::ZERO, DayPhase::Night),
        None
    );
    for day in 1..=24_u64 {
        let calendar = CalendarDefinition::new(day, day as u32, 1, 4);
        let phases = (0..day)
            .map(|at| calendar.day_phase_at(SimulationTick::new(at)))
            .collect::<Vec<_>>();
        for phase in DayPhase::ALL {
            assert_eq!(
                calendar.day_phase_duration(phase).value(),
                phases.iter().filter(|&&actual| actual == phase).count() as u64
            );
            for at in 0..day {
                let tick = SimulationTick::new(at + day);
                let expected =
                    (0..day).find(|offset| phases[((at + offset) % day) as usize] == phase);
                assert_eq!(
                    calendar
                        .ticks_until_day_phase(tick, phase)
                        .map(TickSpan::value),
                    expected
                );
                let end = calendar.ticks_until_day_phase_end(tick).value();
                assert!(end > 0 && end <= day - at);
                assert!(
                    (0..end).all(|offset| phases[(at + offset) as usize] == phases[at as usize])
                );
                if at + end < day {
                    assert_ne!(phases[(at + end) as usize], phases[at as usize]);
                }
            }
        }
    }
}

#[test]
fn calendar_projects_maximum_authoritative_tick_without_overflow() {
    let calendar = CalendarDefinition::new(1, 1, 1, 4);
    let date = calendar.date_at(SimulationTick::new(u64::MAX));

    assert_eq!(date.year(), u64::MAX / 4 + 1);
    assert_eq!(date.month(), 4);
    assert_eq!(date.day(), 1);
    assert_eq!(date.day_tick(), 0);
    assert_eq!(date.season(), Season::Winter);
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
