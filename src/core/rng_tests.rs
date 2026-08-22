//! Tests for the sibling rng module; isolated so test-only edits do not invalidate production builds.

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
