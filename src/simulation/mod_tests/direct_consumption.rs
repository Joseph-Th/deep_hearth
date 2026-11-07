//! Control-rate food and drink refill planning after already-incurred body exertion.

use super::*;

fn generated_player_after_unsettled_jump() -> (Registries, AppState, i64) {
    let (registries, mut state, floor_y) = generated_grounded_player_at(0, 0);
    let jump = PlayerControlInput::NEUTRAL.with_jump(true);
    let _ = advance_control_tick_with_input(&registries, &mut state, jump).unwrap();
    for _ in 0..60 {
        let player = state.logistics().player().unwrap();
        if player_body_is_grounded(&registries, &state)
            && player.body().vertical_velocity().nanometers_per_second() == 0
        {
            break;
        }
        let _ = advance_control_tick(&registries, &mut state).unwrap();
    }
    assert!(player_body_is_grounded(&registries, &state));
    assert!(!state.control_tick().is_simulation_boundary());
    (registries, state, floor_y)
}

#[test]
fn direct_drink_prices_unsettled_jump_hydration_before_claiming_full_refill() {
    use crate::content::FLUID_WATER;
    use crate::core::quantity::Volume;
    use crate::fluid::add_fluid_store_with_contents_for_fixture;
    use crate::logistics::validate_place_fluid_store;
    use crate::survival::{
        project_minimum_drink_to_hydration_target, validate_drink_store_to_full_control_rate,
    };

    let (registries, mut state, floor_y) = generated_player_after_unsettled_jump();
    let pending = state
        .logistics()
        .player()
        .unwrap()
        .locomotion_exertion()
        .pending()
        .hydration_loss_per_tick();
    assert!(!pending.is_zero());
    let physiology = registries.survival().physiology();
    let current = physiology
        .maximum_hydration()
        .checked_sub(Volume::from_microliters(350_000))
        .unwrap();
    let before = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            before.metabolic_energy(),
            current,
            before.vitality(),
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let capacity = Volume::from_microliters(1_000_000);
    let water = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        capacity,
        FLUID_WATER,
        capacity,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, water, VoxelCoord::new(0, floor_y, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    // A seemingly full reserve still owes the jump's hydration. Empty-hand
    // contextual use must recognize the outstanding need rather than dismiss
    // the water source as an already-satisfied no-op.
    let mut full_with_debt = state.clone();
    let saved = *full_with_debt.survival().player().unwrap();
    let revision = full_with_debt.survival().revision();
    full_with_debt.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            saved.metabolic_energy(),
            physiology.maximum_hydration(),
            saved.vitality(),
            saved.nutrition(),
            saved.vitality_recovery_remainder(),
            saved.thermal_comfort_ppm(),
        ),
    );
    let context = advance_control_tick_with_direct_input(
        &registries,
        &mut full_with_debt,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::use_active(
            ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap(),
        )),
    )
    .unwrap();
    assert!(
        matches!(
            context.secondary_action(),
            Some(SecondaryActionOutcome::DrinkingStarted(_))
        ),
        "a full display reserve cannot hide hydration already owed for locomotion: {:?}",
        context.secondary_action()
    );

    let drink = *registries.survival().get_drink(FLUID_WATER).unwrap();
    let unpriced = project_minimum_drink_to_hydration_target(
        physiology,
        drink,
        current,
        physiology.maximum_hydration(),
    )
    .unwrap()
    .unwrap();
    let priced = project_minimum_drink_to_hydration_target(
        physiology,
        drink,
        current.checked_sub(pending).unwrap(),
        physiology.maximum_hydration(),
    )
    .unwrap()
    .unwrap();
    assert!(priced.volume() > unpriced.volume());
    let original = state.clone();
    let outcome = validate_drink_store_to_full_control_rate(&registries, &state, water)
        .unwrap()
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert_eq!(
        outcome.volume(),
        priced.volume(),
        "a full-refill use must pay movement costs already incurred before the drink"
    );
    assert_eq!(state.tick(), original.tick());
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    while state.tick() < outcome.completes_at() {
        let live = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            live,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.survival().player().unwrap().hydration(),
        physiology.maximum_hydration()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}

#[test]
fn selected_meal_prices_unsettled_jump_energy_before_claiming_full_refill() {
    use crate::content::{FORM_FOOD, MATERIAL_BERRIES};
    use crate::inventory::{deposit_lot_for_test, project_material_stacks};
    use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
    use crate::survival::{
        project_minimum_meal_to_metabolic_target, validate_eat_stack_to_full_control_rate,
    };

    let (registries, mut state, _) = generated_player_after_unsettled_jump();
    let pending = state
        .logistics()
        .player()
        .unwrap()
        .locomotion_exertion()
        .pending()
        .energy_cost_per_tick();
    assert!(!pending.is_zero());
    let physiology = registries.survival().physiology();
    let current = physiology
        .maximum_metabolic_energy()
        .checked_sub(Energy::from_nanojoules(350_000_000_000_000))
        .unwrap();
    let before = *state.survival().player().unwrap();
    let revision = state.survival().revision();
    state.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            current,
            before.hydration(),
            before.vitality(),
            before.nutrition(),
            before.vitality_recovery_remainder(),
            before.thermal_comfort_ppm(),
        ),
    );
    let carried = state.logistics().player().unwrap().carried_stockpile();
    let commodity = CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD);
    deposit_lot_for_test(
        &registries,
        &mut state,
        carried,
        commodity,
        Mass::from_milligrams(500_000),
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    let stack = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .remove(0);
    let food = *registries.survival().get_food(commodity).unwrap();
    let unpriced = project_minimum_meal_to_metabolic_target(
        physiology,
        food,
        current,
        physiology.maximum_metabolic_energy(),
    )
    .unwrap()
    .unwrap();
    let priced = project_minimum_meal_to_metabolic_target(
        physiology,
        food,
        current.checked_sub(pending).unwrap(),
        physiology.maximum_metabolic_energy(),
    )
    .unwrap()
    .unwrap();
    assert!(priced.mass() > unpriced.mass());
    let validated = validate_eat_stack_to_full_control_rate(&registries, &state, &stack)
        .unwrap()
        .unwrap();

    // With the displayed reserve exactly full, a committed jump still
    // creates a real refill need. The selected-food input must not no-op.
    let mut full_with_debt = state.clone();
    let saved = *full_with_debt.survival().player().unwrap();
    let revision = full_with_debt.survival().revision();
    full_with_debt.survival_state_mut().apply_player(
        revision,
        revision + 1,
        player_record_with_thermal_comfort(
            physiology.maximum_metabolic_energy(),
            saved.hydration(),
            saved.vitality(),
            saved.nutrition(),
            saved.vitality_recovery_remainder(),
            saved.thermal_comfort_ppm(),
        ),
    );
    validate_set_active_selection(
        &registries,
        &full_with_debt,
        Some(PlayerActiveSelection::material(commodity)),
    )
    .unwrap()
    .commit(&mut full_with_debt)
    .unwrap();
    let action = advance_control_tick_with_direct_input(
        &registries,
        &mut full_with_debt,
        PlayerDirectControlInput::NEUTRAL.with_secondary(SecondaryControlIntent::use_active(
            ViewDirection::new(VIEW_DIRECTION_PARTS_PER_MILLION, 0, 0).unwrap(),
        )),
    )
    .unwrap();
    assert!(
        matches!(
            action.secondary_action(),
            Some(SecondaryActionOutcome::EatingStarted(_))
        ),
        "already incurred movement energy must be refillable: {:?}",
        action.secondary_action()
    );

    let meal = validated.commit(&mut state).unwrap();
    assert_eq!(
        meal.total_mass(),
        priced.mass(),
        "selected food must cover metabolic energy already owed for movement"
    );
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(
        &serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap(),
    )
    .unwrap()
    .into_state(&registries)
    .unwrap();
    while state.tick() < meal.completes_at() {
        let live = advance_control_tick(&registries, &mut state).unwrap();
        assert_eq!(
            live,
            advance_control_tick(&registries, &mut replay).unwrap()
        );
    }
    assert_eq!(state, replay);
    assert_eq!(
        state.survival().player().unwrap().metabolic_energy(),
        physiology.maximum_metabolic_energy()
    );
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
}
