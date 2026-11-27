//! Exact voxel-face contacts and vertical traversal contracts.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::AppState;
use crate::logistics::player_body_is_grounded;
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

#[test]
fn adjacent_descent_rejects_a_standing_cavity_behind_an_obstructed_entrance() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let origin = VoxelCoord::new(5, 30, 5);
    let destination = VoxelCoord::new(6, 27, 5);
    for (x, y, block) in [
        (5, 29, BLOCK_STATE_STONE),
        (5, 30, BlockStateId::AIR),
        (5, 31, BlockStateId::AIR),
        (6, 26, BLOCK_STATE_STONE),
        (6, 27, BlockStateId::AIR),
        (6, 28, BlockStateId::AIR),
        (6, 29, BLOCK_STATE_STONE),
        (6, 30, BLOCK_STATE_STONE),
        (6, 31, BlockStateId::AIR),
    ] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, y, 5),
            block,
        );
    }
    validate_admit_player(&registries, &state, origin)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(super::super::project_player_standing_space(
        &registries,
        &state,
        destination
    ));
    assert!(!project_player_descent_approach(
        &registries,
        &state,
        destination
    ));

    // Removing only the entrance wall leaves a closer floor. The deep
    // cavity must not become the reported landing until that floor is removed.
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, 30, 5),
        BlockStateId::AIR,
    );
    assert!(!project_player_descent_approach(
        &registries,
        &state,
        destination
    ));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, 29, 5),
        BlockStateId::AIR,
    );
    assert!(project_player_descent_approach(
        &registries,
        &state,
        destination
    ));
    assert!(!project_player_descent_approach(
        &registries,
        &state,
        VoxelCoord::new(i64::MIN, 27, i64::MIN)
    ));
    assert!(!project_player_descent_approach(
        &registries,
        &state,
        VoxelCoord::new(6, 28, 5)
    ));
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, 31, 5),
        BLOCK_STATE_STONE,
    );
    assert!(!project_player_descent_approach(
        &registries,
        &state,
        destination
    ));
}

#[test]
fn descending_to_exact_floor_face_is_grounded_without_a_phantom_airborne_tick() {
    for (floor_cell, floor_height) in [
        (0, NANOMETERS_PER_VOXEL),
        (-1, 0),
        (-2, -NANOMETERS_PER_VOXEL),
    ] {
        let blocks = |voxel: VoxelCoord| voxel == VoxelCoord::new(0, floor_cell, 0);
        let footprint = (0, 0, 0, 0);
        let before = floor_height + 100;

        assert_eq!(
            crossed_floor_y(&blocks, footprint, floor_height, before),
            Ok(Some(floor_height)),
            "exact-face contact must count as a landing at floor cell {floor_cell}",
        );
        assert_eq!(
            crossed_floor_y(&|_| false, footprint, floor_height, before),
            Ok(None),
            "an empty column cannot provide a landing surface",
        );
        assert_eq!(
            crossed_floor_y(&blocks, footprint, floor_height + 1, before),
            Ok(None),
            "a foot remaining above the floor cannot land early",
        );
    }
}

#[test]
fn exact_face_landing_settles_vertical_velocity_on_the_live_control_tick() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let ground = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(0, y, 0)) != Some(BlockStateId::AIR))
        .expect("generated stone chunk must provide a solid surface at its corner");
    let spawn = VoxelCoord::new(0, ground + 1, 0);
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();

    // An initially motionless body can move exactly to the floor on its next
    // gravity sample. The discrete integration makes this a realistic landing,
    // not a fabricated fractional collision or a hand-picked vertical velocity.
    let floor_y = spawn.y() * NANOMETERS_PER_VOXEL;
    let displacement = integrate_velocity(-gravity_velocity_delta(&registries).unwrap()).unwrap();
    assert!(displacement < 0);
    let player = state.logistics().player().copied().unwrap();
    let before = player.body();
    let raised = before.with_motion(
        WorldPoint::new(
            before.foot_position().x_nanometers(),
            floor_y - displacement,
            before.foot_position().z_nanometers(),
        ),
        before.horizontal_velocity(),
        before.vertical_velocity(),
        before.stance(),
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_player_body_motion(
        revision,
        revision + 1,
        before,
        raised,
        player.locomotion_exertion(),
        player.pending_fall_vitality_loss_ppm(),
    );
    assert!(!player_body_is_grounded(&registries, &state));

    let outcome = advance_control_tick(&registries, &mut state).unwrap();
    let motion = outcome
        .body_motion()
        .expect("fall must move the player body");
    assert!(
        motion.grounded(),
        "the exact-face landing must settle immediately"
    );
    assert_eq!(motion.after().foot_position().y_nanometers(), floor_y);
    assert_eq!(
        motion.after().vertical_velocity().nanometers_per_second(),
        0
    );
    assert!(player_body_is_grounded(&registries, &state));
}

#[test]
fn exact_ceiling_contact_settles_upward_velocity_on_the_live_control_tick() {
    let registries = build_registries();
    let mut state = AppState::new();
    let spawn = VoxelCoord::new(5, 30, 5);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(41),
        ChunkCoord::containing(spawn),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    for (y, block) in [
        (29, BLOCK_STATE_STONE),
        (30, BlockStateId::AIR),
        (31, BlockStateId::AIR),
        (32, BLOCK_STATE_STONE),
    ] {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(5, y, 5),
            block,
        );
    }
    validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let player = state.logistics().player().copied().unwrap();
    let before = player.body();
    let locomotion = registries.survival().physiology().locomotion();
    let height = length_nanometers(body_height(locomotion, before.stance())).unwrap();
    let maximum_feet = 32 * NANOMETERS_PER_VOXEL - height;
    let gravity = gravity_velocity_delta(&registries).unwrap();
    let displacement = integrate_velocity(gravity).unwrap();
    let raised = before.with_motion(
        WorldPoint::new(
            before.foot_position().x_nanometers(),
            maximum_feet - displacement,
            before.foot_position().z_nanometers(),
        ),
        before.horizontal_velocity(),
        VerticalVelocity::new(2 * gravity),
        before.stance(),
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_player_body_motion(
        revision,
        revision + 1,
        before,
        raised,
        player.locomotion_exertion(),
        player.pending_fall_vitality_loss_ppm(),
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );

    let motion = advance_control_tick(&registries, &mut state)
        .unwrap()
        .body_motion()
        .unwrap();
    assert_eq!(motion.after().foot_position().y_nanometers(), maximum_feet);
    assert!(!motion.grounded());
    assert_eq!(
        motion.after().vertical_velocity().nanometers_per_second(),
        0
    );
}
