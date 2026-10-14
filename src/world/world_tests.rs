//! Contract tests for compact block-world storage, generation, persistence, and accounting.

use crate::content::{BLOCK_STATE_STONE, build_registries};
use crate::core::state::{AppState, StateValidationError};
use crate::logistics::{add_traversable_edge_for_test, validate_start_player_step};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadError, LoadedSaveEnvelope, SaveEnvelope};
use crate::player::{PlayerAdmissionCommitError, validate_admit_player};
use crate::spatial::VoxelCoord;

use super::*;

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0xD33F_4845_4152_5448);

fn generate(state: &mut AppState, coord: ChunkCoord) {
    let registries = build_registries();
    validate_generate_chunk(&registries, state, SEED, coord, BLOCK_STATE_STONE)
        .unwrap_or_else(|error| panic!("terrain generation validation failed: {error}"))
        .commit(state)
        .unwrap_or_else(|error| panic!("terrain generation commit failed: {error}"));
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
