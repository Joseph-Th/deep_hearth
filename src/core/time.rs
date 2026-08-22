//! Strong types for persistent world seed and authoritative simulation time.

use serde::{Deserialize, Serialize};

const MICROSECONDS_PER_SECOND: u64 = 1_000_000;

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

/// Persistent seed from which deterministic world generation and initial randomness are derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WorldSeed(u64);

impl WorldSeed {
    /// Creates a world seed from its stable integer representation.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable integer representation.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
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

#[cfg(test)]
mod calendar_tests {
    use super::*;

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
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;

/// Monotonic authoritative simulation tick.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct SimulationTick(u64);

impl SimulationTick {
    /// First simulation tick before any advancement has occurred.
    pub const ZERO: Self = Self(0);

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
}
