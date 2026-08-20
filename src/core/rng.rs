//! Versioned deterministic pseudo-random state used by authoritative simulation systems.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use super::time::WorldSeed;

/// Stable identifier for one independently advancing authoritative random stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RngStreamId(u32);

impl RngStreamId {
    /// Core stream retained for domain-neutral simulation decisions.
    pub const CORE: Self = Self(1);

    #[must_use]
    pub const fn new(value: u32) -> Self {
        assert!(value != 0, "RNG stream id must be nonzero");
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Persisted algorithm used to derive a new stream's initial seed from world seed and stream ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RngStreamDerivation {
    /// SplitMix64-based domain separation pinned as stream derivation version 1.
    SplitMix64V1,
}

/// Persisted algorithm identity; variants are never silently reinterpreted after release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RngAlgorithm {
    /// xoshiro256** with SplitMix64 seed expansion, pinned as Deep Hearth algorithm version 1.
    Xoshiro256StarStarV1,
}

/// Persisted owner for independent authoritative PRNG streams.
///
/// Streams are created deterministically from the world seed and typed stream ID. Once created,
/// their full state is serialized. Adding a new subsystem stream therefore cannot shift the
/// sequence of any existing stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomState {
    root_seed: WorldSeed,
    derivation: RngStreamDerivation,
    #[serde(deserialize_with = "crate::core::serialization::deserialize_btree_map_no_duplicates")]
    streams: BTreeMap<RngStreamId, DeterministicRng>,
}

impl RandomState {
    #[must_use]
    pub fn new(world_seed: WorldSeed) -> Self {
        let derivation = RngStreamDerivation::SplitMix64V1;
        let mut streams = BTreeMap::new();
        streams.insert(
            RngStreamId::CORE,
            DeterministicRng::from_seed(derive_stream_seed(
                derivation,
                world_seed,
                RngStreamId::CORE,
            )),
        );
        Self {
            root_seed: world_seed,
            derivation,
            streams,
        }
    }

    #[must_use]
    pub const fn root_seed(&self) -> WorldSeed {
        self.root_seed
    }

    #[must_use]
    pub const fn derivation(&self) -> RngStreamDerivation {
        self.derivation
    }

    /// Returns the PRNG algorithm for one stream when that stream has been initialized.
    #[must_use]
    pub fn stream_algorithm(&self, stream: RngStreamId) -> Option<RngAlgorithm> {
        self.streams.get(&stream).map(DeterministicRng::algorithm)
    }

    pub(crate) fn has_valid_core_stream(&self) -> bool {
        self.streams
            .get(&RngStreamId::CORE)
            .is_some_and(DeterministicRng::is_valid)
    }

    /// Advances one independent state-owned stream, creating it deterministically on first use.
    pub fn next_u64(&mut self, stream: RngStreamId) -> u64 {
        let derivation = self.derivation;
        let world_seed = self.root_seed;
        self.streams
            .entry(stream)
            .or_insert_with(|| {
                DeterministicRng::from_seed(derive_stream_seed(derivation, world_seed, stream))
            })
            .next_u64()
    }

    pub(crate) fn validate(&self) -> Result<(), RandomStateValidationError> {
        if !self.streams.contains_key(&RngStreamId::CORE) {
            return Err(RandomStateValidationError::MissingCoreStream);
        }
        for (stream, rng) in &self.streams {
            if stream.value() == 0 {
                return Err(RandomStateValidationError::ZeroStreamId);
            }
            if !rng.is_valid() {
                return Err(RandomStateValidationError::InvalidStreamState { stream: *stream });
            }
        }
        Ok(())
    }
}

/// Persistent random-state corruption detected during load validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomStateValidationError {
    MissingCoreStream,
    ZeroStreamId,
    InvalidStreamState { stream: RngStreamId },
}

impl Display for RandomStateValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingCoreStream => {
                formatter.write_str("random state is missing the core stream")
            }
            Self::ZeroStreamId => formatter.write_str("random state contains stream id zero"),
            Self::InvalidStreamState { stream } => write!(
                formatter,
                "random stream {} contains invalid PRNG state",
                stream.value()
            ),
        }
    }
}

impl Error for RandomStateValidationError {}

/// Serializable deterministic PRNG state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterministicRng {
    algorithm: RngAlgorithm,
    words: [u64; 4],
}

fn derive_stream_seed(
    derivation: RngStreamDerivation,
    world_seed: WorldSeed,
    stream: RngStreamId,
) -> u64 {
    match derivation {
        RngStreamDerivation::SplitMix64V1 => {
            let mut state = world_seed.value()
                ^ 0xD1B5_4A32_D192_ED03
                ^ (u64::from(stream.value()) << 32)
                ^ u64::from(stream.value());
            splitmix64(&mut state)
        }
    }
}

impl DeterministicRng {
    /// Initializes the versioned PRNG from a stable 64-bit seed.
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut seed_state = seed;
        let words = [
            splitmix64(&mut seed_state),
            splitmix64(&mut seed_state),
            splitmix64(&mut seed_state),
            splitmix64(&mut seed_state),
        ];

        Self {
            algorithm: RngAlgorithm::Xoshiro256StarStarV1,
            words,
        }
    }

    /// Returns the persisted algorithm identity.
    #[must_use]
    pub const fn algorithm(&self) -> RngAlgorithm {
        self.algorithm
    }

    /// Reports whether the internal state is valid for its algorithm.
    #[must_use]
    pub(crate) fn is_valid(&self) -> bool {
        match self.algorithm {
            RngAlgorithm::Xoshiro256StarStarV1 => self.words.iter().any(|word| *word != 0),
        }
    }

    /// Advances this explicitly owned generator and returns the next deterministic 64-bit value.
    ///
    /// Authoritative gameplay systems must draw from the RNG stored in `AppState`; this method is
    /// also public so isolated deterministic tools can own and inject an RNG explicitly.
    pub fn next_u64(&mut self) -> u64 {
        match self.algorithm {
            RngAlgorithm::Xoshiro256StarStarV1 => next_xoshiro256_star_star(&mut self.words),
        }
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn next_xoshiro256_star_star(words: &mut [u64; 4]) -> u64 {
    let result = words[1].rotate_left(7).wrapping_mul(9);
    let shifted = words[1] << 17;

    words[2] ^= words[0];
    words[3] ^= words[1];
    words[1] ^= words[2];
    words[0] ^= words[3];
    words[2] ^= shifted;
    words[3] = words[3].rotate_left(45);

    result
}

#[cfg(all(
    test,
    any(not(feature = "test-unit-sharded"), feature = "test-unit-foundation")
))]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_sequence() {
        let mut first = DeterministicRng::from_seed(0xD33F_4EA7_7A11_0001);
        let mut second = DeterministicRng::from_seed(0xD33F_4EA7_7A11_0001);

        for _ in 0..64 {
            assert_eq!(first.next_u64(), second.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut first = DeterministicRng::from_seed(1);
        let mut second = DeterministicRng::from_seed(2);

        assert_ne!(first.next_u64(), second.next_u64());
    }

    #[test]
    fn zero_seed_still_creates_valid_algorithm_state() {
        let rng = DeterministicRng::from_seed(0);

        assert!(rng.is_valid());
    }

    #[test]
    fn independent_streams_do_not_shift_each_other() {
        let seed = WorldSeed::new(0xA11C_E5E5_1234_5678);
        let genetics = RngStreamId::new(101);
        let weather = RngStreamId::new(202);
        let mut interleaved = RandomState::new(seed);
        let mut isolated = RandomState::new(seed);

        let first = interleaved.next_u64(genetics);
        for _ in 0..64 {
            let _ = interleaved.next_u64(weather);
        }
        let second = interleaved.next_u64(genetics);

        assert_eq!(first, isolated.next_u64(genetics));
        assert_eq!(second, isolated.next_u64(genetics));
    }

    #[test]
    fn stream_creation_order_does_not_change_stream_sequences() {
        let seed = WorldSeed::new(77);
        let first_stream = RngStreamId::new(11);
        let second_stream = RngStreamId::new(12);
        let mut first_order = RandomState::new(seed);
        let mut second_order = RandomState::new(seed);

        let first_a = first_order.next_u64(first_stream);
        let second_a = first_order.next_u64(second_stream);
        let second_b = second_order.next_u64(second_stream);
        let first_b = second_order.next_u64(first_stream);

        assert_eq!(first_a, first_b);
        assert_eq!(second_a, second_b);
    }

    #[test]
    fn serialized_stream_state_preserves_independent_continuation() {
        let seed = WorldSeed::new(0x51A7_E5E5_0A11_0001);
        let ecology = RngStreamId::new(301);
        let weather = RngStreamId::new(302);
        let mut state = RandomState::new(seed);
        for _ in 0..17 {
            let _ = state.next_u64(ecology);
        }
        for _ in 0..31 {
            let _ = state.next_u64(weather);
        }

        let encoded = match serde_json::to_vec(&state) {
            Ok(encoded) => encoded,
            Err(error) => panic!("random-state serialization failed: {error}"),
        };
        let mut loaded: RandomState = match serde_json::from_slice(&encoded) {
            Ok(loaded) => loaded,
            Err(error) => panic!("random-state deserialization failed: {error}"),
        };

        assert_eq!(loaded, state);
        assert_eq!(loaded.next_u64(ecology), state.next_u64(ecology));
        assert_eq!(loaded.next_u64(weather), state.next_u64(weather));
        assert_eq!(loaded.validate(), Ok(()));
    }
}
