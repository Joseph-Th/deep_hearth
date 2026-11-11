//! New storage placement shares the physical floor rule of existing-stockpile enclosure.

use super::*;
use crate::content::{BLOCK_STATE_STONE, STORAGE_ROUGH_TIMBER_FIELD_BOX, build_registries};
use crate::core::quantity::Temperature;
use crate::core::state::validate_loaded_state;
use crate::inventory::deposit_lot_for_test;
use crate::logistics::player_body_is_grounded;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::advance_control_tick;
use crate::world::{
    ChunkCoord, WORLD_MAX_Y_EXCLUSIVE, WORLD_MIN_Y, WorldGenerationSeed, validate_generate_chunk,
};

#[test]
fn placement_cannot_assemble_a_hanging_box_without_ground_support() {
    let registries = build_registries();
    let mut state = AppState::new();
    validate_generate_chunk(
        &registries,
        &state,
        WorldGenerationSeed::new(0xB0AF_F00D),
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
    let floating = VoxelCoord::new(5, floor + 3, 5);
    let carried = validate_admit_player(&registries, &state, floating)
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let definition = registries
        .storage()
        .get(STORAGE_ROUGH_TIMBER_FIELD_BOX)
        .unwrap();
    for input in definition.assembly_profile().inputs() {
        let _ = deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            input.commodity(),
            input.mass(),
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
    }
    assert!(!player_body_is_grounded(&registries, &state));
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let before = state.clone();
    assert_eq!(
        validate_place_player_storage(&registries, &state, STORAGE_ROUGH_TIMBER_FIELD_BOX).err(),
        Some(PlayerStoragePlacementError::Construction(
            StorageEnclosureConstructionError::UnsupportedFloor { position: floating },
        )),
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
        let box_stockpile =
            validate_place_player_storage(&registries, actor, STORAGE_ROUGH_TIMBER_FIELD_BOX)
                .unwrap()
                .commit(actor)
                .unwrap();
        assert_eq!(
            actor
                .logistics()
                .stationary_stockpile_position(box_stockpile),
            Some(VoxelCoord::new(5, floor + 1, 5))
        );
        assert!(
            actor
                .inventory()
                .get_stockpile(box_stockpile)
                .unwrap()
                .enclosure()
                .is_some()
        );
        assert_eq!(
            calculate_matter_accounting(actor).unwrap().total(),
            conserved
        );
        assert_eq!(validate_loaded_state(&registries, actor), Ok(()));
    }
    assert_eq!(state, replay);
}
