//! A temple defender is a persistent local actor, not a distance-triggered spawn.

use super::*;

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::validate_loaded_state;
use crate::logistics::relocate_player_for_test;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::world::{WorldGenerationSeed, set_generated_block_for_test, validate_generate_chunk};

fn occupied_temple() -> (Registries, AppState, [VoxelCoord; 3]) {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    // This authored replay seed resolves its ancient precinct to column (0,0).
    let chunk = ChunkCoord::new(0, 0);
    let mut state = AppState::new();
    validate_generate_chunk(&registries, &state, seed, chunk, BLOCK_STATE_STONE)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let sample = VoxelCoord::new(
        chunk.world_min_x().unwrap() + 8,
        -19,
        chunk.world_min_z().unwrap() + 8,
    );
    let route = temple_warden_route_at(&registries, &state, sample).unwrap();
    validate_admit_player(&registries, &state, route[2])
        .unwrap()
        .commit(&mut state)
        .unwrap();
    (registries, state, route)
}

fn advance_to(registries: &Registries, state: &mut AppState, target: u64) {
    while state.tick().value() < target {
        let _ = advance_control_tick(registries, state).unwrap();
    }
}

fn visible_guard(registries: &Registries, state: &AppState) -> Option<VoxelCoord> {
    crate::kobolds::available_nearby_kobold_scouts(registries, state)
        .find(|sighting| sighting.role() == crate::kobolds::KoboldRole::TempleWarden)
        .map(|sighting| sighting.position())
}

fn defending_guard(registries: &Registries, state: &AppState) -> Option<bool> {
    crate::kobolds::available_nearby_kobold_scouts(registries, state)
        .find(|sighting| sighting.role() == crate::kobolds::KoboldRole::TempleWarden)
        .map(|sighting| sighting.is_defending())
}

#[test]
fn intruder_draws_a_warden_forward_by_one_real_cell_then_it_withdraws() {
    let (registries, mut state, route) = occupied_temple();
    assert_eq!(visible_guard(&registries, &state), Some(route[0]));
    assert_eq!(defending_guard(&registries, &state), Some(false));
    advance_to(&registries, &mut state, 5);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(defending_guard(&registries, &state), Some(true));
    assert_eq!(state.kobold_state().temple_wardens.len(), 1);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(visible_guard(&registries, &replay), Some(route[1]));
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(
        state.kobold_state().temple_wardens[0].step,
        1,
        "the warden cannot occupy the living intruder's voxel"
    );
    let elsewhere = VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z());
    relocate_player_for_test(&mut state, elsewhere);
    advance_to(&registries, &mut state, 15);
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn a_warden_takes_a_second_supported_step_but_never_enters_the_player() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1),
    );
    advance_to(&registries, &mut state, 10);
    assert_eq!(visible_guard(&registries, &state), Some(route[2]));
    assert_eq!(state.kobold_state().temple_wardens[0].step, 2);
    assert_eq!(defending_guard(&registries, &state), Some(true));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z()),
    );
    advance_to(&registries, &mut state, 15);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    advance_to(&registries, &mut state, 20);
    assert!(state.kobold_state().temple_wardens.is_empty());
}

#[test]
fn blocking_the_real_hall_throat_prevents_a_warden_move() {
    let (registries, mut state, route) = occupied_temple();
    set_generated_block_for_test(registries.world(), &mut state, route[1], BLOCK_STATE_STONE);
    advance_to(&registries, &mut state, 5);
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(visible_guard(&registries, &state), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn missing_stone_support_prevents_warden_advance() {
    let (registries, mut state, route) = occupied_temple();
    let floor = VoxelCoord::new(route[1].x(), route[1].y() - 1, route[1].z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        floor,
        crate::world::BlockStateId::AIR,
    );
    advance_to(&registries, &mut state, 5);
    assert_eq!(visible_guard(&registries, &state), Some(route[0]));
    assert!(state.kobold_state().temple_wardens.is_empty());
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn removed_floor_strands_a_displaced_warden_without_corrupting_save_continuation() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let floor = VoxelCoord::new(route[1].x(), route[1].y() - 1, route[1].z());
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        floor,
        crate::world::BlockStateId::AIR,
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
}

#[test]
fn internal_control_transaction_snapshot_preserves_and_isolates_temple_guard_progress() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let mut staged = state.snapshot_for_control_transaction();
    assert_eq!(staged, state);
    relocate_player_for_test(
        &mut staged,
        VoxelCoord::new(route[2].x() + 4, route[2].y(), route[2].z()),
    );
    advance_to(&registries, &mut staged, 10);
    assert!(staged.kobold_state().temple_wardens.is_empty());
    assert_eq!(
        state.kobold_state().temple_wardens[0].step,
        1,
        "a rejected staged boundary cannot alter original persistent residents"
    );
    assert_eq!(validate_loaded_state(&registries, &staged), Ok(()));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn hostile_saved_warden_cursors_are_rejected_without_spawning_actor() {
    let (registries, mut state, _) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let save = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let mut out_of_range = save.clone();
    out_of_range["state"]["systems"]["kobolds"]["temple_wardens"][0]["step"] = serde_json::json!(3);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(out_of_range).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut duplicate = save.clone();
    let same = duplicate["state"]["systems"]["kobolds"]["temple_wardens"][0].clone();
    duplicate["state"]["systems"]["kobolds"]["temple_wardens"]
        .as_array_mut()
        .unwrap()
        .push(same);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(duplicate).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut false_home = save.clone();
    false_home["state"]["systems"]["kobolds"]["temple_wardens"][0]["chunk"]["x"] =
        serde_json::json!(8);
    let loaded = serde_json::from_value::<LoadedSaveEnvelope>(false_home).unwrap();
    assert!(loaded.into_state(&registries).is_err());

    let mut missing = save;
    missing["state"]["systems"]["kobolds"]
        .as_object_mut()
        .unwrap()
        .remove("temple_wardens");
    assert!(serde_json::from_value::<LoadedSaveEnvelope>(missing).is_err());
}
