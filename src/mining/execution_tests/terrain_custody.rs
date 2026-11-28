//! Mining output cannot create stationary matter without physical custody.

use super::*;
use crate::world::{ChunkGenerationError, WORLD_MIN_Y};

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0x5052_4F43_524F_434B);

#[test]
fn mining_cannot_reserve_output_into_unsupported_generated_air() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let at = VoxelCoord::new(5, 100, 5);
    assert_eq!(state.world().block_at(at), Some(BlockStateId::AIR));
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::DestinationRequiresFixedFloor {
            stockpile: destination,
            position: at
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mining_cannot_reserve_output_into_generated_rock() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let at = VoxelCoord::new(5, WORLD_MIN_Y, 5);
    assert_eq!(state.world().block_at(at), Some(BLOCK_STATE_STONE));
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_known_mining(
            &registries,
            &state,
            MINING_METHOD_HAND_PICK,
            deposit,
            destination,
            pick,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(MiningStartError::DestinationSolidTerrain {
            stockpile: destination,
            position: at
        })
    );
    assert_eq!(state, before);
}

#[test]
fn mining_reservation_prevents_generation_without_footing() {
    let (registries, mut state, deposit, destination, pick) = survival_only_mining_fixture();
    let at = VoxelCoord::new(5, 100, 5);
    validate_place_ground_stockpile(&state, destination, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_known_mining(
        &registries,
        &state,
        MINING_METHOD_HAND_PICK,
        deposit,
        destination,
        pick,
        Mass::from_milligrams(100_000),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(
        state
            .inventory()
            .get_stockpile(destination)
            .unwrap()
            .reserved_inbound(),
        Mass::from_milligrams(100_000)
    );
    let before = state.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &state,
            SEED,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE
        )
        .err(),
        Some(ChunkGenerationError::GroundMatterRequiresFixedFloor { voxel: at })
    );
    assert_eq!(state, before);
}
