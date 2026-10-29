//! Landing at an exact voxel face must have the same support as standing there.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::AppState;
use crate::logistics::player_body_is_grounded;
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed,
    validate_generate_chunk,
};

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
