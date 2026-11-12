//! Player-created ground stockpiles require actual generated-world footing.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::{MaterialLotSelection, deposit_lot_for_test};
use crate::logistics::{player_body_is_grounded, validate_drop_to_ground};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn falling_player_cannot_allocate_a_floating_ground_cache_and_can_store_after_landing() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xCACE_47A1),
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
    let high = VoxelCoord::new(5, floor + 4, 5);
    let carried = validate_admit_player(&registries, &state, high)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(100_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_allocate_player_ground_stockpile(&registries, &state, mass).err(),
        Some(PlayerGroundStockpileAllocationError::PlayerAirborne),
        "creating a full-capacity loose cache must not pin custody in open air"
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
        let ground = validate_allocate_player_ground_stockpile(&registries, actor, mass)
            .unwrap()
            .commit(actor)
            .unwrap();
        assert_eq!(
            actor.logistics().stationary_stockpile_position(ground),
            Some(VoxelCoord::new(5, floor + 1, 5))
        );
        validate_drop_to_ground(
            &registries,
            actor,
            ground,
            &[MaterialLotSelection::new(lot, mass)],
        )
        .unwrap()
        .commit(actor)
        .unwrap();
        assert_eq!(
            actor
                .inventory()
                .get_stockpile(ground)
                .unwrap()
                .stored_mass(),
            mass
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
fn validated_ground_cache_rejects_removed_footing_before_commit() {
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xCACE_47A2),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    validate_admit_player(&registries, &state, VoxelCoord::new(5, floor_y + 1, 5))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    let allocation = validate_allocate_player_ground_stockpile(
        &registries,
        &state,
        Mass::from_milligrams(100_000),
    )
    .unwrap();
    let expected_world_revision = state.world().revision();

    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, floor_y, 5),
        BlockStateId::AIR,
    );
    assert!(!player_body_is_grounded(&registries, &state));
    let before = state.clone();
    assert_eq!(
        allocation.commit(&mut state).err(),
        Some(GroundStockpileAllocationCommitError::StaleWorldRevision {
            expected: expected_world_revision,
            actual: before.world().revision(),
        }),
    );
    assert_eq!(
        state, before,
        "rejected allocation must preserve owner identities"
    );
}

#[test]
fn ground_cache_cannot_be_filled_after_its_empty_site_loses_footing() {
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xCACE_47A2),
        ChunkCoord::new(0, 0),
        BLOCK_STATE_STONE,
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    let floor_y = (WORLD_MIN_Y..WORLD_MAX_Y_EXCLUSIVE)
        .rev()
        .find(|&y| state.world().block_at(VoxelCoord::new(5, y, 5)) == Some(BLOCK_STATE_STONE))
        .unwrap();
    let position = VoxelCoord::new(5, floor_y + 1, 5);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let mass = Mass::from_milligrams(100_000);
    let lot = deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_STONE, FORM_CHIP),
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let ground = validate_allocate_player_ground_stockpile(&registries, &state, mass)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let selection = [MaterialLotSelection::new(lot, mass)];
    let pending = validate_drop_to_ground(&registries, &state, ground, &selection).unwrap();
    let original_world_revision = state.world().revision();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, floor_y, 5),
        BlockStateId::AIR,
    );
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        pending.commit(&mut state),
        Err(
            crate::logistics::GroundMaterialTransferCommitError::StaleWorldRevision {
                expected: original_world_revision,
                actual: state.world().revision()
            }
        )
    );
    assert_eq!(
        validate_drop_to_ground(&registries, &state, ground, &selection).err(),
        Some(
            crate::logistics::GroundMaterialTransferError::DestinationRequiresFixedFloor {
                stockpile: ground
            }
        )
    );
    assert_eq!(state, before);
    assert_eq!(
        state
            .inventory()
            .get_stockpile(ground)
            .unwrap()
            .stored_mass(),
        Mass::ZERO
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn ground_cache_cannot_rely_on_a_moving_resident_as_footing() {
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
    validate_admit_player(
        &registries,
        &state,
        VoxelCoord::new(post.x(), post.y() + 2, post.z()),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert!(!player_body_has_fixed_floor(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let before = state.clone();
    assert_eq!(
        validate_allocate_player_ground_stockpile(
            &registries,
            &state,
            Mass::from_milligrams(100_000),
        )
        .err(),
        Some(PlayerGroundStockpileAllocationError::PlayerAirborne),
    );
    assert_eq!(state, before);
}
