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
