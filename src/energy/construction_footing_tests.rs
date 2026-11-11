//! Finite stationary energy stores must not be assembled in a falling actor's voxel.

use super::*;
use crate::content::{
    BLOCK_STATE_STONE, ENERGY_STONE_FLYWHEEL_DRIVE, FORM_FLYWHEEL, FORM_HANDLE, MATERIAL_STONE,
    MATERIAL_WOOD, build_registries,
};
use crate::core::state::validate_loaded_state;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::player_body_is_grounded;
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::spatial::VoxelCoord;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn falling_player_cannot_assemble_stationary_power_store_until_landing() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xE0E0_5EED),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let initial = VoxelCoord::new(5, floor + 3, 5);
    let carried = validate_admit_player(&registries, &state, initial)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .err(),
        Some(EnergyStoreAssemblyError::PlayerAirborne),
        "an assembled flywheel would otherwise become a permanent midair machine"
    );
    assert_eq!(state, before);

    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..100 {
        if player_body_is_grounded(&registries, &state) {
            break;
        }
        let live = advance_control_tick(&registries, &mut state).unwrap();
        let loaded = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(live, loaded);
        assert_eq!(state, replay);
    }
    assert!(player_body_is_grounded(&registries, &state));
    for actor in [&mut state, &mut replay] {
        let store = validate_assemble_energy_store(
            &registries,
            actor,
            ENERGY_STONE_FLYWHEEL_DRIVE,
            carried,
        )
        .unwrap()
        .commit(actor)
        .unwrap();
        assert_eq!(
            actor.logistics().energy_store_position(store),
            Some(VoxelCoord::new(5, floor + 1, 5))
        );
        assert_eq!(
            calculate_matter_accounting(actor).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, actor), Ok(()));
    }
    assert_eq!(state, replay);
}

#[test]
fn validated_power_store_assembly_rejects_removed_footing_before_commit() {
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xE0E0_5EEE),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(5, floor + 1, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    assert!(player_body_is_grounded(&registries, &state));
    let assembly =
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .unwrap();
    let expected_world_revision = state.world().revision();

    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, floor, 5),
        BlockStateId::AIR,
    );
    assert!(!player_body_is_grounded(&registries, &state));
    let before = state.clone();
    assert_eq!(
        assembly.commit(&mut state).err(),
        Some(EnergyStoreAssemblyCommitError::StaleWorld {
            expected: expected_world_revision,
            actual: before.world().revision(),
        }),
    );
    assert_eq!(
        state, before,
        "rejected construction must preserve matter custody"
    );
}

#[test]
fn power_store_assembly_cannot_use_moving_resident_as_permanent_footing() {
    use crate::logistics::player_body_has_fixed_floor;
    use crate::world::{BlockStateId, set_generated_block_for_test, temple_warden_route_at};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0x0054_454D_504C_4553),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let post = temple_warden_route_at(&registries, &state, VoxelCoord::new(8, -19, 8))
        .expect("seeded temple warden has a physical post")[0];
    for y in post.y() + 2..=post.y() + 4 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(post.x(), y, post.z()),
            BlockStateId::AIR,
        );
    }
    let carried = validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(post.x(), post.y() + 2, post.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap()
    .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_FLYWHEEL),
            Mass::from_milligrams(900_000),
        ),
        (
            CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
            Mass::from_milligrams(200_000),
        ),
    ] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            commodity,
            mass,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    assert!(player_body_is_grounded(&registries, &state));
    assert!(!player_body_has_fixed_floor(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_assemble_energy_store(&registries, &state, ENERGY_STONE_FLYWHEEL_DRIVE, carried)
            .err(),
        Some(EnergyStoreAssemblyError::PlayerAirborne),
    );
    assert_eq!(state, before);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
}
