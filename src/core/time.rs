//! Strong types for persistent world seed and authoritative simulation time.

use serde::{Deserialize, Serialize};

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
    days_per_month: u16,
    months_per_year: u16,
}

impl CalendarDefinition {
    #[must_use]
    pub(crate) fn new(ticks_per_day: u64, days_per_month: u16, months_per_year: u16) -> Self {
        assert!(ticks_per_day > 0, "calendar ticks per day must be nonzero");
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
            days_per_month,
            months_per_year,
        }
    }

    #[must_use]
    pub const fn ticks_per_day(self) -> u64 {
        self.ticks_per_day
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
        let calendar = CalendarDefinition::new(100, 2, 8);

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
mod tests {
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
}

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
