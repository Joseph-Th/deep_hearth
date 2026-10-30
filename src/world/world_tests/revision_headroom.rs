//! Trusted-load continuation headroom for pending world mutations.

use super::*;

#[test]
fn trusted_load_rejects_world_revision_exhausted_by_pending_excavation() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let before_work = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let _started = validate_start_player_block_excavation(
        &registries,
        &state,
        BlockExcavationRequest::new(
            positive_x_view(),
            MINING_METHOD_HAND_PICK,
            pick,
            Mass::from_milligrams(200_000),
        ),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(state.checked_future_world_revision_demand(), Some(1));
    let original = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();

    for (revision, valid) in [(u64::MAX - 1, true), (u64::MAX, false)] {
        let mut forged = original.clone();
        forged["state"]["systems"]["world"]["revision"] = serde_json::json!(revision);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        let loaded = decoded.into_state(&registries);
        if valid {
            assert!(loaded.is_ok(), "world revision {revision}: {loaded:?}");
        } else {
            assert_eq!(
                loaded,
                Err(LoadError::InvalidState(
                    StateValidationError::FutureWorldRevisionCapacityExhausted {
                        revision,
                        required: 1,
                    }
                ))
            );
        }
    }

    // The same exhausted cursor is loadable when there is no committed future world work.
    let mut idle = before_work;
    idle["state"]["systems"]["world"]["revision"] = serde_json::json!(u64::MAX);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(idle).unwrap();
    assert!(decoded.into_state(&registries).is_ok());
}
