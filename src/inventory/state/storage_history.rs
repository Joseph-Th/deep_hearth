//! Persistent storage-exposure value semantics for material lots and in-flight matter.

use serde::{Deserialize, Serialize};

use crate::core::time::{SimulationTick, TickSpan};

pub(crate) const STORAGE_AGE_PARTS_PER_TICK: u128 = 1_000_000;

/// Ambient-equivalent storage age retained across stockpile moves.
///
/// `ambient_age_parts` records exposure accumulated before `last_transition_at`; the current
/// stockpile's preservation multiplier determines the rate after that tick. One ambient tick equals
/// `STORAGE_AGE_PARTS_PER_TICK` parts. This keeps preservation history independent from any one food
/// definition while preventing later movement into better storage from retroactively improving prior
/// exposure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialStorageHistory {
    ambient_age_parts: u128,
    last_transition_at: SimulationTick,
}

impl MaterialStorageHistory {
    #[must_use]
    pub(crate) const fn new(at: SimulationTick) -> Self {
        Self {
            ambient_age_parts: 0,
            last_transition_at: at,
        }
    }

    #[must_use]
    pub(crate) const fn last_transition_at(self) -> SimulationTick {
        self.last_transition_at
    }

    pub(crate) fn project(
        self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
    ) -> Option<u128> {
        let elapsed = at.value().checked_sub(self.last_transition_at.value())?;
        let numerator =
            u128::from(elapsed) * STORAGE_AGE_PARTS_PER_TICK * STORAGE_AGE_PARTS_PER_TICK;
        let increment = numerator.div_ceil(u128::from(preservation_multiplier_ppm));
        self.ambient_age_parts.checked_add(increment)
    }

    pub(crate) fn rebase(
        self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
    ) -> Option<Self> {
        Some(Self {
            ambient_age_parts: self.project(at, preservation_multiplier_ppm)?,
            last_transition_at: at,
        })
    }

    /// Moves this history across a preservation-rate boundary without introducing transaction-only
    /// rounding. If the effective rate is unchanged, the existing representation already describes
    /// the same physical exposure and must remain intact. A real rate change checkpoints exposure at
    /// `at` under the source rate before the destination rate begins.
    pub(crate) fn transition_preservation(
        self,
        at: SimulationTick,
        source_preservation_multiplier_ppm: u32,
        destination_preservation_multiplier_ppm: u32,
    ) -> Option<Self> {
        if source_preservation_multiplier_ppm == destination_preservation_multiplier_ppm {
            Some(self)
        } else {
            self.rebase(at, source_preservation_multiplier_ppm)
        }
    }

    /// Returns whether two histories have identical current and future projections while they stay
    /// under one preservation multiplier.
    ///
    /// Equal projected age at `at` is not sufficient when the per-tick rational increment requires
    /// rounding. Two histories anchored at different phases of that rational sequence can match now
    /// and diverge on a later tick. The reduced denominator is the projection sequence's period;
    /// equal age plus equal phase makes the two histories future-equivalent without rebasing either.
    pub(crate) fn is_projection_equivalent(
        self,
        other: Self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
    ) -> Option<bool> {
        let self_age = self.project(at, preservation_multiplier_ppm)?;
        let other_age = other.project(at, preservation_multiplier_ppm)?;
        if self_age != other_age {
            return Some(false);
        }

        let preservation = u128::from(preservation_multiplier_ppm);
        let age_numerator_per_tick = STORAGE_AGE_PARTS_PER_TICK * STORAGE_AGE_PARTS_PER_TICK;
        let period = preservation / greatest_common_divisor(preservation, age_numerator_per_tick);
        if period == 1 {
            return Some(true);
        }
        let self_elapsed = at.value().checked_sub(self.last_transition_at.value())?;
        let other_elapsed = at.value().checked_sub(other.last_transition_at.value())?;
        Some(u128::from(self_elapsed) % period == u128::from(other_elapsed) % period)
    }

    /// Returns the first number of future ticks at which projected ambient-equivalent exposure
    /// reaches `target_age_parts` under one unchanged preservation multiplier.
    ///
    /// This solves against the history's original projection phase rather than treating the
    /// currently rounded age as a new anchor. That distinction matters when the rational per-tick
    /// exposure does not divide evenly into whole storage-age parts.
    pub(crate) fn ticks_until_projected_age(
        self,
        at: SimulationTick,
        preservation_multiplier_ppm: u32,
        target_age_parts: u128,
    ) -> Option<TickSpan> {
        let current_age = self.project(at, preservation_multiplier_ppm)?;
        if current_age >= target_age_parts {
            return Some(TickSpan::ZERO);
        }
        debug_assert!(target_age_parts > self.ambient_age_parts);
        let required_increment = target_age_parts.checked_sub(self.ambient_age_parts)?;
        let preservation = u128::from(preservation_multiplier_ppm);
        let age_numerator_per_tick = STORAGE_AGE_PARTS_PER_TICK * STORAGE_AGE_PARTS_PER_TICK;
        let threshold_numerator = required_increment
            .checked_sub(1)?
            .checked_mul(preservation)?;
        let threshold_elapsed = threshold_numerator / age_numerator_per_tick + 1;
        let elapsed = u128::from(at.value().checked_sub(self.last_transition_at.value())?);
        let remaining = threshold_elapsed.checked_sub(elapsed)?;
        Some(TickSpan::new(u64::try_from(remaining).ok()?))
    }

    #[must_use]
    pub(crate) const fn with_ambient_age_parts(
        ambient_age_parts: u128,
        at: SimulationTick,
    ) -> Self {
        Self {
            ambient_age_parts,
            last_transition_at: at,
        }
    }
}

fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
#[path = "storage_history_tests.rs"]
mod tests;
