//! Equipment assembly settles into stationary voxel custody only from real player footing.

use super::*;
use crate::content::{
    BLOCK_STATE_STONE, EQUIPMENT_STONE_PICK, FORM_HANDLE, FORM_TOOL, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::Temperature;
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

fn generated_assembly_fixture(airborne: bool) -> (Registries, AppState, StockpileId, VoxelCoord) {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xE09A_55E0),
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
    let position = VoxelCoord::new(5, floor + if airborne { 3 } else { 1 }, 5);
    let carried = validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    for (commodity, mass) in [
        (
            CommodityKey::new(MATERIAL_STONE, FORM_TOOL),
            Mass::from_milligrams(800_000),
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
    (registries, state, carried, VoxelCoord::new(5, floor, 5))
}

#[test]
fn falling_player_cannot_assemble_stationary_equipment_until_landing() {
    let (registries, mut state, carried, floor) = generated_assembly_fixture(true);
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried).err(),
        Some(EquipmentAssemblyError::PlayerAirborne),
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
        assert_eq!(
            advance_control_tick(&registries, &mut state).unwrap(),
            advance_control_tick(&registries, &mut replay).unwrap()
        );
        assert_eq!(state, replay);
    }
    assert!(player_body_is_grounded(&registries, &state));
    for actor in [&mut state, &mut replay] {
        let equipment =
            validate_assemble_equipment(&registries, actor, EQUIPMENT_STONE_PICK, carried)
                .unwrap()
                .commit(actor)
                .unwrap();
        assert_eq!(
            actor.logistics().stationary_equipment_position(equipment),
            Some(VoxelCoord::new(floor.x(), floor.y() + 1, floor.z()))
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
fn equipment_assembly_rechecks_footing_before_consuming_stock() {
    use crate::world::{BlockStateId, set_generated_block_for_test};

    let (registries, mut state, carried, floor) = generated_assembly_fixture(false);
    assert!(player_body_is_grounded(&registries, &state));
    let validated =
        validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried).unwrap();
    let expected = state.world().revision();
    set_generated_block_for_test(registries.world(), &mut state, floor, BlockStateId::AIR);
    assert!(!player_body_is_grounded(&registries, &state));
    let before = state.clone();
    assert_eq!(
        validated.commit(&mut state),
        Err(EquipmentAssemblyCommitError::StaleWorld {
            expected,
            actual: before.world().revision(),
        })
    );
    assert_eq!(state, before);
}
