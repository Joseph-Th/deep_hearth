//! Shared active-time and equipment-wear semantics for powered ore preparation.

use crate::core::time::TickSpan;
use crate::maintenance::{
    ActiveConditionDurationError, Condition, calculate_usable_condition_after_active_ticks,
};

/// The two independent physical durations that can limit a powered ore-preparation operation.
///
/// The process remains active until both throughput and energy delivery requirements have been met.
/// Equipment wear therefore applies to that complete active duration, not only to the ideal
/// throughput duration. Keeping this relation in one type prevents resolver and persistence replay
/// from drifting into different wear semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OreProcessActiveTiming {
    throughput: TickSpan,
    energy_delivery: TickSpan,
}

impl OreProcessActiveTiming {
    #[must_use]
    pub(super) const fn new(throughput: TickSpan, energy_delivery: TickSpan) -> Self {
        Self {
            throughput,
            energy_delivery,
        }
    }

    #[must_use]
    pub(super) fn duration(self) -> TickSpan {
        std::cmp::max(self.throughput, self.energy_delivery)
    }

    pub(super) fn condition_after(
        self,
        wear_ppm_per_active_tick: u32,
        condition_before: Condition,
    ) -> Result<Condition, ActiveConditionDurationError> {
        calculate_usable_condition_after_active_ticks(
            wear_ppm_per_active_tick,
            condition_before,
            self.duration(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_limited_time_is_active_time_for_condition_wear() {
        let timing = OreProcessActiveTiming::new(TickSpan::new(1), TickSpan::new(6));
        let after = timing
            .condition_after(1_000, Condition::PRISTINE)
            .unwrap_or_else(|error| panic!("power-limited wear calculation failed: {error}"));
        let expected = Condition::new(994_000)
            .unwrap_or_else(|error| panic!("expected condition fixture failed: {error}"));

        assert_eq!(timing.duration(), TickSpan::new(6));
        assert_eq!(after, expected);
    }
}
