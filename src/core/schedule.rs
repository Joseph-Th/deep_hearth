//! Deterministic periodic scheduling primitives for slow simulation phases without callback or queue ownership.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize};

use super::time::SimulationTick;

/// Tick-derived periodic cadence with one canonical phase inside the interval.
///
/// This type is intentionally pure. It does not own callbacks, queued work, or mutable scheduling
/// state. Slow systems may use it to derive whether a phase is due from the authoritative clock.
/// Dynamic one-off work that affects continuation, such as production completion, remains an
/// explicit persisted record/index instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct PeriodicSchedule {
    interval_ticks: NonZeroU64,
    phase_tick: u64,
}

impl PeriodicSchedule {
    /// Builds a cadence whose phase is strictly inside `[0, interval)`.
    pub fn new(interval_ticks: NonZeroU64, phase_tick: u64) -> Result<Self, ScheduleError> {
        if phase_tick >= interval_ticks.get() {
            return Err(ScheduleError::PhaseOutsideInterval {
                interval_ticks,
                phase_tick,
            });
        }
        Ok(Self {
            interval_ticks,
            phase_tick,
        })
    }

    #[must_use]
    pub const fn interval_ticks(self) -> NonZeroU64 {
        self.interval_ticks
    }

    #[must_use]
    pub const fn phase_tick(self) -> u64 {
        self.phase_tick
    }

    /// Returns whether this cadence is due on the exact authoritative tick.
    #[must_use]
    pub fn is_due(self, tick: SimulationTick) -> bool {
        tick.value() % self.interval_ticks.get() == self.phase_tick
    }

    /// Finds the first due tick at or after `tick` without wrapping the authoritative clock.
    pub fn next_due_at_or_after(
        self,
        tick: SimulationTick,
    ) -> Result<SimulationTick, ScheduleAdvanceError> {
        let interval = self.interval_ticks.get();
        let current_phase = tick.value() % interval;
        let delta = if current_phase <= self.phase_tick {
            self.phase_tick - current_phase
        } else {
            interval - (current_phase - self.phase_tick)
        };
        let Some(value) = tick.value().checked_add(delta) else {
            return Err(ScheduleAdvanceError::TickOverflow { from: tick });
        };
        Ok(SimulationTick::new(value))
    }

    /// Finds the first due tick strictly after `tick`.
    pub fn next_due_after(
        self,
        tick: SimulationTick,
    ) -> Result<SimulationTick, ScheduleAdvanceError> {
        let Some(next_value) = tick.value().checked_add(1) else {
            return Err(ScheduleAdvanceError::TickOverflow { from: tick });
        };
        self.next_due_at_or_after(SimulationTick::new(next_value))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeriodicScheduleRepresentation {
    interval_ticks: NonZeroU64,
    phase_tick: u64,
}

impl<'de> Deserialize<'de> for PeriodicSchedule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let representation = PeriodicScheduleRepresentation::deserialize(deserializer)?;
        Self::new(representation.interval_ticks, representation.phase_tick)
            .map_err(serde::de::Error::custom)
    }
}

/// Invalid static periodic cadence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    PhaseOutsideInterval {
        interval_ticks: NonZeroU64,
        phase_tick: u64,
    },
}

impl Display for ScheduleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PhaseOutsideInterval {
                interval_ticks,
                phase_tick,
            } => write!(
                formatter,
                "periodic schedule phase {phase_tick} is outside interval {}",
                interval_ticks.get()
            ),
        }
    }
}

impl Error for ScheduleError {}

/// Failure to derive a future due tick without overflowing authoritative time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleAdvanceError {
    TickOverflow { from: SimulationTick },
}

impl Display for ScheduleAdvanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TickOverflow { from } => write!(
                formatter,
                "periodic schedule cannot advance past simulation tick {}",
                from.value()
            ),
        }
    }
}

impl Error for ScheduleAdvanceError {}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-foundation")
))]
mod tests {
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
}
