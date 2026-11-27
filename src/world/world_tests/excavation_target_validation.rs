//! Trusted active excavation must remain bound to the real pointed target.

use super::*;

#[test]
fn trusted_load_rejects_retargeted_active_block_excavation() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    // Equivalent stone cells preserve every material, physics and schedule
    // field of the admitted batch. One is behind the first hit; the other
    // is beyond interaction reach altogether.
    let hidden = VoxelCoord::new(target.x() + 1, target.y(), target.z());
    let remote = VoxelCoord::new(target.x() + 9, target.y(), target.z());
    for cell in [hidden, remote] {
        set_generated_block_for_test(registries.world(), &mut state, cell, BLOCK_STATE_STONE);
    }
    let start = validate_start_player_block_excavation(
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
    assert_eq!(start.target(), target);
    let saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_value::<LoadedSaveEnvelope>(saved.clone())
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    for cell in [hidden, remote] {
        assert_eq!(
            state.world().represented_mass_at(registries.world(), cell),
            state
                .world()
                .represented_mass_at(registries.world(), target)
        );
        let mut forged = saved.clone();
        forged["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"]["target"] =
            serde_json::to_value(cell).unwrap();
        let decoded = serde_json::from_value::<LoadedSaveEnvelope>(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries).err(),
            Some(LoadError::InvalidState(StateValidationError::PlayerWork(
                PlayerWorkValidationError::BlockExcavationTargetRayMismatch
            ))),
            "active excavation cannot be retargeted to {cell:?}"
        );
    }
    for (aim, expected) in [
        (
            [0, 0, 0],
            PlayerWorkValidationError::BlockExcavationAimInvalid,
        ),
        (
            [-1_000_000, 0, 0],
            PlayerWorkValidationError::BlockExcavationTargetRayMismatch,
        ),
    ] {
        let mut forged = saved.clone();
        forged["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"]["aim"] =
            serde_json::json!(aim);
        let decoded = serde_json::from_value::<LoadedSaveEnvelope>(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries).err(),
            Some(LoadError::InvalidState(StateValidationError::PlayerWork(
                expected
            )))
        );
    }
    let mut missing = saved;
    missing["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"]
        .as_object_mut()
        .unwrap()
        .remove("aim")
        .unwrap();
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());

    let before = calculate_matter_accounting(&state).unwrap().total();
    while state.control_tick() < start.completes_at() {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
