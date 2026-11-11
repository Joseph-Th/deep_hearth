//! Generated-world hand drops must not leave immobile loose matter in midair.

use super::*;
use crate::content::{BLOCK_STATE_STONE, FORM_CHIP, MATERIAL_STONE, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::{AppState, validate_loaded_state};
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
fn falling_player_cannot_freeze_a_loose_parcel_in_air_but_can_drop_after_landing() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xD0A1_A1F0),
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
    let falling_from = VoxelCoord::new(5, floor + 5, 5);
    let carried = validate_admit_player(&registries, &state, falling_from)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(100_000);
    deposit_lot_for_test(
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
        validate_drop_player_material(&registries, &state, commodity).err(),
        Some(PlayerMaterialDropError::PlayerAirborne),
        "a world drop would otherwise hang at the falling actor's unsupported voxel"
    );
    assert_eq!(state, before);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );

    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..100 {
        if player_body_is_grounded(&registries, &state) {
            break;
        }
        let live = advance_control_tick(&registries, &mut state).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(live, restored);
        assert_eq!(state, replay);
    }
    assert!(player_body_is_grounded(&registries, &state));
    for game in [&mut state, &mut replay] {
        let dropped = validate_drop_player_material(&registries, game, commodity)
            .unwrap()
            .commit(game)
            .unwrap();
        assert_eq!(dropped.dropped_mass(), mass);
        assert_eq!(dropped.position(), VoxelCoord::new(5, floor + 1, 5));
        assert_eq!(
            game.inventory()
                .get_stockpile(carried)
                .unwrap()
                .get_mass(commodity),
            Mass::ZERO
        );
        assert_eq!(
            calculate_matter_accounting(game).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, game), Ok(()));
    }
    assert_eq!(state, replay);
}

#[test]
fn movable_resident_is_not_permanent_footing_for_a_player_drop() {
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
    let perch = VoxelCoord::new(post.x(), post.y() + 2, post.z());
    let carried = validate_admit_player(&registries, &state, perch)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    let mass = Mass::from_milligrams(100_000);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert!(player_body_is_grounded(&registries, &state));
    assert!(!player_body_has_fixed_floor(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let before = state.clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_drop_player_material(&registries, &state, commodity).err(),
        Some(PlayerMaterialDropError::PlayerAirborne),
        "a moving resident must not leave a permanent parcel hanging when it departs"
    );
    assert_eq!(state, before);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn an_adjacent_ledge_under_the_body_cannot_support_a_drop_in_an_open_shaft() {
    use crate::logistics::player_body_has_fixed_floor;
    use crate::spatial::{NANOMETERS_PER_VOXEL, WorldPoint};
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xD0A1_A1F0),
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
    let spawn = VoxelCoord::new(5, floor + 1, 5);
    let carried = validate_admit_player(&registries, &state, spawn)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_CHIP);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(100_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(5, floor, 5),
        BlockStateId::AIR,
    );
    set_generated_block_for_test(
        registries.world(),
        &mut state,
        VoxelCoord::new(6, floor, 5),
        BLOCK_STATE_STONE,
    );
    for y in floor + 1..=floor + 3 {
        set_generated_block_for_test(
            registries.world(),
            &mut state,
            VoxelCoord::new(6, y, 5),
            BlockStateId::AIR,
        );
    }
    let player = state.logistics().player().copied().unwrap();
    let body = player.body();
    let right_edge = 6 * NANOMETERS_PER_VOXEL - 50_000_000;
    let shifted = body.with_motion(
        WorldPoint::new(
            right_edge,
            body.foot_position().y_nanometers(),
            body.foot_position().z_nanometers(),
        ),
        body.horizontal_velocity(),
        body.vertical_velocity(),
        body.stance(),
    );
    let revision = state.logistics().revision();
    state.logistics_state_mut().apply_player_body_motion(
        revision,
        revision + 1,
        body,
        shifted,
        player.locomotion_exertion(),
        player.pending_fall_vitality_loss_ppm(),
    );
    assert_eq!(state.logistics().player().unwrap().position(), spawn);
    assert!(player_body_is_grounded(&registries, &state));
    assert!(player_body_has_fixed_floor(&registries, &state));
    assert_eq!(
        state.world().block_at(VoxelCoord::new(5, floor, 5)),
        Some(BlockStateId::AIR)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let before = state.clone();
    assert_eq!(
        validate_drop_player_material(&registries, &state, commodity).err(),
        Some(PlayerMaterialDropError::PlayerAirborne),
        "support at the edge of the body cannot anchor a parcel over the empty center"
    );
    assert_eq!(state, before);
}
