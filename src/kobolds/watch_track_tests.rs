//! Real watchman spoor is local, role-distinct and strictly validated at load.

use super::*;
use crate::kobolds::{KoboldTrailKind, available_nearby_kobold_tracks};

#[test]
fn guard_warning_leaves_two_real_role_distinct_tracks_and_fades() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.watch_tracks.len(), 1);
    assert_eq!(colony.watch_tracks[0].position(), route[0]);
    assert_eq!(colony.watch_tracks[0].toward(), route[1]);
    assert_eq!(colony.watch_tracks[0].role(), KoboldRole::Watchman);
    assert_eq!(
        colony.watch_tracks[0].observed_activity(),
        KoboldTrailKind::Walking
    );
    assert_eq!(
        available_nearby_kobold_tracks(&registries, &state).count(),
        0
    );
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    let colony = state.kobold_state().colony.as_ref().unwrap();
    assert_eq!(colony.watch_tracks.len(), 2);
    assert_eq!(colony.watch_tracks[1].position(), route[1]);
    assert_eq!(colony.watch_tracks[1].toward(), route[0]);
    assert_eq!(colony.watch_tracks[1].left_at(), SimulationTick::new(205));
    assert_eq!(
        colony.watch_tracks[1].observed_activity(),
        KoboldTrailKind::Fleeing
    );
    assert!(
        colony
            .tracks
            .iter()
            .all(|mark| mark.role() == KoboldRole::Scout)
    );
    let visible = available_nearby_kobold_tracks(&registries, &state)
        .filter(|track| track.role() == KoboldRole::Watchman)
        .collect::<Vec<_>>();
    assert_eq!(visible, colony.watch_tracks);
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut replay = decoded.into_state(&registries).unwrap();
    for _ in 0..15 {
        assert_eq!(
            crate::simulation::advance_control_tick(&registries, &mut state).unwrap(),
            crate::simulation::advance_control_tick(&registries, &mut replay).unwrap(),
        );
    }
    assert_eq!(state, replay);
    advance_to(&registries, &mut state, 335);
    assert!(
        available_nearby_kobold_tracks(&registries, &state)
            .all(|track| track.role() != KoboldRole::Watchman),
        "old guard footsteps fade without global cleanup"
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn outer_picket_makes_a_four_leg_spoor_path_without_remote_disclosure() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Electrical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    advance_to(&registries, &mut state, 200);
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    advance_to(&registries, &mut state, 205);
    relocate_player_for_test(&mut state, VoxelCoord::new(40, 0, 0));
    for tick in (210..=365).step_by(5) {
        advance_to(&registries, &mut state, tick);
        if state.kobold_state().colony.as_ref().unwrap().watch_leg == 1 {
            break;
        }
    }
    let at_tip = state.tick().value();
    assert_eq!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_position(),
        route[2]
    );
    assert_eq!(
        available_nearby_kobold_tracks(&registries, &state).count(),
        0
    );
    let bytes = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&bytes).unwrap();
    let mut replay = loaded.into_state(&registries).unwrap();
    advance_to(&registries, &mut state, at_tip + 25);
    advance_to(&registries, &mut replay, at_tip + 25);
    assert_eq!(state, replay);
    let guard = state.kobold_state().colony.as_ref().unwrap();
    let tail = &guard.watch_tracks[guard.watch_tracks.len() - 4..];
    assert_eq!(
        tail.iter()
            .map(|mark| (mark.position(), mark.toward()))
            .collect::<Vec<_>>(),
        vec![
            (route[0], route[1]),
            (route[1], route[2]),
            (route[2], route[1]),
            (route[1], route[0]),
        ],
    );
    assert!(tail.iter().all(|mark| mark.role() == KoboldRole::Watchman));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn blocked_post_creates_no_guard_tracks_and_trusted_load_rejects_forged_spoor() {
    let (registries, mut state, route) =
        scenario(EnergyCarrier::Mechanical, Some(VoxelCoord::new(0, 0, 0)));
    advance_to(&registries, &mut state, 195);
    ration(&registries, &mut state, route[0]);
    let mut blocked = state.clone();
    validate_admit_player(&registries, &blocked, route[1])
        .unwrap()
        .commit(&mut blocked)
        .unwrap();
    advance_to(&registries, &mut blocked, 200);
    assert!(
        blocked
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .watch_tracks
            .is_empty()
    );
    advance_to(&registries, &mut state, 200);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    for field in ["role", "toward", "at", "kind"] {
        let mut forged = valid.clone();
        let mark = &mut forged["state"]["systems"]["kobolds"]["colony"]["watch_tracks"][0];
        mark[field] = match field {
            "role" => serde_json::json!("Scout"),
            "toward" => serde_json::to_value(VoxelCoord::new(900, 0, 0)).unwrap(),
            "at" => serde_json::json!(185),
            "kind" => serde_json::json!("Hauling"),
            _ => unreachable!(),
        };
        let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
        assert!(
            decoded.into_state(&registries).is_err(),
            "invalid guard track {field}"
        );
    }
    let mut inflated = valid.clone();
    let watch = &mut inflated["state"]["systems"]["kobolds"]["colony"]["watch_tracks"];
    let duplicate = watch[0].clone();
    watch
        .as_array_mut()
        .unwrap()
        .extend((0..8).map(|_| duplicate.clone()));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(inflated).unwrap();
    assert!(decoded.into_state(&registries).is_err());
    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("watch_tracks");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}
