//! Continuous equipment condition and pure wear/repair planning without imposing one degradation curve.

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

/// Pure condition transition plan; an owning runtime system decides when and where to persist it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConditionPlan {
    before: Condition,
    after: Condition,
}

impl ConditionPlan {
    #[must_use]
    pub const fn before(self) -> Condition {
        self.before
    }

    #[must_use]
    pub const fn after(self) -> Condition {
        self.after
    }
}

#[must_use]
pub fn decide_wear(current: Condition, wear_ppm: u32) -> ConditionPlan {
    let after = Condition(current.0.saturating_sub(wear_ppm));
    ConditionPlan {
        before: current,
        after,
    }
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
    decide_wear(before, bounded_wear).after()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn condition(value: u32) -> Condition {
        match Condition::new(value) {
            Ok(condition) => condition,
            Err(error) => panic!("condition fixture failed: {error}"),
        }
    }

    #[test]
    fn wear_clamps_at_failed_bound_without_destroying_records() {
        assert_eq!(decide_wear(condition(10), 20).after(), Condition::FAILED);
    }

    #[test]
    fn active_tick_wear_clamps_without_duration_overflow() {
        assert_eq!(
            calculate_condition_after_active_ticks(
                CONDITION_PARTS_PER_MILLION,
                Condition::PRISTINE,
                TickSpan::new(u64::MAX),
            ),
            Condition::FAILED
        );
    }

    #[test]
    fn warning_and_critical_bands_are_authored_independently_of_wear_curve() {
        let thresholds = match MaintenanceThresholds::new(condition(600_000), condition(250_000)) {
            Ok(thresholds) => thresholds,
            Err(error) => panic!("threshold fixture failed: {error}"),
        };
        assert_eq!(
            thresholds.classify(condition(800_000)),
            MaintenanceBand::Normal
        );
        assert_eq!(
            thresholds.classify(condition(500_000)),
            MaintenanceBand::Warning
        );
        assert_eq!(
            thresholds.classify(condition(200_000)),
            MaintenanceBand::Critical
        );
    }

    #[test]
    fn condition_deserialization_rejects_out_of_range_values() {
        let result: Result<Condition, _> = serde_json::from_str("1000001");
        assert!(result.is_err());
    }

    #[test]
    fn condition_wear_rate_requires_normalized_nonzero_value() {
        assert_valid_condition_wear_ppm_per_tick(1);
        assert_valid_condition_wear_ppm_per_tick(CONDITION_PARTS_PER_MILLION);
        assert!(std::panic::catch_unwind(|| assert_valid_condition_wear_ppm_per_tick(0)).is_err());
        assert!(
            std::panic::catch_unwind(|| {
                assert_valid_condition_wear_ppm_per_tick(CONDITION_PARTS_PER_MILLION + 1)
            })
            .is_err()
        );
    }
}
