//! Excavation overflow must obey the footing rule for stationary ground drops.

use super::*;
use crate::logistics::{player_body_has_fixed_floor, player_placement_has_fixed_floor};
use crate::spatial::{NANOMETERS_PER_VOXEL, WorldPoint};

#[test]
fn full_floor_excavation_cannot_leave_overflow_at_its_own_removed_support() {
    let (registries, mut state, pick, _) = generated_excavation_fixture();
    let player = state.logistics().player().copied().unwrap();
    let at = player.position();
    let floor = VoxelCoord::new(at.x(), at.y() - 1, at.z());
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
    let carried = player.carried_stockpile();
    let free = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .available_capacity();
    let down = ViewDirection::new(0, -VIEW_DIRECTION_PARTS_PER_MILLION, 0).unwrap();
    let request = BlockExcavationRequest::new(
        down,
        MINING_METHOD_HAND_PICK,
        pick,
        Mass::from_milligrams(2),
    );
    let safe = validate_start_player_block_excavation(&registries, &state, request).unwrap();
    assert_eq!(
        safe.work().output(),
        crate::labor::BlockExcavationOutputCustody::Carried { stockpile: carried }
    );
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        free,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let before = state.clone();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, request).err(),
        Some(BlockExcavationError::OverflowOutputRequiresFixedFloor)
    );
    assert_eq!(state, before);
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL.with_primary(PrimaryControlIntent::break_target(down)),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held mining must choose a smaller safe cut: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(1));
    assert_eq!(
        started.output(),
        crate::labor::BlockExcavationOutputCustody::Surface { position: at }
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    // Model an invalid saved full cut in place of the safe partial cut.
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    let work = &mut encoded["state"]["systems"]["player_work"]["active"]["BlockExcavation"]["work"];
    assert_eq!(work["excavated_mass"], serde_json::json!(1));
    work["excavated_mass"] = serde_json::json!(2);
    let decoded: LoadedSaveEnvelope = serde_json::from_value(encoded).unwrap();
    assert_eq!(
        decoded.into_state(&registries),
        Err(LoadError::InvalidState(StateValidationError::PlayerWork(
            PlayerWorkValidationError::BlockExcavationSurfaceRequiresFixedFloor
        )))
    );
    while state.control_tick() < started.completes_at() {
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert_eq!(
        state.world().represented_mass_at(registries.world(), floor),
        Some(Mass::from_milligrams(1))
    );
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn shift_excavator_onto_ledge(registries: &Registries, state: &mut AppState) -> VoxelCoord {
    let player = state.logistics().player().copied().unwrap();
    let position = player.position();
    let below = VoxelCoord::new(position.x(), position.y() - 1, position.z());
    let neighbor = VoxelCoord::new(position.x() + 1, below.y(), position.z());
    set_generated_block_for_test(registries.world(), state, below, BlockStateId::AIR);
    set_generated_block_for_test(registries.world(), state, neighbor, BLOCK_STATE_STONE);
    // The body reaches the solid east lip, but its discrete center is over air.
    let body = player.body();
    let shifted = body.with_motion(
        WorldPoint::new(
            position.x() * NANOMETERS_PER_VOXEL + 950_000_000,
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
    assert_eq!(state.logistics().player().unwrap().position(), position);
    assert!(player_body_has_fixed_floor(registries, state));
    assert!(!player_placement_has_fixed_floor(registries, state));
    position
}

#[test]
fn ledge_supported_mining_can_fill_carried_custody_but_cannot_suspend_overflow_in_a_shaft() {
    let (registries, mut state, pick, target) = generated_excavation_fixture();
    let player = state.logistics().player().copied().unwrap();
    shift_excavator_onto_ledge(&registries, &mut state);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let request = BlockExcavationRequest::new(
        positive_x_view(),
        MINING_METHOD_HAND_PICK,
        pick,
        Mass::from_milligrams(200_000),
    );
    // Safe carried ingress remains allowed on this ledge.
    assert!(validate_start_player_block_excavation(&registries, &state, request).is_ok());

    let carried = player.carried_stockpile();
    let free = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .available_capacity();
    let filler = free.checked_sub(Mass::from_milligrams(199_999)).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        filler,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let baseline = state.clone();
    let matter = calculate_matter_accounting(&state).unwrap().total();
    assert_eq!(
        validate_start_player_block_excavation(&registries, &state, request).err(),
        Some(BlockExcavationError::OverflowOutputRequiresFixedFloor)
    );
    assert_eq!(state, baseline);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(state.world().block_at(target), Some(BLOCK_STATE_STONE));
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // The familiar held-pick action finds the largest safely carried batch
    // instead of refusing all mining because the nominal batch would overflow.
    let control = advance_control_tick_with_direct_input(
        &registries,
        &mut state,
        PlayerDirectControlInput::NEUTRAL
            .with_primary(PrimaryControlIntent::break_target(positive_x_view())),
    )
    .unwrap();
    let started = match control.primary_action() {
        Some(PrimaryActionOutcome::BlockExcavationStarted(started)) => *started,
        other => panic!("held mining should reduce its batch to safe carried custody: {other:?}"),
    };
    assert_eq!(started.excavated_mass(), Mass::from_milligrams(199_999));
    assert_eq!(
        started.output(),
        crate::labor::BlockExcavationOutputCustody::Carried { stockpile: carried }
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let mut completion = None;
    while state.control_tick() < started.completes_at() {
        let tick = advance_control_tick(&registries, &mut state).unwrap();
        if let Some(outcome) = tick.block_excavation() {
            completion = Some(outcome);
        }
    }
    let completion = completion.expect("the reduced excavation batch never completed");
    assert_eq!(completion.excavated_mass(), Mass::from_milligrams(199_999));
    assert!(matches!(
        completion.output(),
        BlockExcavationOutputReceipt::Carried { .. }
    ));
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn trusted_load_rejects_surface_excavation_work_above_an_unsupported_voxel() {
    let (registries, mut state, pick, _target) = generated_excavation_fixture();
    let player = state.logistics().player().copied().unwrap();
    let carried = player.carried_stockpile();
    let free = state
        .inventory()
        .get_stockpile(carried)
        .unwrap()
        .available_capacity();
    let filler = free.checked_sub(Mass::from_milligrams(199_999)).unwrap();
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        CommodityKey::new(MATERIAL_WOOD, FORM_HANDLE),
        filler,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let request = BlockExcavationRequest::new(
        positive_x_view(),
        MINING_METHOD_HAND_PICK,
        pick,
        Mass::from_milligrams(200_000),
    );
    let started = validate_start_player_block_excavation(&registries, &state, request)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        started.output(),
        crate::labor::BlockExcavationOutputCustody::Surface {
            position: player.position()
        }
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    // Model a corrupted save retaining the output binding after the center
    // footing disappears. The body remains stably supported by the ledge.
    shift_excavator_onto_ledge(&registries, &mut state);
    let invalid = StateValidationError::PlayerWork(
        PlayerWorkValidationError::BlockExcavationSurfaceRequiresFixedFloor,
    );
    assert_eq!(
        validate_loaded_state(&registries, &state),
        Err(invalid.clone())
    );
    let encoded = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let loaded: LoadedSaveEnvelope = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        loaded.into_state(&registries),
        Err(LoadError::InvalidState(invalid))
    );
}
