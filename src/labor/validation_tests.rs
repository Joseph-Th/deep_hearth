//! Shared trusted-load schedule projection regressions.

use super::*;

#[test]
fn active_work_schedule_projects_total_and_remaining_ticks_once() {
    assert_eq!(
        project_active_work_schedule(
            SimulationTick::new(15),
            SimulationTick::new(10),
            SimulationTick::new(25),
        ),
        Some(ActiveWorkSchedule {
            duration: TickSpan::new(15),
            remaining: TickSpan::new(10),
        })
    );
}

#[test]
fn active_work_schedule_rejects_non_active_intervals() {
    for (current, started_at, completes_at) in [(10, 11, 20), (10, 5, 10), (10, 12, 11)] {
        assert_eq!(
            project_active_work_schedule(
                SimulationTick::new(current),
                SimulationTick::new(started_at),
                SimulationTick::new(completes_at),
            ),
            None
        );
    }
}
