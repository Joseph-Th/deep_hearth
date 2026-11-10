//! Save admission must not accept a scout that has not had time to walk its route.

use super::*;
use crate::content::build_registries;
use crate::core::state::validate_loaded_state;
use crate::logistics::add_traversable_edge_for_test;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::advance_tick;

#[test]
fn saved_scout_cannot_leave_its_den_before_the_first_patrol_beat() {
    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let original = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();

    for step in [1_u8, 2] {
        let mut forged = original.clone();
        forged["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(step);
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert_eq!(
            decoded.into_state(&registries),
            Err(crate::persistence::LoadError::InvalidState(
                crate::core::state::StateValidationError::Kobolds(
                    "kobold patrol cursor precedes its first physical route step"
                )
            )),
            "a scout cannot reach step {step} at the initial world tick"
        );
    }
}

#[test]
fn initial_patrol_departure_and_tip_replay_only_after_their_real_step_times() {
    let registries = build_registries();
    let mut state = AppState::new();
    let den = VoxelCoord::new(-1, 0, 0);
    let junction = VoxelCoord::new(0, 0, 0);
    let tip = VoxelCoord::new(1, 0, 0);
    add_traversable_edge_for_test(&mut state, den, junction);
    add_traversable_edge_for_test(&mut state, junction, tip);
    state
        .kobold_state_mut()
        .apply_opening_colony([den, junction, tip]);

    for _ in 0..FIRST_SCOUT_TICK - 1 {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        den
    );
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(1);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(matches!(
        loaded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Kobolds(
                "kobold patrol cursor precedes its first physical route step"
            )
        ))
    ));

    let _ = advance_tick(&registries, &mut state).unwrap();
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        junction
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    for _ in 0..SCOUT_STEP_TICKS - 1 {
        assert_eq!(
            advance_tick(&registries, &mut state).unwrap(),
            advance_tick(&registries, &mut replay).unwrap()
        );
    }
    // Without recent tracks, the old route validator accepted this claimed
    // second step four ticks before its earliest possible arrival.
    let mut forged = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    forged["state"]["systems"]["kobolds"]["colony"]["scout_step"] = serde_json::json!(2);
    forged["state"]["systems"]["kobolds"]["colony"]["tracks"] = serde_json::json!([]);
    let loaded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(matches!(
        loaded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Kobolds(
                "kobold patrol cursor precedes its first physical route step"
            )
        ))
    ));

    assert_eq!(
        advance_tick(&registries, &mut state).unwrap(),
        advance_tick(&registries, &mut replay).unwrap()
    );
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .scout_position(),
        tip
    );
    assert_eq!(state, replay);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&bytes)
            .unwrap()
            .into_state(&registries)
            .unwrap(),
        state
    );
}
