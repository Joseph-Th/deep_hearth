//! A downward pour is not permission to pass through unknown or solid terrain.

use super::*;
use crate::content::BLOCK_STATE_STONE;
use crate::fluid::validate_gravity_feed_fluid_to_lower_store;
use crate::world::{BlockStateId, ChunkCoord, WorldGenerationSeed, validate_generate_chunk};

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0xF17D_F10D_2026);

#[test]
fn known_generated_stone_blocks_gravity_delivery_without_consuming_water() {
    let registries = build_registries();
    let mut state = AppState::new();
    let _ = validate_generate_chunk(
        &registries,
        &state,
        SEED,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let (source_position, destination_position) = (-20..20)
        .rev()
        .find_map(|y| {
            let ground = VoxelCoord::new(0, y, 0);
            let feet = VoxelCoord::new(0, y + 1, 0);
            let head = VoxelCoord::new(0, y + 2, 0);
            (state.world().block_at(ground) == Some(BLOCK_STATE_STONE)
                && state.world().block_at(feet) == Some(BlockStateId::AIR)
                && state.world().block_at(head) == Some(BlockStateId::AIR))
            .then_some((feet, ground))
        })
        .expect("seeded column should have a stone floor with standing clearance");
    let source = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        Volume::from_microliters(1_000_000),
        FLUID_WATER,
        Volume::from_microliters(500_000),
        ROOM_TEMPERATURE,
    )
    .unwrap();
    let destination = add_fluid_store(&mut state, Volume::from_microliters(1_000_000)).unwrap();
    for (store, position) in [
        (source, source_position),
        (destination, destination_position),
    ] {
        validate_place_fluid_store(&state, store, position)
            .unwrap()
            .commit(&mut state)
            .unwrap();
    }
    validate_admit_player(&registries, &state, source_position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let before = state.clone();
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &registries,
            &state,
            source,
            destination,
            Volume::from_microliters(10_000),
        )
        .err(),
        Some(FluidTransferError::BlockedGravityDestination {
            destination_position
        }),
    );
    assert_eq!(state, before);
}

#[test]
fn generated_world_must_know_the_downhill_destination_and_commit_rechecks_world_revision() {
    let mut fixture = fixture_at(None, VoxelCoord::new(0, -1, 0));
    let token = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    )
    .unwrap();
    let expected = fixture.state.world().revision();
    let _ = validate_generate_chunk(
        &fixture.registries,
        &fixture.state,
        SEED,
        ChunkCoord::new(10, 10),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut fixture.state)
    .unwrap();
    let actual = fixture.state.world().revision();
    let before = fixture.state.clone();
    assert_eq!(
        token.commit(&mut fixture.state),
        Err(FluidTransferCommitError::StaleWorldRevision { expected, actual }),
    );
    assert_eq!(fixture.state, before);
    assert_eq!(
        validate_gravity_feed_fluid_to_lower_store(
            &fixture.registries,
            &fixture.state,
            fixture.source,
            fixture.destination,
            Volume::from_microliters(1),
        )
        .err(),
        Some(FluidTransferError::UnknownGravityTerrain {
            destination_position: VoxelCoord::new(0, -1, 0),
        }),
    );
}
