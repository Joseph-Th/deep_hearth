//! A temple defender is a persistent local actor, not a distance-triggered spawn.

use super::*;

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::validate_loaded_state;
use crate::logistics::{HorizontalControlInput, PlayerControlInput, relocate_player_for_test};
use crate::logistics::{PlayerWalkProjectionError, project_player_adjacent_walk};
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{validate_admit_player, validate_respawn_player};
use crate::simulation::{advance_control_tick, advance_control_tick_with_input};
use crate::survival::{Vitality, player_record};
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
fn heavy_castes_are_solid_on_the_actual_continuous_player_walk_sweep() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    let original = state.logistics().player().unwrap().body().foot_position();
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        route[1]
    ));
    assert!(crate::kobolds::temple_caste_occupies_voxel(
        &registries,
        &state,
        VoxelCoord::new(route[1].x(), route[1].y() + 1, route[1].z()),
    ));
    assert!(
        !crate::kobolds::temple_caste_occupies_voxel(&registries, &state, route[0]),
        "a displaced guardian must not block its vacant ancestral post"
    );
    assert!(matches!(
        project_player_adjacent_walk(&registries, &state, route[1]),
        Err(PlayerWalkProjectionError::DestinationUnreachableBeforeBoundary { .. })
    ));
    let forward = PlayerControlInput::new(HorizontalControlInput::new(0, 1_000_000).unwrap());
    for _ in 0..68 {
        let _ = advance_control_tick_with_input(&registries, &mut state, forward).unwrap();
    }
    let player = state.logistics().player().unwrap();
    let advanced = player.body().foot_position();
    assert!(
        advanced.z_nanometers() > original.z_nanometers(),
        "the player must still advance through the open part of the hall"
    );
    assert_eq!(
        player.position(),
        route[2],
        "the warden must hold its occupied cell"
    );
    assert!(
        advanced.z_nanometers() < route[1].z() * 1_000_000_000,
        "continuous collision must stop at the defender's face"
    );
    assert_eq!(visible_guard(&registries, &state), Some(route[1]));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn player_admission_and_trusted_load_reject_overlapping_temple_residents() {
    let (registries, state, route) = occupied_temple();
    let mut unadmitted = AppState::new();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    validate_generate_chunk(
        &registries,
        &unadmitted,
        seed,
        ChunkCoord::containing(route[0]),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut unadmitted)
    .unwrap();
    assert!(
        validate_admit_player(&registries, &unadmitted, route[0]).is_err(),
        "wardens are not passable initial spawn points"
    );
    let archivist = VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 2);
    assert!(
        validate_admit_player(&registries, &unadmitted, archivist).is_err(),
        "the much larger archivist also occupies real world space"
    );
    let mut overlapping = state;
    relocate_player_for_test(&mut overlapping, route[0]);
    assert!(validate_loaded_state(&registries, &overlapping).is_err());
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &overlapping)).unwrap();
    assert!(
        serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .is_err(),
        "the trusted-load boundary must refuse impossible contact"
    );
}

#[test]
fn massive_archivist_occupies_a_real_two_by_two_three_high_body() {
    let (registries, state, route) = occupied_temple();
    let archivist = VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 2);
    for dx in 0..=1 {
        for dz in -1..=0 {
            for dy in 0..=2 {
                let voxel =
                    VoxelCoord::new(archivist.x() + dx, archivist.y() + dy, archivist.z() + dz);
                assert_eq!(
                    state.world().block_at(voxel),
                    Some(crate::world::BlockStateId::AIR),
                    "the massive resident needs actual excavated volume"
                );
                assert!(crate::kobolds::temple_caste_occupies_voxel(
                    &registries,
                    &state,
                    voxel
                ));
            }
        }
    }
    for voxel in [
        VoxelCoord::new(archivist.x() - 1, archivist.y(), archivist.z()),
        VoxelCoord::new(archivist.x() + 2, archivist.y(), archivist.z()),
        VoxelCoord::new(archivist.x(), archivist.y() + 3, archivist.z()),
    ] {
        assert!(!crate::kobolds::temple_caste_occupies_voxel(
            &registries,
            &state,
            voxel
        ));
    }
}

#[test]
fn respawn_falls_back_when_a_warden_now_guards_the_original_spawn() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x(), route[2].y(), route[2].z() - 1),
    );
    advance_to(&registries, &mut state, 10);
    assert_eq!(visible_guard(&registries, &state), Some(route[2]));
    let death = VoxelCoord::new(route[2].x() + 3, route[2].y(), route[2].z());
    relocate_player_for_test(&mut state, death);
    let living = state.survival().player().copied().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record(
            living.metabolic_energy(),
            living.hydration(),
            Vitality::ZERO,
            living.nutrition(),
            living.vitality_recovery_remainder(),
        ),
    );
    let outcome = validate_respawn_player(&registries, &state)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(!outcome.used_spawn_anchor());
    assert_eq!(outcome.position(), death);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
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
fn displaced_warden_finishes_walking_home_after_player_leaves_temple_chunk() {
    let (registries, mut state, route) = occupied_temple();
    advance_to(&registries, &mut state, 5);
    assert_eq!(state.kobold_state().temple_wardens[0].step, 1);
    relocate_player_for_test(
        &mut state,
        VoxelCoord::new(route[2].x() + 24, route[2].y(), route[2].z()),
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    advance_to(&registries, &mut state, 10);
    advance_to(&registries, &mut replay, 10);
    assert_eq!(state, replay);
    assert!(
        state.kobold_state().temple_wardens.is_empty(),
        "a displaced defender must finish its physical retreat rather than freeze forever offscreen"
    );
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
