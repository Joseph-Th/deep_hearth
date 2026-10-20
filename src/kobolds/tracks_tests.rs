//! Player-local spoor follows actual slow scout steps and survives continuation.

use super::*;
use crate::content::{FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
use crate::logistics::add_traversable_edge_for_test;
use crate::material::{CommodityKey, MaterialComposition};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{advance_control_tick, advance_tick};
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

fn setup() -> (Registries, AppState, [VoxelCoord; 3]) {
    let registries = build_registries();
    let mut state = AppState::new();
    let route = [
        VoxelCoord::new(0, 0, 0),
        VoxelCoord::new(1, 0, 0),
        VoxelCoord::new(2, 0, 0),
    ];
    add_traversable_edge_for_test(&mut state, route[0], route[1]);
    add_traversable_edge_for_test(&mut state, route[1], route[2]);
    state.kobold_state_mut().apply_opening_colony(route);
    (registries, state, route)
}

fn advance_to(registries: &Registries, state: &mut AppState, until: u64) {
    while state.tick().value() < until {
        if state.logistics().player().is_some() {
            let _ = advance_control_tick(registries, state).unwrap();
        } else {
            let _ = advance_tick(registries, state).unwrap();
        }
    }
}

#[test]
fn real_haul_leaves_local_directional_tracks_that_replay_but_do_not_reveal_remote_activity() {
    let (registries, mut state, [den, junction, tip]) = setup();
    let cargo = SurfaceResourceSpec::new(
        SurfaceResourceKind::Drop,
        tip,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        Mass::from_milligrams(250_000),
        Temperature::from_millikelvin(293_150),
        MaterialComposition::pure(MATERIAL_STONE),
    )
    .unwrap();
    validate_create_surface_resources(&registries, &state, [cargo])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    advance_to(&registries, &mut state, 195);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().tracks.len(),
        4
    );
    assert!(
        state
            .kobold_state()
            .colony
            .as_ref()
            .unwrap()
            .tracks
            .iter()
            .any(|track| {
                track.position() == tip
                    && track.toward() == junction
                    && track.left_at() == SimulationTick::new(190)
                    && track.observed_activity() == KoboldTrailKind::Hauling
            })
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    assert_eq!(state, loaded);
    assert!(
        available_nearby_kobold_tracks(&registries, &state)
            .next()
            .is_none()
    );
    validate_admit_player(&registries, &state, junction)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &loaded, junction)
        .unwrap()
        .commit(&mut loaded)
        .unwrap();
    let clues = available_nearby_kobold_tracks(&registries, &state).collect::<Vec<_>>();
    assert_eq!(clues.len(), 4);
    assert!(clues.iter().any(|track| track.position() == den));
    assert!(clues.iter().any(|track| {
        track.position() == tip && track.observed_activity() == KoboldTrailKind::Hauling
    }));
    assert_eq!(
        clues,
        available_nearby_kobold_tracks(&registries, &loaded).collect::<Vec<_>>()
    );
    for _ in 0..50 {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(state, loaded);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}

#[test]
fn tracks_fade_without_new_steps_and_stationary_scouts_leave_no_false_evidence() {
    let (registries, mut state, [den, _junction, _]) = setup();
    advance_to(&registries, &mut state, 180);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().tracks.len(),
        1
    );
    validate_admit_player(&registries, &state, den)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        available_nearby_kobold_tracks(&registries, &state).count(),
        1
    );
    advance_to(&registries, &mut state, 190);
    // The player physically occupies the retreat destination. An attempted
    // but blocked escape cannot leave invented footprints there.
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().tracks.len(),
        1
    );
    let count = state.kobold_state().colony.as_ref().unwrap().tracks.len();
    advance_to(&registries, &mut state, 315);
    assert_eq!(
        state.kobold_state().colony.as_ref().unwrap().tracks.len(),
        count
    );
    assert_eq!(
        available_nearby_kobold_tracks(&registries, &state).count(),
        0
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn distant_observers_see_no_tracks_and_long_patrols_keep_bounded_history() {
    let (registries, mut state, _) = setup();
    advance_to(&registries, &mut state, 810);
    let recent = &state.kobold_state().colony.as_ref().unwrap().tracks;
    assert_eq!(recent.len(), super::MAX_RECENT_TRACKS);
    assert!(
        recent
            .windows(2)
            .all(|pair| pair[0].left_at() < pair[1].left_at())
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&saved).unwrap();
    let mut loaded = loaded.into_state(&registries).unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(500, 0, 500))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &loaded, VoxelCoord::new(500, 0, 500))
        .unwrap()
        .commit(&mut loaded)
        .unwrap();
    assert!(
        available_nearby_kobold_tracks(&registries, &state)
            .next()
            .is_none()
    );
    assert_eq!(state, loaded);
}

#[test]
fn malformed_tracks_are_rejected_at_trusted_load() {
    let (registries, mut state, [den, _, _]) = setup();
    advance_to(&registries, &mut state, 185);
    let valid = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut forged = valid.clone();
    forged["state"]["systems"]["kobolds"]["colony"]["tracks"][0]["toward"] =
        serde_json::to_value(VoxelCoord::new(900, 0, 0)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(forged).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut future = valid.clone();
    future["state"]["systems"]["kobolds"]["colony"]["tracks"][0]["at"] =
        serde_json::to_value(SimulationTick::new(980)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_value(future).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut duplicates = valid.clone();
    let mark = duplicates["state"]["systems"]["kobolds"]["colony"]["tracks"][0].clone();
    duplicates["state"]["systems"]["kobolds"]["colony"]["tracks"]
        .as_array_mut()
        .unwrap()
        .push(mark);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(duplicates).unwrap();
    assert!(decoded.into_state(&registries).is_err());

    let mut missing = valid;
    missing["state"]["systems"]["kobolds"]["colony"]
        .as_object_mut()
        .unwrap()
        .remove("tracks");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
    assert_eq!(den, VoxelCoord::new(0, 0, 0));
}
