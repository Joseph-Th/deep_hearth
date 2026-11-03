//! Selected edible material is assessed on the input frame without committing a meal.

use super::*;
use crate::content::{FORM_FOOD, MATERIAL_BERRIES, build_registries};
use crate::core::quantity::{Mass, Temperature};
use crate::core::state::validate_loaded_state;
use crate::inventory::{deposit_lot_for_test, project_material_stacks};
use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
use crate::material::CommodityKey;
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::player::validate_admit_player;
use crate::simulation::{
    PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick_with_direct_input,
    advance_tick,
};
use crate::spatial::{ViewDirection, VoxelCoord};
use crate::survival::{EatStackToTargetError, validate_eat_stack_to_full};

fn selected_berries(mass: Mass) -> (Registries, AppState) {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        mass,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    (registries, state)
}

fn use_selected_food() -> PlayerDirectControlInput {
    let aim = ViewDirection::new(1_000_000, 0, 0).unwrap();
    PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::use_active(aim))
}

#[test]
fn undersized_food_is_rejected_on_input_without_queueing_a_doomed_meal() {
    let (registries, mut state) = selected_berries(Mass::from_milligrams(1));
    let _ = advance_tick(&registries, &mut state).unwrap();
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let stack = project_material_stacks(&registries, &state, carried).unwrap();
    assert_eq!(stack.len(), 1);
    let expected = validate_eat_stack_to_full(&registries, &state, &stack[0])
        .expect_err("an undersized serving cannot admit eating");
    assert!(matches!(
        expected,
        EatStackToTargetError::InsufficientStackMass { .. }
    ));
    let before = state.clone();
    let conserved = calculate_matter_accounting(&state).unwrap().total();
    let save = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut restored = serde_json::from_slice::<LoadedSaveEnvelope>(&save)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    let outcome =
        advance_control_tick_with_direct_input(&registries, &mut state, use_selected_food())
            .unwrap();
    let replay =
        advance_control_tick_with_direct_input(&registries, &mut restored, use_selected_food())
            .unwrap();
    assert_eq!(outcome, replay);
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::EatingRejected(expected))
    );
    assert_eq!(state, restored);
    assert_eq!(state.inventory(), before.inventory());
    assert_eq!(state.survival(), before.survival());
    assert!(state.logistics().pending_secondary_use().is_none());
    assert!(state.player_work().active().is_none());
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn viable_food_starts_on_input_and_satisfied_food_does_not_queue() {
    let (registries, mut viable) = selected_berries(Mass::from_milligrams(250_000));
    let _ = advance_tick(&registries, &mut viable).unwrap();
    let carried = viable.logistics().player().unwrap().carried_stockpile();
    let stack = project_material_stacks(&registries, &viable, carried).unwrap();
    assert!(matches!(
        validate_eat_stack_to_full(&registries, &viable, &stack[0]),
        Ok(Some(_))
    ));
    let started =
        advance_control_tick_with_direct_input(&registries, &mut viable, use_selected_food())
            .unwrap();
    assert!(matches!(
        started.secondary_action(),
        Some(SecondaryActionOutcome::EatingStarted(_))
    ));
    assert!(viable.logistics().pending_secondary_use().is_none());
    assert!(matches!(
        viable.player_work().active(),
        Some(crate::labor::PlayerWork::Eating { .. })
    ));
    assert_eq!(validate_loaded_state(&registries, &viable), Ok(()));

    let (registries, mut satisfied) = selected_berries(Mass::from_milligrams(250_000));
    let carried = satisfied.logistics().player().unwrap().carried_stockpile();
    let stack = project_material_stacks(&registries, &satisfied, carried).unwrap();
    assert!(matches!(
        validate_eat_stack_to_full(&registries, &satisfied, &stack[0]),
        Ok(None)
    ));
    let result =
        advance_control_tick_with_direct_input(&registries, &mut satisfied, use_selected_food())
            .unwrap();
    assert_eq!(
        result.secondary_action(),
        Some(&SecondaryActionOutcome::AlreadySatisfied)
    );
    assert!(satisfied.logistics().pending_secondary_use().is_none());
    assert!(satisfied.player_work().active().is_none());
    assert_eq!(validate_loaded_state(&registries, &satisfied), Ok(()));
}
