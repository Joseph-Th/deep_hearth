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
