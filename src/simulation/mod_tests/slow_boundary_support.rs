//! Neutral fast-forward must retain control-rate falling when a slow event removes support.

use super::*;
use crate::content::{
    BASIC_TIMBER_COVER_PANEL_AREA, BASIC_TIMBER_COVER_PANEL_THICKNESS, FORM_BOARD,
    STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER, STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
};
use crate::core::state::validate_loaded_state;
use crate::matter::calculate_matter_accounting;
use crate::spatial::VoxelBounds;
use crate::structural::{
    StructuralElementGeometry, add_structural_element, materialize_structural_element_for_test,
    validate_activate_structural_element, validate_start_player_structural_dismantling,
};

#[test]
fn due_dismantling_of_the_player_floor_must_replay_control_rate_gravity() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let floor = VoxelCoord::new(0, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    let geometry = StructuralElementGeometry::new(
        VoxelBounds::new(floor, VoxelCoord::new(1, floor_y, 1)).unwrap(),
        BASIC_TIMBER_COVER_PANEL_THICKNESS,
        BASIC_TIMBER_COVER_PANEL_AREA,
    )
    .unwrap();
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        geometry,
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, panel)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let work = validate_start_player_structural_dismantling(
        &registries,
        &state,
        panel,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(work.completes_at().value() > state.tick().value() + 1);
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        advance_tick(&registries, &mut state),
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick(),
        }),
        "fast-forward would omit the boundary control sample after the floor vanishes"
    );
    assert_eq!(state, before);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(state, replay);
    let mut final_control = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_control = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    let final_control = final_control.unwrap();
    assert_eq!(replay.tick(), work.completes_at());
    assert!(
        final_control
            .simulation()
            .unwrap()
            .structural_dismantling()
            .is_some()
    );
    assert!(final_control.body_motion().is_some());
    assert!(replay.structures().get_element(panel).is_none());
    assert!(!player_body_is_grounded(&registries, &replay));
    assert!(
        replay
            .logistics()
            .player()
            .unwrap()
            .body()
            .vertical_velocity()
            .nanometers_per_second()
            < 0
    );
    assert_eq!(
        calculate_matter_accounting(&replay).unwrap().total(),
        matter_before,
        "dismantling transfers embodied matter to the reserved carried inventory"
    );
    assert_eq!(validate_loaded_state(&registries, &replay), Ok(()));
}

#[test]
fn temple_warden_departing_under_player_requires_boundary_gravity_step() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::player::validate_admit_player;
    use crate::world::{
        ChunkCoord, WorldGenerationSeed, temple_warden_route_at, validate_generate_chunk,
    };

    let registries = build_registries();
    let mut state = AppState::new();
    let seed = WorldGenerationSeed::new(0x0054_454D_504C_4553);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let route = temple_warden_route_at(&registries, &state, VoxelCoord::new(8, -19, 8))
        .expect("authored temple route");
    let post = route[0];
    // Stand on the warden's upper occupied voxel, with no stone floor:
    // the resident's real body is the only support at this height.
    for y in post.y() + 2..=post.y() + 4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(post.x(), y, post.z()),
            BlockStateId::AIR,
        );
    }
    let perch = VoxelCoord::new(post.x(), post.y() + 2, post.z());
    validate_admit_player(&registries, &state, perch)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    // Quiet resident ticks retain the fast path and remain bit-identical
    // to the actual 20 Hz neutral sequence.
    let mut neutral = state.clone();
    while state.tick().value() < 4 {
        let fast = advance_tick(&registries, &mut state).unwrap();
        let mut boundary = None;
        for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
            boundary = Some(advance_control_tick(&registries, &mut neutral).unwrap());
        }
        assert_eq!(fast, *boundary.unwrap().simulation().unwrap());
        assert_eq!(state, neutral);
    }
    let before = state.clone();
    let mut control = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let skipped = advance_tick(&registries, &mut state);
    let mut final_control = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        final_control = Some(advance_control_tick(&registries, &mut control).unwrap());
    }
    assert!(
        !crate::kobolds::temple_caste_occupies_voxel(&registries, &control, post),
        "a real warden step must remove the moving floor"
    );
    assert!(final_control.unwrap().body_motion().is_some());
    assert!(!player_body_is_grounded(&registries, &control));
    assert_eq!(
        skipped,
        Err(TickError::ControlSteppingRequired {
            current: before.control_tick()
        })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &control), Ok(()));
}

#[test]
fn nearby_dismantling_preserves_fast_forward_when_the_player_floor_is_unchanged() {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let nearby = VoxelCoord::new(1, floor_y - 1, 0);
    set_generated_block_for_test(registries.world(), &mut state, nearby, BlockStateId::AIR);
    let panel = add_structural_element(
        &registries,
        &mut state,
        STRUCTURAL_PROFILE_AXIAL_COMPRESSION,
        MATERIAL_WOOD,
        StructuralElementGeometry::new(
            VoxelBounds::new(nearby, VoxelCoord::new(2, floor_y, 1)).unwrap(),
            BASIC_TIMBER_COVER_PANEL_THICKNESS,
            BASIC_TIMBER_COVER_PANEL_AREA,
        )
        .unwrap(),
        true,
    )
    .unwrap();
    materialize_structural_element_for_test(&registries, &mut state, panel, FORM_BOARD);
    let _ = validate_activate_structural_element(&registries, &state, panel)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let work = validate_start_player_structural_dismantling(
        &registries,
        &state,
        panel,
        STRUCTURAL_CONSTRUCTION_HAND_SET_MEMBER,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    while state.tick().value() + 1 < work.completes_at().value() {
        let _ = advance_tick(&registries, &mut state).unwrap();
    }
    let before = state.clone();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    let fast = advance_tick(&registries, &mut state).unwrap();
    let mut normal = None;
    for _ in 0..CONTROL_TICKS_PER_SIMULATION_TICK {
        normal = Some(advance_control_tick(&registries, &mut replay).unwrap());
    }
    assert_eq!(fast, *normal.unwrap().simulation().unwrap());
    assert_eq!(state, replay);
    assert!(state.structures().get_element(panel).is_none());
    assert_eq!(
        state.logistics().player().unwrap().body(),
        before.logistics().player().unwrap().body()
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
