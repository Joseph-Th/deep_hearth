//! Deterministic normalized risk decisions using only persisted state-owned random streams.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize};

use super::rng::RngStreamId;
use super::state::AppState;

pub const PROBABILITY_PARTS_PER_MILLION: u32 = 1_000_000;

/// Normalized probability in integer parts per million.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct ProbabilityPpm(u32);

impl ProbabilityPpm {
    pub const NEVER: Self = Self(0);
    pub const ALWAYS: Self = Self(PROBABILITY_PARTS_PER_MILLION);

    pub fn new(parts_per_million: u32) -> Result<Self, ProbabilityError> {
        if parts_per_million > PROBABILITY_PARTS_PER_MILLION {
            return Err(ProbabilityError::OutOfRange { parts_per_million });
        }
        Ok(Self(parts_per_million))
    }

    #[must_use]
    pub const fn parts_per_million(self) -> u32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for ProbabilityPpm {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbabilityError {
    OutOfRange { parts_per_million: u32 },
}

impl Display for ProbabilityError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange { parts_per_million } => write!(
                formatter,
                "probability {parts_per_million} ppm exceeds {PROBABILITY_PARTS_PER_MILLION} ppm"
            ),
        }
    }
}

impl Error for ProbabilityError {}

/// Draws one raw value from the named authoritative stream.
pub fn draw_random_u64(state: &mut AppState, stream: RngStreamId) -> u64 {
    state.random_state_mut().next_u64(stream)
}

/// Draws uniformly from `[0, bound)` using rejection sampling without modulo bias.
pub fn draw_bounded_u64(state: &mut AppState, stream: RngStreamId, bound: NonZeroU64) -> u64 {
    let bound_value = bound.get();
    let rejection_threshold = bound_value.wrapping_neg() % bound_value;
    loop {
        let value = draw_random_u64(state, stream);
        let product = u128::from(value) * u128::from(bound_value);
        let low = product as u64;
        if low >= rejection_threshold {
            return (product >> 64) as u64;
        }
    }
}

/// Resolves one normalized probability without advancing RNG for deterministic 0%/100% outcomes.
pub fn roll_probability(
    state: &mut AppState,
    stream: RngStreamId,
    probability: ProbabilityPpm,
) -> bool {
    if probability == ProbabilityPpm::NEVER {
        return false;
    }
    if probability == ProbabilityPpm::ALWAYS {
        return true;
    }
    let bound = match NonZeroU64::new(u64::from(PROBABILITY_PARTS_PER_MILLION)) {
        Some(bound) => bound,
        None => panic!("probability normalization constant must be nonzero"),
    };
    draw_bounded_u64(state, stream, bound) < u64::from(probability.parts_per_million())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::time::WorldSeed;

    #[test]
    fn probability_deserialization_rejects_values_above_one() {
        let result: Result<ProbabilityPpm, _> = serde_json::from_str("1000001");
        assert!(result.is_err());
    }

    #[test]
    fn certain_probability_does_not_consume_random_stream_state() {
        let stream = RngStreamId::new(77);
        let mut first = AppState::new(WorldSeed::new(123));
        let mut second = first.clone();

        assert!(!roll_probability(&mut first, stream, ProbabilityPpm::NEVER));
        assert!(roll_probability(&mut first, stream, ProbabilityPpm::ALWAYS));
        assert_eq!(
            draw_random_u64(&mut first, stream),
            draw_random_u64(&mut second, stream)
        );
    }

    #[test]
    fn bounded_draw_is_deterministic_for_same_state_and_stream() {
        let stream = RngStreamId::new(88);
        let bound = match NonZeroU64::new(37) {
            Some(bound) => bound,
            None => panic!("fixture bound must be nonzero"),
        };
        let mut first = AppState::new(WorldSeed::new(456));
        let mut second = first.clone();
        for _ in 0..100 {
            assert_eq!(
                draw_bounded_u64(&mut first, stream, bound),
                draw_bounded_u64(&mut second, stream, bound)
            );
        }
    }
}
