//! Explicit physical intake choices remain canonical across input, save and replay.

use super::*;
use crate::core::state::validate_loaded_state;
use crate::fluid::calculate_fluid_volume_accounting;
use crate::inventory::{deposit_lot_for_test, project_material_stacks};
use crate::matter::calculate_matter_accounting;
use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
use crate::simulation::{
    PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick_with_direct_input,
    advance_tick,
};

#[test]
fn explicit_food_choice_consumes_only_observed_cohort_and_rejects_stale_choices() {
    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let berries = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    for temperature in [290_000, 300_000] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            berries,
            Mass::from_milligrams(250_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap();
    }
    let _ = advance_tick(&registries, &mut state).unwrap();
    let stacks = project_material_stacks(&registries, &state, carried).unwrap();
    assert_eq!(stacks.len(), 2);
    assert_ne!(stacks[0].temperature(), stacks[1].temperature());
    let choice = stacks[1].choice();
    let original = state.clone();
    let total = calculate_matter_accounting(&state).unwrap().total();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let input =
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::eat_stack(choice));
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    assert_eq!(state, replay);
    assert!(matches!(
        outcome.secondary_action(),
        Some(SecondaryActionOutcome::EatingStarted(meal)) if !meal.total_mass().is_zero()
    ));
    let after = project_material_stacks(&registries, &state, carried).unwrap();
    assert_eq!(
        after
            .iter()
            .find(|stack| stack.temperature() == stacks[0].temperature())
            .unwrap()
            .mass(),
        stacks[0].mass(),
        "explicit eating must not consume the other physical cohort"
    );
    assert!(
        after
            .iter()
            .find(|stack| stack.temperature() == stacks[1].temperature())
            .is_none_or(|stack| stack.mass() < stacks[1].mass())
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));

    let mut stale = original;
    let _ = advance_tick(&registries, &mut stale).unwrap();
    let before_inventory = stale.inventory().clone();
    let rejection = advance_control_tick_with_direct_input(
        &registries,
        &mut stale,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::eat_stack(choice)),
    )
    .unwrap();
    assert!(matches!(
        rejection.secondary_action(),
        Some(SecondaryActionOutcome::EatingRejected(
            crate::survival::EatStackToTargetError::Stack(
                crate::inventory::MaterialStackProjectionError::StaleSimulationTick { .. }
            )
        ))
    ));
    assert_eq!(stale.inventory(), &before_inventory);
    assert_eq!(validate_loaded_state(&registries, &stale), Ok(()));
}

#[test]
fn explicit_drinking_resolves_co_located_water_and_gathering_without_spilling() {
    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, mut state, stores) =
        water_and_gathering_context(&[minimum, minimum], true, true);
    let before = state.clone();
    let total = calculate_fluid_volume_accounting(&state).unwrap();
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::drink_source(stores[1]));
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    assert!(matches!(
        outcome.secondary_action(),
        Some(SecondaryActionOutcome::DrinkingStarted(drink))
            if drink.store() == stores[1]
    ));
    assert_eq!(state, replay);
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        before.fluid().get_store(stores[0]).unwrap().stored_volume(),
        "the unchosen finite source must remain unchanged"
    );
    assert!(
        state.fluid().get_store(stores[1]).unwrap().stored_volume()
            < before.fluid().get_store(stores[1]).unwrap().stored_volume()
    );
    assert_eq!(state.surface(), before.surface());
    assert_eq!(calculate_fluid_volume_accounting(&state).unwrap(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}
