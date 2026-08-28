//! Continuous equipment condition and pure wear/maintenance planning without imposing one degradation curve.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use crate::core::time::TickSpan;

pub const CONDITION_PARTS_PER_MILLION: u32 = 1_000_000;

/// Enforces the normalized per-tick wear range shared by all authored equipment work.
pub(crate) const fn assert_valid_condition_wear_ppm_per_tick(wear_ppm_per_tick: u32) {
    assert!(
        wear_ppm_per_tick > 0,
        "equipment condition wear per active tick must be nonzero"
    );
    assert!(
        wear_ppm_per_tick <= CONDITION_PARTS_PER_MILLION,
        "equipment condition wear per active tick cannot exceed the normalized condition range"
    );
}

/// Normalized remaining physical condition where zero is fully degraded and one million is pristine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Condition(u32);

impl Condition {
    pub const FAILED: Self = Self(0);
    pub const PRISTINE: Self = Self(CONDITION_PARTS_PER_MILLION);

    pub fn new(parts_per_million: u32) -> Result<Self, ConditionError> {
        if parts_per_million > CONDITION_PARTS_PER_MILLION {
            return Err(ConditionError::OutOfRange { parts_per_million });
        }
        Ok(Self(parts_per_million))
    }

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionError {
    OutOfRange { parts_per_million: u32 },
}

impl Display for ConditionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange { parts_per_million } => write!(
                formatter,
                "condition {parts_per_million} ppm exceeds {CONDITION_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for ConditionError {}

/// Authored warning/critical thresholds for one maintainable equipment class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaintenanceThresholds {
    warning_below: Condition,
    critical_below: Condition,
}

impl MaintenanceThresholds {
    pub fn new(
        warning_below: Condition,
        critical_below: Condition,
    ) -> Result<Self, MaintenanceThresholdError> {
        if critical_below > warning_below {
            return Err(MaintenanceThresholdError::CriticalAboveWarning {
                warning_below,
                critical_below,
            });
        }
        Ok(Self {
            warning_below,
            critical_below,
        })
    }

    #[must_use]
    pub const fn warning_below(self) -> Condition {
        self.warning_below
    }

    #[must_use]
    pub const fn critical_below(self) -> Condition {
        self.critical_below
    }

    #[must_use]
    pub fn classify(self, condition: Condition) -> MaintenanceBand {
        if condition <= self.critical_below {
            MaintenanceBand::Critical
        } else if condition <= self.warning_below {
            MaintenanceBand::Warning
        } else {
            MaintenanceBand::Normal
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceThresholdError {
    CriticalAboveWarning {
        warning_below: Condition,
        critical_below: Condition,
    },
}

impl Display for MaintenanceThresholdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CriticalAboveWarning {
                warning_below,
                critical_below,
            } => write!(
                formatter,
                "critical condition {} ppm exceeds warning condition {} ppm",
                critical_below.parts_per_million(),
                warning_below.parts_per_million()
            ),
        }
    }
}

impl Error for MaintenanceThresholdError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceBand {
    Normal,
    Warning,
    Critical,
}

/// Calculates condition after an operation remains active for an exact authoritative tick span.
///
/// Per-tick wear is accumulated in `u128`, then clamped once at the normalized condition range so
/// long operations cannot overflow or behave differently when split into smaller arithmetic steps.
#[must_use]
pub(crate) fn calculate_condition_after_active_ticks(
    wear_ppm_per_active_tick: u32,
    before: Condition,
    duration: TickSpan,
) -> Condition {
    let total_wear = u128::from(wear_ppm_per_active_tick) * u128::from(duration.value());
    let bounded_wear = std::cmp::min(total_wear, u128::from(CONDITION_PARTS_PER_MILLION)) as u32;
    Condition(before.0.saturating_sub(bounded_wear))
}

/// Failure to schedule productive active time entirely inside an equipment instance's remaining
/// usable condition lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveConditionDurationError {
    before: Condition,
    wear_ppm_per_active_tick: u32,
    requested: TickSpan,
    maximum: TickSpan,
}

impl Display for ActiveConditionDurationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "equipment at {} ppm with {} ppm wear per active tick can provide at most {} productive ticks, not {}",
            self.before.parts_per_million(),
            self.wear_ppm_per_active_tick,
            self.maximum.value(),
            self.requested.value()
        )
    }
}

impl Error for ActiveConditionDurationError {}

/// Calculates post-operation condition while refusing active ticks that would occur after failure.
///
/// The final useful tick may consume the remaining condition and finish at [`Condition::FAILED`].
/// A further active tick is impossible because productive condition-sensitive capabilities resolve to
/// zero once the equipment has failed.
pub(crate) fn calculate_usable_condition_after_active_ticks(
    wear_ppm_per_active_tick: u32,
    before: Condition,
    duration: TickSpan,
) -> Result<Condition, ActiveConditionDurationError> {
    assert_valid_condition_wear_ppm_per_tick(wear_ppm_per_active_tick);
    let remaining = u64::from(before.parts_per_million());
    let wear = u64::from(wear_ppm_per_active_tick);
    let maximum_ticks = remaining.div_ceil(wear);
    let maximum = TickSpan::new(maximum_ticks);
    if duration > maximum {
        return Err(ActiveConditionDurationError {
            before,
            wear_ppm_per_active_tick,
            requested: duration,
            maximum,
        });
    }
    Ok(calculate_condition_after_active_ticks(
        wear_ppm_per_active_tick,
        before,
        duration,
    ))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
