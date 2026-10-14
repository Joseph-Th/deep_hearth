//! Strong types for authoritative simulation time.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

const MICROSECONDS_PER_SECOND: u64 = 1_000_000;

/// High-frequency direct-control cadence: 20 Hz, with one slow simulation quantum every 72 steps.
pub const CONTROL_TICKS_PER_SIMULATION_TICK: u64 = 72;
pub const CONTROL_TICK_DURATION: PhysicalTickDuration =
    PhysicalTickDuration::from_microseconds(50_000);

/// Exact physical world-time represented by one authoritative simulation tick.
///
/// This is intentionally distinct from any future wall-clock update cadence. Rate-authored
/// physics such as watts, mass/second, and volume/second integrate against this duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysicalTickDuration(u64);

impl PhysicalTickDuration {
    #[must_use]
    pub const fn from_microseconds(microseconds: u64) -> Self {
        assert!(
            microseconds > 0,
            "physical tick duration must be at least one microsecond"
        );
        Self(microseconds)
    }

    #[must_use]
    pub const fn microseconds(self) -> u64 {
        self.0
    }

    #[must_use]
    pub(crate) fn span_microseconds(self, span: TickSpan) -> u128 {
        u128::from(self.0) * u128::from(span.value())
    }
}

/// High-frequency authoritative control/kinematic tick used by direct world interaction.
///
/// Slow simulation state advances only at deterministic boundaries of
/// [`CONTROL_TICKS_PER_SIMULATION_TICK`] control ticks. This preserves exact coarse physical
/// accounting while providing a 20 Hz interaction clock.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct ControlTick(u64);

impl ControlTick {
    pub const ZERO: Self = Self(0);
    /// Last control tick from which the slow simulation and control clocks remain mutually representable.
    pub const MAX_COMPLETE: Self =
        Self((u64::MAX / CONTROL_TICKS_PER_SIMULATION_TICK) * CONTROL_TICKS_PER_SIMULATION_TICK);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn simulation_tick(self) -> SimulationTick {
        SimulationTick::new(self.0 / CONTROL_TICKS_PER_SIMULATION_TICK)
    }

    #[must_use]
    pub const fn is_simulation_boundary(self) -> bool {
        self.0.is_multiple_of(CONTROL_TICKS_PER_SIMULATION_TICK)
    }

    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        if self.0 >= Self::MAX_COMPLETE.0 {
            return None;
        }
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn from_simulation_tick(tick: SimulationTick) -> Option<Self> {
        match tick.0.checked_mul(CONTROL_TICKS_PER_SIMULATION_TICK) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Slow-scheduled work may begin only where the 20 Hz control clock and slow simulation coincide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimulationBoundaryError {
    control_tick: ControlTick,
}

impl SimulationBoundaryError {
    #[must_use]
    pub const fn control_tick(self) -> ControlTick {
        self.control_tick
    }
}

impl Display for SimulationBoundaryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "slow-scheduled work cannot begin at control tick {}; wait for the next 3.6-second simulation boundary",
            self.control_tick.value()
        )
    }
}

impl Error for SimulationBoundaryError {}

/// Requires an exact slow-simulation boundary before admitting work expressed in [`TickSpan`].
pub const fn require_simulation_boundary(
    control_tick: ControlTick,
) -> Result<(), SimulationBoundaryError> {
    if control_tick.is_simulation_boundary() {
        Ok(())
    } else {
        Err(SimulationBoundaryError { control_tick })
    }
}

/// Four broad agricultural seasons projected from the authored world calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

/// Immutable world-calendar definition used to interpret authoritative ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDefinition {
    ticks_per_day: u64,
    physical_seconds_per_day: u32,
    physical_tick_duration: PhysicalTickDuration,
    days_per_month: u16,
    months_per_year: u16,
}

impl CalendarDefinition {
    #[must_use]
    pub(crate) fn new(
        ticks_per_day: u64,
        physical_seconds_per_day: u32,
        days_per_month: u16,
        months_per_year: u16,
    ) -> Self {
        assert!(ticks_per_day > 0, "calendar ticks per day must be nonzero");
        assert!(
            physical_seconds_per_day > 0,
            "calendar physical seconds per day must be nonzero"
        );
        let day_microseconds = u64::from(physical_seconds_per_day)
            .checked_mul(MICROSECONDS_PER_SECOND)
            .unwrap_or_else(|| panic!("calendar physical day duration exceeds supported range"));
        assert!(
            day_microseconds.is_multiple_of(ticks_per_day),
            "calendar physical day duration must divide exactly into authored ticks"
        );
        let physical_tick_duration =
            PhysicalTickDuration::from_microseconds(day_microseconds / ticks_per_day);
        assert!(
            days_per_month > 0,
            "calendar days per month must be nonzero"
        );
        assert!(
            months_per_year >= 4,
            "calendar must contain at least four months"
        );
        assert!(
            months_per_year.is_multiple_of(4),
            "calendar months per year must divide evenly into four seasons"
        );
        Self {
            ticks_per_day,
            physical_seconds_per_day,
            physical_tick_duration,
            days_per_month,
            months_per_year,
        }
    }

    #[must_use]
    pub const fn ticks_per_day(self) -> u64 {
        self.ticks_per_day
    }

    /// Returns the physical duration of one calendar day in world seconds.
    #[must_use]
    pub const fn physical_seconds_per_day(self) -> u32 {
        self.physical_seconds_per_day
    }

    /// Returns the exact physical world-time represented by one simulation tick.
    #[must_use]
    pub const fn physical_tick_duration(self) -> PhysicalTickDuration {
        self.physical_tick_duration
    }

    #[must_use]
    pub const fn days_per_month(self) -> u16 {
        self.days_per_month
    }

    #[must_use]
    pub const fn months_per_year(self) -> u16 {
        self.months_per_year
    }

    /// Projects one authoritative tick into a stable calendar date.
    #[must_use]
    pub fn date_at(self, tick: SimulationTick) -> CalendarDate {
        let absolute_day = tick.value() / self.ticks_per_day;
        let day_tick = tick.value() % self.ticks_per_day;
        let days_per_year = u64::from(self.days_per_month) * u64::from(self.months_per_year);
        let year = absolute_day / days_per_year + 1;
        let day_in_year = absolute_day % days_per_year;
        let month_index = day_in_year / u64::from(self.days_per_month);
        let day_index = day_in_year % u64::from(self.days_per_month);
        let months_per_season = u64::from(self.months_per_year / 4);
        let season = match month_index / months_per_season {
            0 => Season::Spring,
            1 => Season::Summer,
            2 => Season::Autumn,
            3 => Season::Winter,
            _ => unreachable!("calendar month index must map into exactly four seasons"),
        };
        let month = match u16::try_from(month_index + 1) {
            Ok(month) => month,
            Err(_) => unreachable!("calendar month index must fit authored u16 range"),
        };
        let day = match u16::try_from(day_index + 1) {
            Ok(day) => day,
            Err(_) => unreachable!("calendar day index must fit authored u16 range"),
        };
        CalendarDate {
            year,
            month,
            day,
            day_tick,
            season,
        }
    }

    #[must_use]
    pub fn season_at(self, tick: SimulationTick) -> Season {
        self.date_at(tick).season
    }
}

/// Read-only calendar projection for one authoritative tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDate {
    year: u64,
    month: u16,
    day: u16,
    day_tick: u64,
    season: Season,
}

impl CalendarDate {
    #[must_use]
    pub const fn year(self) -> u64 {
        self.year
    }

    #[must_use]
    pub const fn month(self) -> u16 {
        self.month
    }

    #[must_use]
    pub const fn day(self) -> u16 {
        self.day
    }

    #[must_use]
    pub const fn day_tick(self) -> u64 {
        self.day_tick
    }

    #[must_use]
    pub const fn season(self) -> Season {
        self.season
    }
}

/// Relative duration measured in authoritative simulation ticks.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct TickSpan(u64);

impl TickSpan {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Subtracts a relative duration without underflowing the authoritative tick-span domain.
    #[must_use]
    pub const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Monotonic authoritative simulation tick.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct SimulationTick(u64);

impl SimulationTick {
    /// First simulation tick before any advancement has occurred.
    pub const ZERO: Self = Self(0);
    /// Largest slow-simulation tick that still has an exact high-frequency control boundary.
    pub const MAX_CONTROL_ALIGNED: Self = Self(u64::MAX / CONTROL_TICKS_PER_SIMULATION_TICK);

    /// Creates a tick from its stable integer representation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable integer representation.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Adds a relative duration without allowing authoritative time to wrap.
    #[must_use]
    pub const fn checked_add_span(self, span: TickSpan) -> Option<Self> {
        match self.0.checked_add(span.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Returns the elapsed duration since an earlier authoritative tick.
    ///
    /// Returns `None` when `earlier` is after `self`, keeping monotonic tick subtraction inside the
    /// time owner instead of exposing unchecked integer arithmetic at call sites.
    #[must_use]
    pub const fn checked_duration_since(self, earlier: Self) -> Option<TickSpan> {
        match self.0.checked_sub(earlier.0) {
            Some(value) => Some(TickSpan::new(value)),
            None => None,
        }
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
