//! Local player sightings must originate at the physical first-person eye.

use super::*;
use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::validate_loaded_state;
use crate::logistics::{PlayerControlInput, PlayerStance};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick_with_input;
use crate::world::{
    BlockStateId, ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

#[test]
fn overhead_obstruction_cannot_be_seen_under_from_the_players_feet() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player = VoxelCoord::new(0, 0, 0);
    let resident = VoxelCoord::new(2, 0, 0);
    let overhang = VoxelCoord::new(1, 1, 0);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(7),
        ChunkCoord::containing(player),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    // Sky-lit endpoints and a clear low passage beneath a solid overhang.
    for x in [player.x(), resident.x()] {
        for y in 0..WORLD_MAX_Y_EXCLUSIVE {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(1, 0, 0),
        BlockStateId::AIR,
    );
    // The player's body must remain grounded through the crouch control
    // sample; a generated cave can otherwise leave its first foot cell airborne.
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(0, -1, 0),
        BLOCK_STATE_STONE,
    );
    // Generation can leave this middle cell as air; place an actual solid
    // overhead barrier instead of assuming the seed did.
    set_generated_block_for_test(registries.world(), &mut state, overhang, BLOCK_STATE_STONE);
    assert_eq!(state.world().block_at(overhang), Some(BLOCK_STATE_STONE));
    validate_admit_player(&registries, &state, player)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let head = VoxelCoord::new(resident.x(), resident.y() + 1, resident.z());
    assert_eq!(
        crate::world::player_eye_voxel(&registries, &state),
        Some(VoxelCoord::new(player.x(), player.y() + 1, player.z()))
    );
    assert!(
        crate::world::has_clear_two_step_sightline(&registries, &state, player, resident),
        "a feet-as-eyes probe can see through the low corridor"
    );
    assert!(
        !player_sees_resident_body(&registries, &state, player, [resident, head]),
        "the actual eye cannot see under the overhang"
    );

    let _ = advance_control_tick_with_input(
        &registries,
        &mut state,
        PlayerControlInput::NEUTRAL.with_crouch(true),
    )
    .unwrap();
    assert_eq!(
        state.logistics().player().unwrap().body().stance(),
        PlayerStance::Crouching
    );
    assert_eq!(state.logistics().player().unwrap().position(), player);
    assert!(!player_sees_resident_body(
        &registries,
        &state,
        player,
        [resident, head]
    ));

    set_generated_block_for_test(registries.world(), &mut state, overhang, BlockStateId::AIR);
    let eye = crate::world::player_eye_voxel(&registries, &state).unwrap();
    assert_eq!(eye, VoxelCoord::new(0, 1, 0));
    assert_eq!(state.world().block_at(overhang), Some(BlockStateId::AIR));
    assert!(
        crate::world::has_clear_two_step_sightline(&registries, &state, eye, head),
        "the actual high corridor must be transparent after removing the roof block"
    );
    assert!(
        crate::world::player_can_see_from_eye(&registries, &state, player, head),
        "the cleared head must be illuminated"
    );
    assert!(player_sees_resident_body(
        &registries,
        &state,
        player,
        [resident, head]
    ));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
