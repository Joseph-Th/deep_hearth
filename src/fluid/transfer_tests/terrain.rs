//! A downward pour is not permission to pass through unknown or solid terrain.

use super::*;
use crate::content::BLOCK_STATE_STONE;
use crate::fluid::{player_fluid_transfer_options, validate_gravity_feed_fluid_to_lower_store};
use crate::logistics::{FluidStorePlacementCommitError, FluidStorePlacementError};
use crate::world::{
    BlockStateId, ChunkCoord, ChunkGenerationError, WORLD_MIN_Y, WorldGenerationSeed,
    set_generated_block_for_test, validate_generate_chunk,
};

const SEED: WorldGenerationSeed = WorldGenerationSeed::new(0xF17D_F10D_2026);

#[test]
fn generated_world_bottom_cannot_gravity_feed_into_unknown_vertical_space() {
    let registries = build_registries();
    let mut state = AppState::new();
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
    let source_position = VoxelCoord::new(0, WORLD_MIN_Y, 0);
    let destination_position = VoxelCoord::new(0, WORLD_MIN_Y - 1, 0);
    for y in WORLD_MIN_Y..=WORLD_MIN_Y + 2 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(0, y, 0),
            BlockStateId::AIR,
        );
    }
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
    assert_eq!(
        state.world().block_at(source_position),
        Some(BlockStateId::AIR)
    );
    assert_eq!(state.world().block_at(destination_position), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let options =
        player_fluid_transfer_options(&registries, &state, Volume::from_microliters(10_000))
            .unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].source(), source);
    assert_eq!(options[0].destination(), destination);
    assert_eq!(
        options[0].blocker(),
        Some(&FluidTransferError::UnknownGravityTerrain {
            destination_position
        })
    );
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
        Some(FluidTransferError::UnknownGravityTerrain {
            destination_position
        }),
    );
    assert_eq!(state, before);
}

#[test]
fn fluid_bootstrap_rejects_both_living_trunk_cells_and_world_change_before_commit() {
    let registries = build_registries();
    let seed = WorldGenerationSeed::new(41);
    let mut state = AppState::new();
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
    let root = state
        .world()
        .trees()
        .next()
        .expect("seed must grow a tree")
        .root();
    let upper = VoxelCoord::new(root.x(), root.y() + 1, root.z());
    let air = VoxelCoord::new(root.x(), root.y() + 2, root.z());
    let store = add_fluid_store(&mut state, Volume::from_microliters(1_000_000)).unwrap();
    for position in [root, upper] {
        assert_eq!(state.world().block_at(position), Some(BlockStateId::AIR));
        let before = state.clone();
        assert_eq!(
            validate_place_fluid_store(&state, store, position).err(),
            Some(FluidStorePlacementError::StandingTreeOccupied { position }),
        );
        assert_eq!(state, before);
    }
    assert_eq!(state.world().block_at(air), Some(BlockStateId::AIR));
    validate_place_fluid_store(&state, store, air)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // A pending placement in ungenerated space cannot silently survive the
    // chunk transaction that materializes a living trunk at its destination.
    let mut ungenerated = AppState::new();
    let unlocated = add_fluid_store(&mut ungenerated, Volume::from_microliters(1)).unwrap();
    let token = validate_place_fluid_store(&ungenerated, unlocated, root).unwrap();
    let expected = ungenerated.world().revision();
    validate_generate_chunk(
        &registries,
        &ungenerated,
        seed,
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut ungenerated)
    .unwrap();
    let actual = ungenerated.world().revision();
    let before = ungenerated.clone();
    assert_eq!(
        token.commit(&mut ungenerated),
        Err(FluidStorePlacementCommitError::StaleWorldRevision { expected, actual }),
    );
    assert_eq!(ungenerated, before);
    assert_eq!(
        validate_place_fluid_store(&ungenerated, unlocated, root).err(),
        Some(FluidStorePlacementError::StandingTreeOccupied { position: root }),
    );
    assert_eq!(validate_loaded_state(&registries, &ungenerated), Ok(()));

    // The reverse order is symmetric: a previously located vessel prevents
    // generating the seeded trunk through its existing physical custody.
    let mut occupied = AppState::new();
    let vessel = add_fluid_store(&mut occupied, Volume::from_microliters(1)).unwrap();
    validate_place_fluid_store(&occupied, vessel, root)
        .unwrap()
        .commit(&mut occupied)
        .unwrap();
    let before = occupied.clone();
    assert_eq!(
        validate_generate_chunk(
            &registries,
            &occupied,
            seed,
            ChunkCoord::new(0, 0),
            BLOCK_STATE_STONE,
        )
        .err(),
        Some(ChunkGenerationError::TreePlacementObstructed { voxel: root }),
    );
    assert_eq!(occupied, before);
    assert_eq!(validate_loaded_state(&registries, &occupied), Ok(()));
}

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
fn distant_terrain_change_stales_gravity_commit_without_blocking_ungenerated_local_pour() {
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
    assert!(
        fixture
            .state
            .world()
            .get_chunk(ChunkCoord::containing(VoxelCoord::new(0, -1, 0)))
            .is_none()
    );
    let before_fluid = calculate_fluid_volume_accounting(&fixture.state).unwrap();
    let transfer = validate_gravity_feed_fluid_to_lower_store(
        &fixture.registries,
        &fixture.state,
        fixture.source,
        fixture.destination,
        Volume::from_microliters(1),
    )
    .expect("distant generation cannot block a local ungenerated gravity pour");
    let _ = transfer.commit(&mut fixture.state).unwrap();
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.source)
            .unwrap()
            .stored_volume(),
        Volume::from_microliters(999_999),
    );
    assert_eq!(
        fixture
            .state
            .fluid()
            .get_store(fixture.destination)
            .unwrap()
            .stored_volume(),
        Volume::from_microliters(1),
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&fixture.state).unwrap(),
        before_fluid
    );
    assert_eq!(
        validate_loaded_state(&fixture.registries, &fixture.state),
        Ok(())
    );
    let saved =
        serde_json::to_vec(&SaveEnvelope::new(&fixture.registries, &fixture.state)).unwrap();
    let loaded = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&fixture.registries)
        .unwrap();
    assert_eq!(loaded, fixture.state);
}
