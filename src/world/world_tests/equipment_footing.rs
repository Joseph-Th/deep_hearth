//! Stationary, unmounted material-built equipment retains its supporting terrain.

use super::*;
use crate::world::{
    BlockExcavationError, BlockExcavationRequest, raycast_player_block,
    validate_start_player_block_excavation,
};

#[test]
fn placed_equipment_blocks_final_footing_cut_until_physically_picked_up() {
    let (registries, mut state, mining_pick, _) = generated_excavation_fixture();
    let position = state.logistics().player().unwrap().position();
    let floor = VoxelCoord::new(position.x(), position.y() - 1, position.z());
    let full = state
        .world()
        .represented_mass_at(registries.world(), floor)
        .unwrap();
    let revision = state.world().revision();
    state.world_state_mut().apply_excavated_cell_mass(
        revision,
        revision + 1,
        floor,
        BLOCK_STATE_STONE,
        full,
        Mass::from_milligrams(2),
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
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
    let assembled = validate_assemble_equipment(&registries, &state, EQUIPMENT_STONE_PICK, carried)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(assembled),
        Some(position)
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let conserved = calculate_matter_accounting(&state).unwrap().total();

    let down = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    assert_eq!(
        raycast_player_block(&registries, &state, down)
            .unwrap()
            .unwrap()
            .voxel(),
        floor
    );
    let dig = |mass| {
        BlockExcavationRequest::new(
            down,
            MINING_METHOD_HAND_PICK,
            mining_pick,
            Mass::from_milligrams(mass),
        )
    };
    let before = state.clone();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig(2)).err(),
        Some(BlockExcavationError::StationaryEquipmentRequiresFloor {
            target: floor,
            equipment: assembled,
        }),
    );
    assert_eq!(state, before);
    // The familiar held-pick control must find the admissible partial cut
    // instead of rejecting the entire block while the machinery rests on it.
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::break_target(down)),
    )
    .unwrap();
    let cut = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(outcome)) => *outcome,
        other => panic!("held excavation failed to select the supported partial cut: {other:?}"),
    };
    assert_eq!(cut.excavated_mass(), Mass::from_milligrams(1));
    while state.control_tick() < cut.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, dig(1)).err(),
        Some(BlockExcavationError::StationaryEquipmentRequiresFloor {
            target: floor,
            equipment: assembled,
        }),
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let restored = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(restored, state);

    validate_pickup_equipment(&registries, &state, assembled)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        state.logistics().stationary_equipment_position(assembled),
        None
    );
    assert!(validate_start_player_block_excavation(&registries, &state, dig(1)).is_ok());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    assert_eq!(state.world().block_at(floor), Some(BLOCK_STATE_STONE));
}
