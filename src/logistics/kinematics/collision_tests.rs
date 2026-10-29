//! Occupied-cell queries and body-collision traversal share the same exact geometry.

use super::*;
use crate::content::build_registries;
use crate::core::state::AppState;
use crate::logistics::PlayerStance;
use crate::player::validate_admit_player;
use crate::spatial::{HorizontalVelocity, VerticalVelocity, WorldPoint};

#[test]
fn distant_terrain_does_not_activate_continuous_body_collision_in_the_opening() {
    use crate::content::BLOCK_STATE_STONE;
    use crate::spatial::VoxelBounds;
    use crate::world::{ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

    let registries = build_registries();
    let mut state = AppState::new();
    let foot = VoxelCoord::new(0, 100, 0);
    validate_admit_player(&registries, &state, foot)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let body_bounds = VoxelBounds::new(foot, VoxelCoord::new(1, 102, 1)).unwrap();
    let overlap =
        |state: &AppState| first_player_body_overlap_with_bounds(&registries, state, body_bounds);
    assert_eq!(overlap(&state), None);

    let seed = WorldGenerationSeed::new(41);
    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::new(20, 20),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(overlap(&state), None);

    validate_generate_chunk(
        &registries,
        &state,
        seed,
        ChunkCoord::containing(foot),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(overlap(&state), Some(foot));
}

#[test]
fn body_collision_and_actor_occupancy_agree_across_voxel_faces_and_stances() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let locomotion = registries.survival().physiology().locomotion();

    for (stance, foot) in [
        (
            PlayerStance::Standing,
            WorldPoint::new(-200_000_000, -300_000_000, 850_000_000),
        ),
        (
            PlayerStance::Crouching,
            WorldPoint::new(750_000_000, 600_000_000, -100_000_000),
        ),
    ] {
        let player = state.logistics().player().copied().unwrap();
        let before = player.body();
        let next = before.with_motion(
            foot,
            HorizontalVelocity::ZERO,
            VerticalVelocity::ZERO,
            stance,
        );
        let revision = state.logistics().revision();
        state.logistics_state_mut().apply_player_body_motion(
            revision,
            revision + 1,
            before,
            next,
            player.locomotion_exertion(),
            player.pending_fall_vitality_loss_ppm(),
        );
        for y in -2..=4 {
            for z in -2..=2 {
                for x in -2..=2 {
                    let voxel = VoxelCoord::new(x, y, z);
                    let collision =
                        first_player_body_collision_where(next, locomotion, |cell| cell == voxel)
                            .is_some();
                    assert_eq!(
                        player_body_occupies_voxel(&registries, &state, voxel),
                        collision,
                        "body-cell disagreement at {voxel:?} in stance {stance:?}"
                    );
                }
            }
        }
    }
}
