//! Generated-world vertical collision, admission, and deterministic continuation contracts.

use super::*;

fn generated_player_at_vertical_world_limit(feet_y: i64, platform: bool) -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, feet_y, 0);
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0A1_DA12),
        ChunkCoord::containing(position),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("vertical-boundary generation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("vertical-boundary generation commit failed: {error}"));
    for y in feet_y..=feet_y + 1 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(0, y, 0),
            BlockStateId::AIR,
        );
    }
    if platform {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(0, feet_y - 1, 0),
            BLOCK_STATE_STONE,
        );
    }
    validate_admit_player(&registries, &state, position)
        .unwrap_or_else(|error| panic!("vertical-boundary admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("vertical-boundary admission commit failed: {error}"));
    (registries, state)
}

#[test]
fn unsupported_world_depth_is_a_solid_control_boundary_with_replay() {
    let (registries, mut state) = generated_player_at_vertical_world_limit(WORLD_MIN_Y, false);
    let minimum = WORLD_MIN_Y * NANOMETERS_PER_VOXEL;
    for _ in 0..4 {
        let _ = advance_control_tick(&registries, &mut state)
            .unwrap_or_else(|error| panic!("world-depth control failed: {error}"));
        let body = state.logistics().player().unwrap().body();
        assert_eq!(body.foot_position().y_nanometers(), minimum);
        assert_eq!(body.vertical_velocity().nanometers_per_second(), 0);
    }
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    let mut loaded = decoded.into_state(&registries).unwrap();
    for _ in 0..4 {
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut loaded).unwrap()
        );
    }
    assert_eq!(loaded, state);
}

#[test]
fn upper_world_limit_blocks_jump_before_body_enters_unrepresented_height() {
    let feet_y = WORLD_MAX_Y_EXCLUSIVE - 2;
    let (registries, mut state) = generated_player_at_vertical_world_limit(feet_y, true);
    let standing_height = i64::try_from(
        registries
            .survival()
            .physiology()
            .locomotion()
            .standing_height()
            .micrometers(),
    )
    .unwrap();
    let maximum_feet = WORLD_MAX_Y_EXCLUSIVE * NANOMETERS_PER_VOXEL - standing_height * 1_000;
    let mut reached_ceiling = false;
    for step in 0..20 {
        let _ = advance_control_tick_with_input(
            &registries,
            &mut state,
            PlayerControlInput::NEUTRAL.with_jump(step == 0),
        )
        .unwrap_or_else(|error| panic!("world-ceiling control failed: {error}"));
        let body = state.logistics().player().unwrap().body();
        assert!(
            body.foot_position().y_nanometers() <= maximum_feet,
            "player body passed the maximum represented world height"
        );
        reached_ceiling |= body.foot_position().y_nanometers() == maximum_feet;
    }
    assert!(
        reached_ceiling,
        "the jump must actually hit the world ceiling"
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn generated_world_rejects_initial_and_loaded_bodies_outside_vertical_truth() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0A1_DA12),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();

    for (feet_y, blocked_y) in [
        (WORLD_MIN_Y - 1, WORLD_MIN_Y - 1),
        (WORLD_MAX_Y_EXCLUSIVE - 1, WORLD_MAX_Y_EXCLUSIVE),
    ] {
        let position = VoxelCoord::new(0, feet_y, 0);
        assert_eq!(
            validate_admit_player(&registries, &state, position).err(),
            Some(crate::player::PlayerAdmissionError::BodyIntersectsSolid {
                voxel: VoxelCoord::new(0, blocked_y, 0),
            })
        );
    }
    assert!(state.logistics().player().is_none());
    assert!(state.survival().player().is_none());

    let mut ungenerated = AppState::new();
    validate_admit_player(
        &registries,
        &ungenerated,
        VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE - 1, 0),
    )
    .unwrap()
    .commit(&mut ungenerated)
    .unwrap();
    let before_generation = ungenerated.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &ungenerated,
            WorldGenerationSeed::new(0xB0A1_DA12),
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(crate::world::ChunkGenerationError::PlayerBodyCollision {
            voxel: VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE, 0),
        })
    );
    assert_eq!(ungenerated, before_generation);

    let (registries, valid) =
        generated_player_at_vertical_world_limit(WORLD_MAX_Y_EXCLUSIVE - 2, true);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &valid)).unwrap();
    encoded["state"]["systems"]["logistics"]["player"]["body"]["foot_position"]["y_nanometers"] =
        serde_json::json!((WORLD_MAX_Y_EXCLUSIVE - 1) * NANOMETERS_PER_VOXEL);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(crate::persistence::LoadError::InvalidState(
            crate::core::state::StateValidationError::Logistics(
                crate::logistics::LogisticsValidationError::PlayerBodyIntersectsSolid {
                    voxel: VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE, 0),
                }
            )
        ))
    );
}
