//! Contract tests for compact block-world storage, generation, persistence, and accounting.

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, StateValidationError};
use crate::logistics::{
    PlayerControlInput, add_traversable_edge_for_test, validate_start_player_step,
};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{PlayerAdmissionCommitError, validate_admit_player};
use crate::simulation::advance_control_tick_with_input;
use crate::spatial::{CubeFace, VIEW_DIRECTION_PARTS_PER_MILLION, ViewDirection, VoxelCoord};

use super::*;

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0xD33F_4845_4152_5448);

fn generate(state: &mut AppState, coord: ChunkCoord) {
    let registries = build_registries();
    validate_generate_chunk(&registries, state, SEED, coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("terrain generation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("terrain generation commit failed: {error}"));
}

fn generated_targeting_player_at(x: i64, z: i64) -> (crate::registry::Registries, AppState, i64) {
    let registries = build_registries();
    let mut state = AppState::new();
    let coord = ChunkCoord::containing(VoxelCoord::new(x, 0, z));
    validate_generate_chunk(&registries, &state, SEED, coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("targeting terrain generation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("targeting terrain commit failed: {error}"));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(x, *y, z)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("targeting terrain column has no solid surface"));
    let floor_y = surface_y + 1;
    validate_admit_player(&registries, &state, VoxelCoord::new(x, floor_y, z))
        .unwrap_or_else(|error| panic!("targeting player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("targeting player commit failed: {error}"));
    (registries, state, floor_y)
}

fn positive_x_view() -> ViewDirection {
    ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0)
        .unwrap_or_else(|error| panic!("positive-X view fixture failed: {error}"))
}

#[test]
fn player_block_targeting_requires_complete_player_admission() {
    let registries = build_registries();
    assert_eq!(
        raycast_player_block(&registries, &AppState::new(), positive_x_view()),
        Err(PlayerBlockTargetError::PlayerNotAdmitted)
    );
}

#[test]
fn player_block_targeting_hits_first_full_cube_and_reports_entry_face() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for x in 1..=4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_voxel_y, 0),
            BlockStateId::AIR,
        );
    }
    let target_voxel = VoxelCoord::new(3, eye_voxel_y, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target_voxel,
        BLOCK_STATE_STONE,
    );

    let target = raycast_player_block(&registries, &state, positive_x_view())
        .unwrap_or_else(|error| panic!("player block raycast failed: {error}"))
        .unwrap_or_else(|| panic!("player block raycast missed authored target"));
    assert_eq!(target.voxel(), target_voxel);
    assert_eq!(target.state(), BLOCK_STATE_STONE);
    assert_eq!(target.face(), CubeFace::West);
}

#[test]
fn player_block_targeting_includes_exact_reach_boundary_and_excludes_beyond_it() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for x in 1..=6 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(x, eye_voxel_y, 0),
            BlockStateId::AIR,
        );
    }
    let exact_reach = VoxelCoord::new(5, eye_voxel_y, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        exact_reach,
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("exact-reach raycast failed: {error}"))
            .map(PlayerBlockTarget::voxel),
        Some(exact_reach)
    );

    set_generated_block_for_test(
        registries.world(),
        &mut state,
        exact_reach,
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, eye_voxel_y, 0),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view())
            .unwrap_or_else(|error| panic!("beyond-reach raycast failed: {error}")),
        None
    );
}

#[test]
fn player_block_targeting_fails_closed_at_missing_chunk_authority() {
    let (registries, state, floor_y) = generated_targeting_player_at(15, 0);
    let unavailable = VoxelCoord::new(16, floor_y + 1, 0);
    assert_eq!(
        raycast_player_block(&registries, &state, positive_x_view()),
        Err(PlayerBlockTargetError::WorldAuthorityUnavailable { voxel: unavailable })
    );
}

#[test]
fn player_block_targeting_uses_deterministic_axis_precedence_at_corner_entry() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    let eye_voxel_y = floor_y + 1;
    for voxel in [
        VoxelCoord::new(1, eye_voxel_y, 0),
        VoxelCoord::new(0, eye_voxel_y, 1),
        VoxelCoord::new(1, eye_voxel_y, 1),
    ] {
        set_generated_block_for_test(registries.world(), &mut state, voxel, BlockStateId::AIR);
    }
    let diagonal_target = VoxelCoord::new(1, eye_voxel_y, 1);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        diagonal_target,
        BLOCK_STATE_STONE,
    );
    let direction = ViewDirection::new(707_107, 0, 707_107)
        .unwrap_or_else(|error| panic!("diagonal view fixture failed: {error}"));
    let target = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("diagonal player block raycast failed: {error}"))
        .unwrap_or_else(|| panic!("diagonal player block raycast missed target"));
    assert_eq!(target.voxel(), diagonal_target);
    assert_eq!(target.face(), CubeFace::West);
}

#[test]
fn player_block_targeting_derives_eye_origin_from_persisted_stance() {
    let (registries, mut state, floor_y) = generated_targeting_player_at(0, 0);
    for x in 1..=4 {
        for y in floor_y + 1..=floor_y + 4 {
            set_generated_block_for_test(
                registries.world(),
                &mut state,
                VoxelCoord::new(x, y, 0),
                BlockStateId::AIR,
            );
        }
    }
    let target_voxel = VoxelCoord::new(3, floor_y + 3, 0);
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        target_voxel,
        BLOCK_STATE_STONE,
    );
    let direction = ViewDirection::new(866_025, 500_000, 0)
        .unwrap_or_else(|error| panic!("stance-sensitive view fixture failed: {error}"));

    let standing = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("standing stance raycast failed: {error}"))
        .unwrap_or_else(|| panic!("standing stance raycast missed target"));
    assert_eq!(standing.voxel(), target_voxel);
    assert_eq!(standing.face(), CubeFace::West);

    let _ = advance_control_tick_with_input(
        &registries,
        &mut state,
        PlayerControlInput::NEUTRAL.with_crouch(true),
    )
    .unwrap_or_else(|error| panic!("crouching stance control tick failed: {error}"));
    let crouching = raycast_player_block(&registries, &state, direction)
        .unwrap_or_else(|error| panic!("crouching stance raycast failed: {error}"))
        .unwrap_or_else(|| panic!("crouching stance raycast missed target"));
    assert_eq!(crouching.voxel(), target_voxel);
    assert_eq!(crouching.face(), CubeFace::Bottom);
}

#[test]
fn terrain_generation_persists_one_canonical_profile_and_rejects_seed_changes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first = ChunkCoord::new(0, 0);
    generate(&mut state, first);
    let profile = WorldGenerationProfile::new(SEED, BLOCK_STATE_STONE);
    assert_eq!(state.world().generation_profile(), Some(profile));

    let requested = WorldGenerationProfile::new(
        WorldGenerationSeed::new(SEED.value().wrapping_add(1)),
        BLOCK_STATE_STONE,
    );
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            requested.seed(),
            ChunkCoord::new(1, 0),
            requested.solid_state(),
        )
        .err(),
        Some(ChunkGenerationError::GenerationProfileConflict)
    );
    assert_eq!(state.world().chunks().len(), 1);
}

#[test]
fn player_chunk_demand_requires_profile_then_generates_missing_neighborhood_once() {
    let registries = build_registries();
    let mut missing_profile = AppState::new();
    validate_admit_player(&registries, &missing_profile, VoxelCoord::new(0, 100, 0))
        .unwrap_or_else(|error| panic!("demand missing-profile admission failed: {error}"))
        .commit(&mut missing_profile)
        .unwrap_or_else(|error| panic!("demand missing-profile commit failed: {error}"));
    assert_eq!(
        validate_generate_player_neighborhood(&registries, &missing_profile).err(),
        Some(ChunkGenerationError::GenerationProfileMissing)
    );

    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("demand terrain column has no solid surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("demand player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("demand player commit failed: {error}"));
    let revision_before = state.world().revision();
    let matter_before = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("demand matter accounting before failed: {error}"))
        .world();

    let demand = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("player chunk demand failed: {error}"))
        .unwrap_or_else(|| panic!("player chunk demand found no missing neighborhood"));
    assert_eq!(demand.chunks().len(), 8);
    let generated = demand
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("player chunk demand commit failed: {error}"));
    assert_eq!(generated.len(), 8);
    assert_eq!(state.world().chunks().len(), 9);
    assert_eq!(state.world().revision(), revision_before + 1);
    assert_eq!(
        state.world().generation_profile(),
        Some(WorldGenerationProfile::new(SEED, BLOCK_STATE_STONE))
    );
    let matter_after = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("demand matter accounting after failed: {error}"))
        .world();
    assert!(matter_after > matter_before);
    assert!(
        validate_generate_player_neighborhood(&registries, &state)
            .unwrap_or_else(|error| panic!("idempotent player chunk demand failed: {error}"))
            .is_none()
    );
}

#[test]
fn player_chunk_demand_commit_is_world_revision_bound() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let surface_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|y| state.world().block_at(VoxelCoord::new(0, *y, 0)) == Some(BLOCK_STATE_STONE))
        .unwrap_or_else(|| panic!("stale-demand terrain column has no solid surface"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, surface_y + 1, 0))
        .unwrap_or_else(|error| panic!("stale-demand player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-demand player commit failed: {error}"));
    let demand = validate_generate_player_neighborhood(&registries, &state)
        .unwrap_or_else(|error| panic!("stale-demand validation failed: {error}"))
        .unwrap_or_else(|| panic!("stale-demand fixture has no missing chunks"));
    let expected = state.world().revision();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(2, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("stale-demand intervening chunk failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale-demand intervening commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        demand.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleWorldRevision {
            expected,
            actual: expected + 1,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn chunk_coordinates_use_euclidean_partitioning_across_zero() {
    for (x, expected) in [(-17, -2), (-16, -1), (-1, -1), (0, 0), (15, 0), (16, 1)] {
        assert_eq!(
            ChunkCoord::containing(VoxelCoord::new(x, 0, x)),
            ChunkCoord::new(expected, expected)
        );
    }
}

#[test]
fn trusted_load_rejects_generated_chunks_without_generation_profile() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world profile tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["generation_profile"] = serde_json::Value::Null;
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world profile tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::ChunksWithoutGenerationProfile
        )))
    );
}

#[test]
fn trusted_load_rejects_air_generation_profile() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("air profile tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["generation_profile"]["solid_state"] =
        serde_json::json!(BlockStateId::AIR.value());
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("air profile tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::AirGenerationSolidState
        )))
    );
}

#[test]
fn terrain_generation_is_deterministic_compact_and_seed_sensitive() {
    let registries = build_registries();
    let coord = ChunkCoord::new(-2, 3);
    let first = validate_generate_chunk(
        &registries,
        &AppState::new(),
        SEED,
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("first terrain generation failed: {error}"));
    let replay = validate_generate_chunk(
        &registries,
        &AppState::new(),
        SEED,
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("replay terrain generation failed: {error}"));
    assert_eq!(first.chunk(), replay.chunk());

    let different = validate_generate_chunk(
        &registries,
        &AppState::new(),
        WorldGenerationSeed::new(SEED.value() + 1),
        coord,
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("alternate terrain generation failed: {error}"));
    assert_ne!(first.chunk(), different.chunk());

    let homogeneous = first
        .chunk()
        .sections()
        .find(|section| section.non_air_count() == SECTION_CELL_COUNT as u16)
        .unwrap_or_else(|| panic!("generated terrain lost its homogeneous deep section"));
    assert_eq!(homogeneous.palette(), &[BLOCK_STATE_STONE]);
    assert_eq!(homogeneous.packed_word_count(), 0);
}

#[test]
fn generated_chunk_distinguishes_air_from_absent_world_truth() {
    let mut state = AppState::new();
    let coord = ChunkCoord::new(0, 0);
    generate(&mut state, coord);

    assert_eq!(
        state.world().block_at(VoxelCoord::new(0, WORLD_MIN_Y, 0)),
        Some(BLOCK_STATE_STONE)
    );
    assert_eq!(
        state.world().block_at(VoxelCoord::new(0, 128, 0)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(CHUNK_SIDE as i64, 0, 0)),
        None
    );
    assert_eq!(
        state
            .world()
            .block_at(VoxelCoord::new(0, WORLD_MAX_Y_EXCLUSIVE, 0)),
        None
    );
}

#[test]
fn generated_terrain_is_included_in_exact_matter_accounting() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));

    let occupied_cells = state
        .world()
        .chunks()
        .flat_map(|chunk| chunk.sections())
        .map(|section| u128::from(section.non_air_count()))
        .sum::<u128>();
    let stone_mass = registries
        .world()
        .represented_mass_for_state(BLOCK_STATE_STONE)
        .unwrap_or_else(|| panic!("stone block state lost represented mass"));
    let expected = occupied_cells * u128::from(stone_mass.milligrams());
    let accounting = calculate_matter_accounting(&state)
        .unwrap_or_else(|error| panic!("world matter accounting failed: {error}"));

    assert_eq!(accounting.world().milligrams(), expected);
    assert_eq!(accounting.total().milligrams(), expected);
}

#[test]
fn terrain_generation_rejects_unrepresentable_chunk_coordinates() {
    let registries = build_registries();
    let state = AppState::new();
    let chunk = ChunkCoord::new(i64::MAX, 0);

    assert_eq!(
        validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE,).err(),
        Some(ChunkGenerationError::ChunkCoordinateOverflow { chunk })
    );
}

#[test]
fn chunk_generation_commit_is_revision_bound_and_does_not_overwrite_world_truth() {
    let registries = build_registries();
    let mut state = AppState::new();
    let first_coord = ChunkCoord::new(0, 0);
    let second_coord = ChunkCoord::new(1, 0);
    let first = validate_generate_chunk(&registries, &state, SEED, first_coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("first chunk validation failed: {error}"));
    let stale = validate_generate_chunk(&registries, &state, SEED, second_coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale chunk validation failed: {error}"));

    assert_eq!(first.commit(&mut state), Ok(first_coord));
    let before = state.clone();
    assert_eq!(
        stale.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleWorldRevision {
            expected: 0,
            actual: 1,
        })
    );
    assert_eq!(state, before);
    assert_eq!(
        validate_generate_chunk(&registries, &state, SEED, first_coord, BLOCK_STATE_STONE,).err(),
        Some(ChunkGenerationError::AlreadyGenerated { chunk: first_coord })
    );
}

#[test]
fn terrain_generation_rejects_player_body_overlap_without_world_mutation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    validate_admit_player(&registries, &state, player_voxel)
        .unwrap_or_else(|error| panic!("body-collision admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("body-collision admission commit failed: {error}"));
    let before = state.clone();

    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::PlayerBodyCollision {
            voxel: player_voxel,
        })
    );
    assert_eq!(state, before);
}

#[test]
fn validated_player_admission_cannot_cross_world_generation() {
    let registries = build_registries();
    let mut state = AppState::new();
    let player_voxel = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let admission = validate_admit_player(&registries, &state, player_voxel)
        .unwrap_or_else(|error| panic!("stale-world admission validation failed: {error}"));
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap_or_else(|error| panic!("stale-world terrain validation failed: {error}"))
    .commit(&mut state)
    .unwrap_or_else(|error| panic!("stale-world terrain commit failed: {error}"));

    assert_eq!(
        admission.commit(&mut state),
        Err(PlayerAdmissionCommitError::StaleWorldRevision {
            expected: 0,
            actual: 1,
        })
    );
}

#[test]
fn validated_chunk_cannot_cross_player_or_player_work_changes() {
    let registries = build_registries();
    let mut state = AppState::new();
    let chunk = ChunkCoord::new(0, 0);
    let first = validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale-logistics terrain validation failed: {error}"));
    validate_admit_player(&registries, &state, VoxelCoord::new(0, 100, 0))
        .unwrap_or_else(|error| panic!("stale-logistics player admission failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-logistics player commit failed: {error}"));
    let actual_logistics_revision = state.logistics().revision();
    let before = state.clone();
    assert_eq!(
        first.commit(&mut state),
        Err(ChunkGenerationCommitError::StaleLogisticsRevision {
            expected: 0,
            actual: actual_logistics_revision,
        })
    );
    assert_eq!(state, before);

    let origin = VoxelCoord::new(0, 100, 0);
    let destination = VoxelCoord::new(1, 100, 0);
    add_traversable_edge_for_test(&mut state, origin, destination);
    let validated = validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("stale-work terrain validation failed: {error}"));
    let expected_player_work_revision = state.player_work().revision();
    validate_start_player_step(&registries, &state, destination)
        .unwrap_or_else(|error| panic!("stale-work legacy step validation failed: {error}"))
        .commit(&mut state)
        .unwrap_or_else(|error| panic!("stale-work legacy step commit failed: {error}"));
    let actual_player_work_revision = state.player_work().revision();
    assert_eq!(
        validated.commit(&mut state),
        Err(ChunkGenerationCommitError::StalePlayerWorkRevision {
            expected: expected_player_work_revision,
            actual: actual_player_work_revision,
        })
    );
    assert_eq!(
        validate_generate_chunk(&registries, &state, SEED, chunk, BLOCK_STATE_STONE).err(),
        Some(ChunkGenerationError::PlayerStepInProgress {
            origin,
            destination,
        })
    );
}

#[test]
fn generated_world_round_trip_preserves_compact_cells() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(-1, 2));
    generate(&mut state, ChunkCoord::new(0, 2));

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world persistence serialization failed: {error}"));
    let decoded: LoadedSaveEnvelope = serde_json::from_slice(&encoded)
        .unwrap_or_else(|error| panic!("world persistence decode failed: {error}"));
    let loaded = decoded
        .into_state(&registries)
        .unwrap_or_else(|error| panic!("world persistence trusted load failed: {error}"));

    assert_eq!(loaded, state);
}

#[test]
fn trusted_load_rejects_duplicate_section_palette_state() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world palette tamper serialization failed: {error}"));
    let sections = encoded["state"]["systems"]["world"]["chunks"][0]["sections"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("serialized world sections are not an array"));
    let section = sections
        .iter_mut()
        .find(|section| {
            section["palette"]
                .as_array()
                .is_some_and(|palette| palette.len() > 1)
        })
        .unwrap_or_else(|| panic!("generated terrain has no mixed surface section"));
    let duplicate = section["palette"][0].clone();
    section["palette"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("serialized world palette is not an array"))
        .push(duplicate.clone());
    let duplicated_state: BlockStateId = serde_json::from_value(duplicate)
        .unwrap_or_else(|error| panic!("serialized palette state failed to decode: {error}"));
    let section_y = section["y"]
        .as_i64()
        .unwrap_or_else(|| panic!("serialized world section y is not an integer"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world palette tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::DuplicateSectionPaletteState {
                section_y,
                state: duplicated_state,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_tampered_represented_mass() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let actual_mass = state
        .world()
        .get_chunk(ChunkCoord::new(0, 0))
        .and_then(|chunk| chunk.sections().next())
        .map(WorldSection::represented_mass)
        .unwrap_or_else(|| panic!("generated world lost its first terrain section"));
    let section_y = state
        .world()
        .get_chunk(ChunkCoord::new(0, 0))
        .and_then(|chunk| chunk.sections().next())
        .map(WorldSection::y)
        .unwrap_or_else(|| panic!("generated world lost its first terrain section"));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world mass tamper serialization failed: {error}"));
    encoded["state"]["systems"]["world"]["chunks"][0]["sections"][0]["represented_mass"] =
        serde_json::json!(0);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world mass tamper decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::SectionMassMismatch {
                chunk: ChunkCoord::new(0, 0),
                section_y,
                stored: crate::core::quantity::Mass::ZERO,
                actual: actual_mass,
            }
        )))
    );
}

#[test]
fn trusted_load_rejects_unknown_block_state_reference() {
    let registries = build_registries();
    let mut state = AppState::new();
    generate(&mut state, ChunkCoord::new(0, 0));
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state))
        .unwrap_or_else(|error| panic!("world unknown-state serialization failed: {error}"));
    let section = &mut encoded["state"]["systems"]["world"]["chunks"][0]["sections"][0];
    let unknown = BlockStateId::new(u16::MAX);
    section["palette"][0] = serde_json::json!(unknown.value());
    let section_y = section["y"]
        .as_i64()
        .unwrap_or_else(|| panic!("serialized world section y is not an integer"));
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded)
        .unwrap_or_else(|error| panic!("world unknown-state decode failed: {error}"));

    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::World(
            WorldValidationError::UnknownSectionPaletteState {
                chunk: ChunkCoord::new(0, 0),
                section_y,
                state: unknown,
            }
        )))
    );
}
