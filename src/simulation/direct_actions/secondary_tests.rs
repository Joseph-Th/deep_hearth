//! Regression contracts for physical-source and gathering-method contextual selection.

#[path = "secondary_tests/food_preview.rs"]
mod food_preview;

use super::*;
use crate::content::{
    FORM_FOOD, FORM_LOG, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, MATERIAL_WOOD,
    build_registries,
};
use crate::core::quantity::{Mass, Temperature};
use crate::material::MaterialComposition;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{
    SurfaceGatheringError, SurfaceResourceKind, SurfaceResourceSpec,
    assess_player_surface_gathering_option_admission, assess_player_surface_gathering_reservation,
    player_surface_gathering_options, validate_create_surface_resources,
    validate_start_player_surface_gathering_option,
};
use crate::survival::{assess_drink_store_to_full_admission, validate_drink_store_to_full};

#[test]
fn distinct_timber_stacks_still_require_explicit_cohort_selection() {
    use crate::core::state::validate_loaded_state;
    use crate::inventory::deposit_lot_for_test;

    let registries = build_registries();
    let mut state = AppState::new();
    let carried = validate_admit_player(&registries, &state, VoxelCoord::new(0, 0, 0))
        .unwrap()
        .commit(&mut state)
        .unwrap()
        .carried_stockpile();
    let timber = CommodityKey::new(MATERIAL_WOOD, FORM_LOG);
    for temperature in [293_150, 303_150] {
        deposit_lot_for_test(
            &registries,
            &mut state,
            carried,
            timber,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(temperature),
        )
        .unwrap();
    }
    let before = state.clone();
    let aim = ViewDirection::new(1_000_000, 0, 0).unwrap();
    assert_eq!(
        attempt_selected_timber_placement(&registries, &mut state, timber, aim),
        Some(SecondaryActionOutcome::AmbiguousMaterialStacks { count: 2 })
    );
    assert_eq!(state, before);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn satisfied_player_with_two_fresh_food_stacks_reports_no_meal_needed() {
    use crate::core::state::validate_loaded_state;
    use crate::inventory::{deposit_lot_for_test, project_material_stacks};
    use crate::logistics::{PlayerActiveSelection, validate_set_active_selection};
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::{
        PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick_with_direct_input,
    };
    use crate::survival::validate_eat_stack_to_full;

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
    let cohorts = project_material_stacks(&registries, &state, carried)
        .unwrap()
        .into_iter()
        .filter(|stack| stack.commodity() == berries)
        .collect::<Vec<_>>();
    assert_eq!(cohorts.len(), 2);
    for cohort in &cohorts {
        assert!(matches!(
            validate_eat_stack_to_full(&registries, &state, cohort),
            Ok(None)
        ));
    }
    validate_set_active_selection(
        &registries,
        &state,
        Some(PlayerActiveSelection::material(berries)),
    )
    .unwrap()
    .commit(&mut state)
    .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let matter_before = calculate_matter_accounting(&state).unwrap().total();
    let inventory_before = state.inventory().clone();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let input = PlayerDirectControlInput::NEUTRAL
        .with_secondary(SecondaryControlIntent::use_active(direction));
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::AlreadySatisfied)
    );
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    assert_eq!(state, replay);
    assert_eq!(state.inventory(), &inventory_before);
    assert_eq!(state.player_work().active(), None);
    assert_eq!(
        calculate_matter_accounting(&state).unwrap().total(),
        matter_before
    );
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn unusable_gathering_does_not_obstruct_a_real_local_drink() {
    use crate::core::state::validate_loaded_state;
    use crate::inventory::deposit_lot_for_test;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    for hot in [true, false] {
        let (registries, mut state, stores) = water_and_gathering_context(&[minimum], false, true);
        let position = state.logistics().player().unwrap().position();
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
        let temperature = Temperature::from_millikelvin(if hot { 423_150 } else { 293_150 });
        let source = validate_create_surface_resources(
            &registries,
            &state,
            [SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                position,
                commodity,
                Mass::from_milligrams(500_000),
                temperature,
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap()],
        )
        .unwrap();
        let resource = source.first_resource_id();
        source.commit(&mut state).unwrap();
        if !hot {
            // The viable stone option cannot start work when the carried
            // stockpile has no space. It must not prevent drinking either.
            let carried = state.logistics().player().unwrap().carried_stockpile();
            deposit_lot_for_test(
                &registries,
                &mut state,
                carried,
                commodity,
                registries.survival().physiology().carried_mass_capacity(),
                Temperature::from_millikelvin(293_150),
            )
            .unwrap();
        }
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
        let matter = calculate_matter_accounting(&state).unwrap().total();
        let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
        let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
            .unwrap()
            .into_state(&registries)
            .unwrap();
        let outcome = attempt_empty_hand_secondary_action(&registries, &mut state);
        let replay_outcome = attempt_empty_hand_secondary_action(&registries, &mut replay);
        assert_eq!(outcome, replay_outcome);
        assert!(matches!(
            outcome,
            SecondaryActionOutcome::DrinkingStarted(ref drink) if drink.store() == stores[0]
        ));
        assert_eq!(state, replay);
        assert_eq!(
            state.surface().get(resource).unwrap().remaining_mass(),
            Mass::from_milligrams(500_000)
        );
        assert_eq!(calculate_matter_accounting(&state).unwrap().total(), matter);
        assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    }
}

#[test]
fn unavailable_gathering_without_drink_retains_its_specific_rejection() {
    let (registries, mut state, _) = water_and_gathering_context(&[], false, true);
    let position = state.logistics().player().unwrap().position();
    let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
    let source = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(423_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    let resource = source.first_resource_id();
    source.commit(&mut state).unwrap();
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringRejected(SurfaceGatheringError::ResourceTooHot {
            resource: blocked,
            ..
        }) if blocked == resource
    ));
}

#[test]
fn unusable_local_source_does_not_obscure_the_only_gatherable_source() {
    use crate::core::state::validate_loaded_state;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let (registries, mut state, _) = water_and_gathering_context(&[], false, true);
    let position = state.logistics().player().unwrap().position();
    let sources = [
        (CommodityKey::new(MATERIAL_STONE, FORM_LUMP), 423_150),
        (CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD), 293_150),
    ]
    .map(|(commodity, millikelvin)| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(millikelvin),
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap()
    });
    validate_create_surface_resources(&registries, &state, sources)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(options.len(), 2);
    let hot_stone = options[0].resource();
    let berries = options[1].resource();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let total = calculate_matter_accounting(&state).unwrap().total();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();

    let result = attempt_empty_hand_secondary_action(&registries, &mut state);
    let replay_result = attempt_empty_hand_secondary_action(&registries, &mut replay);
    assert_eq!(result, replay_result);
    assert!(matches!(
        result,
        SecondaryActionOutcome::SurfaceGatheringStarted(work) if work.resource() == berries
    ));
    assert_eq!(state, replay);
    assert_eq!(
        state.surface().get(hot_stone).unwrap().remaining_mass(),
        Mass::from_milligrams(500_000)
    );
    assert_eq!(calculate_matter_accounting(&state).unwrap().total(), total);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

fn water_and_gathering_context(
    water_volumes: &[crate::core::quantity::Volume],
    forage: bool,
    thirsty: bool,
) -> (
    crate::registry::Registries,
    AppState,
    Vec<crate::fluid::FluidStoreId>,
) {
    use crate::content::{FLUID_WATER, FORM_LUMP, MATERIAL_STONE};
    use crate::fluid::add_fluid_store_with_contents_for_fixture;
    use crate::logistics::validate_place_fluid_store;

    let registries = build_registries();
    let mut state = AppState::new();
    let at = VoxelCoord::new(0, 0, 0);
    validate_admit_player(&registries, &state, at)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let mut stores = Vec::new();
    for &volume in water_volumes {
        let store = add_fluid_store_with_contents_for_fixture(
            &registries,
            &mut state,
            volume,
            FLUID_WATER,
            volume,
            Temperature::from_millikelvin(293_150),
        )
        .unwrap();
        validate_place_fluid_store(&state, store, at)
            .unwrap()
            .commit(&mut state)
            .unwrap();
        stores.push(store);
    }
    if forage {
        let commodity = CommodityKey::new(MATERIAL_STONE, FORM_LUMP);
        validate_create_surface_resources(
            &registries,
            &state,
            [SurfaceResourceSpec::new(
                SurfaceResourceKind::Opportunity,
                at,
                commodity,
                Mass::from_milligrams(500_000),
                Temperature::from_millikelvin(293_150),
                MaterialComposition::pure(MATERIAL_STONE),
            )
            .unwrap()],
        )
        .unwrap()
        .commit(&mut state)
        .unwrap();
    }
    if thirsty {
        let _ = crate::simulation::advance_tick(&registries, &mut state).unwrap();
        assert!(
            state.survival().player().unwrap().hydration()
                < registries.survival().physiology().maximum_hydration()
        );
    }
    assert_eq!(
        crate::core::state::validate_loaded_state(&registries, &state),
        Ok(())
    );
    (registries, state, stores)
}

#[test]
fn hydrated_player_with_two_local_water_sources_has_no_drink_choice_to_make() {
    use crate::core::quantity::Volume;
    use crate::core::state::validate_loaded_state;
    use crate::fluid::calculate_fluid_volume_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::{
        PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick_with_direct_input,
    };

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, mut state, stores) =
        water_and_gathering_context(&[minimum, minimum], false, false);
    assert_eq!(stores.len(), 2);
    assert_eq!(
        state.survival().player().unwrap().hydration(),
        registries.survival().physiology().maximum_hydration()
    );
    let volumes_before = stores
        .iter()
        .map(|&id| state.fluid().get_store(id).unwrap().stored_volume())
        .collect::<Vec<_>>();
    assert_eq!(volumes_before, vec![minimum; 2]);
    let fluid_before = calculate_fluid_volume_accounting(&state).unwrap();
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &state)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let input = PlayerDirectControlInput::NEUTRAL.with_secondary(
        SecondaryControlIntent::use_active(ViewDirection::new(1_000_000, 0, 0).unwrap()),
    );
    let outcome = advance_control_tick_with_direct_input(&registries, &mut state, input).unwrap();
    assert_eq!(
        outcome.secondary_action(),
        Some(&SecondaryActionOutcome::AlreadySatisfied)
    );
    assert_eq!(
        outcome,
        advance_control_tick_with_direct_input(&registries, &mut replay, input).unwrap()
    );
    assert_eq!(state, replay);
    assert_eq!(
        stores
            .iter()
            .map(|&id| state.fluid().get_store(id).unwrap().stored_volume())
            .collect::<Vec<Volume>>(),
        volumes_before
    );
    assert_eq!(
        calculate_fluid_volume_accounting(&state).unwrap(),
        fluid_before
    );
    assert_eq!(state.player_work().active(), None);
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
}

#[test]
fn observed_water_without_a_survival_actor_does_not_invent_full_hydration() {
    use crate::content::FLUID_WATER;
    use crate::fluid::add_fluid_store_with_contents_for_fixture;
    use crate::logistics::{validate_initialize_player_logistics, validate_place_fluid_store};
    use crate::survival::{DrinkStoreToTargetError, available_local_drink_sources};

    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let volume = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let store = add_fluid_store_with_contents_for_fixture(
        &registries,
        &mut state,
        volume,
        FLUID_WATER,
        volume,
        Temperature::from_millikelvin(293_150),
    )
    .unwrap();
    validate_place_fluid_store(&state, store, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_initialize_player_logistics(&state, position, Mass::from_milligrams(25_000_000))
        .unwrap()
        .commit(&mut state)
        .unwrap();
    assert!(state.survival().player().is_none());
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| source.store())
            .collect::<Vec<_>>(),
        vec![store]
    );
    let before = state.clone();
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::DrinkingRejected(DrinkStoreToTargetError::SurvivalNotInitialized)
    );
    assert_eq!(state, before);
}

#[test]
fn unusable_water_remainder_does_not_obstruct_empty_hand_gathering() {
    use crate::core::quantity::Volume;
    use crate::survival::available_local_drink_sources;

    let registries = build_registries();
    let minimum = registries
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume()
        .microliters();
    let (registries, mut state, stores) =
        water_and_gathering_context(&[Volume::from_microliters(minimum - 1)], true, true);
    assert_eq!(
        available_local_drink_sources(&registries, &state)
            .map(|source| source.store())
            .collect::<Vec<_>>(),
        stores,
        "the observed water remains visible even when it cannot be consumed"
    );
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        Volume::from_microliters(minimum - 1)
    );
}

#[test]
fn contextual_drinking_ignores_empty_servings_but_preserves_actual_ambiguity() {
    use crate::core::quantity::Volume;

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let tiny = Volume::from_microliters(minimum.microliters() - 1);
    let (registries, mut state, stores) =
        water_and_gathering_context(&[tiny, minimum], false, true);
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::DrinkingStarted(outcome) if outcome.store() == stores[1]
    ));

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], true, true);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: 1,
            surface_resources: 1,
        }
    );

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], true, false);
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
}

#[test]
fn gathering_without_surface_revision_headroom_cannot_mask_drinking() {
    use crate::core::state::validate_loaded_state;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::{
        PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, state, stores) = water_and_gathering_context(&[minimum], true, true);
    let mut saved = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    saved["state"]["systems"]["surface"]["revision"] = serde_json::json!(u64::MAX);
    let mut exhausted = serde_json::from_value::<LoadedSaveEnvelope>(saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let option = player_surface_gathering_options(&registries, &exhausted)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let amount = option.direct_batch_mass().min(option.source_mass());
    assert_eq!(
        assess_player_surface_gathering_reservation(&exhausted, amount),
        Ok(())
    );
    assert!(matches!(
        validate_start_player_surface_gathering_option(&registries, &exhausted, option, amount),
        Err(SurfaceGatheringError::SurfaceRevisionExhausted)
    ));
    assert_eq!(validate_loaded_state(&registries, &exhausted), Ok(()));
    let mut via_input = exhausted.clone();
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut exhausted),
        SecondaryActionOutcome::DrinkingStarted(ref drink) if drink.store() == stores[0]
    ));
    assert_eq!(exhausted.surface().revision(), u64::MAX);
    assert_eq!(
        exhausted
            .surface()
            .get(option.resource())
            .unwrap()
            .remaining_mass(),
        option.source_mass()
    );
    assert_eq!(validate_loaded_state(&registries, &exhausted), Ok(()));

    // The ordinary held-use input now admits drinking on this control frame.
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut via_input,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert!(matches!(
        queued.secondary_action(),
        Some(SecondaryActionOutcome::DrinkingStarted(drink)) if drink.store() == stores[0]
    ));
    assert!(via_input.logistics().pending_secondary_use().is_none());
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &via_input)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut via_input).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, restored);
        assert_eq!(via_input, replay);
    }
    assert_eq!(via_input.surface().revision(), u64::MAX);
    assert_eq!(validate_loaded_state(&registries, &via_input), Ok(()));
}

#[test]
fn drinking_without_survival_revision_headroom_cannot_mask_short_gathering() {
    use crate::core::state::validate_loaded_state;
    use crate::matter::calculate_matter_accounting;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};
    use crate::simulation::{
        PlayerDirectControlInput, SecondaryControlIntent, advance_control_tick,
        advance_control_tick_with_direct_input,
    };
    use crate::survival::{DrinkError, DrinkStoreToTargetError};

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, mut state, stores) = water_and_gathering_context(&[minimum], false, true);
    let position = state.logistics().player().unwrap().position();
    let option = validate_create_surface_resources(
        &registries,
        &state,
        [SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
            Mass::from_milligrams(1),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(MATERIAL_STONE),
        )
        .unwrap()],
    )
    .unwrap();
    option.commit(&mut state).unwrap();
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["survival"]["revision"] = serde_json::json!(u64::MAX - 3);
    let mut state = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert_eq!(validate_loaded_state(&registries, &state), Ok(()));
    let option = player_surface_gathering_options(&registries, &state)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        assess_player_surface_gathering_option_admission(
            &registries,
            &state,
            option,
            Mass::from_milligrams(1),
        ),
        Ok(())
    );
    assert_eq!(
        validate_drink_store_to_full(&registries, &state, stores[0]).err(),
        Some(DrinkStoreToTargetError::Drink(
            DrinkError::SurvivalRevisionExhausted
        )),
    );
    let mut off_boundary = state.clone();
    let _ = crate::simulation::advance_control_tick(&registries, &mut off_boundary).unwrap();
    assert!(!off_boundary.control_tick().is_simulation_boundary());
    assert!(matches!(
        validate_drink_store_to_full(&registries, &off_boundary, stores[0]),
        Err(DrinkStoreToTargetError::Drink(
            DrinkError::SimulationBoundary(_)
        ))
    ));
    let mut via_input = off_boundary.clone();
    assert!(
        matches!(
            attempt_empty_hand_secondary_action(&registries, &mut off_boundary),
            SecondaryActionOutcome::SurfaceGatheringStarted(_)
        ),
        "an impossible drink cannot hide an immediately admissible gather"
    );
    assert!(
        matches!(
            attempt_empty_hand_secondary_action(&registries, &mut state),
            SecondaryActionOutcome::SurfaceGatheringStarted(_)
        ),
        "a drink that cannot reserve its uptake revisions must not create fake contextual ambiguity"
    );
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        minimum
    );
    // Input-frame gathering replays through its work interval without taking water.
    let conserved = calculate_matter_accounting(&via_input).unwrap().total();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut via_input,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert!(matches!(
        queued.secondary_action(),
        Some(SecondaryActionOutcome::SurfaceGatheringStarted(work))
            if work.resource() == option.resource()
    ));
    assert!(via_input.logistics().pending_secondary_use().is_none());
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &via_input)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let outcome = advance_control_tick(&registries, &mut via_input).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(outcome, restored);
        assert_eq!(via_input, replay);
    }
    assert_eq!(
        via_input
            .fluid()
            .get_store(stores[0])
            .unwrap()
            .stored_volume(),
        minimum
    );
    assert_eq!(
        calculate_matter_accounting(&via_input).unwrap().total(),
        conserved
    );
    assert_eq!(validate_loaded_state(&registries, &via_input), Ok(()));
}

#[test]
fn off_boundary_feasibility_preview_preserves_real_water_and_gathering_ambiguity() {
    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, mut state, stores) = water_and_gathering_context(&[minimum], true, true);
    let _ = crate::simulation::advance_control_tick(&registries, &mut state).unwrap();
    assert!(!state.control_tick().is_simulation_boundary());
    assert!(assess_drink_store_to_full_admission(
        &registries,
        &state,
        stores[0]
    ));
    let snapshot = state.clone();
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::AmbiguousEmptyHandContext {
            drink_sources: 1,
            surface_resources: 1,
        }
    );
    assert_eq!(state, snapshot);
}

#[test]
fn unavailable_water_egress_cannot_mask_local_gathering() {
    use crate::fluid::validate_fluid_egress;
    use crate::persistence::{LoadedSaveEnvelope, SaveEnvelope};

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let (registries, state, stores) = water_and_gathering_context(&[minimum], true, true);
    let mut encoded = serde_json::to_value(SaveEnvelope::new(&registries, &state)).unwrap();
    encoded["state"]["systems"]["fluid"]["revision"] = serde_json::json!(u64::MAX);
    let mut state = serde_json::from_value::<LoadedSaveEnvelope>(encoded)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    assert!(validate_fluid_egress(&registries, &state, stores[0], minimum).is_err());
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        minimum
    );
    assert!(matches!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::SurfaceGatheringStarted(_)
    ));
    assert_eq!(
        state.fluid().get_store(stores[0]).unwrap().stored_volume(),
        minimum
    );
}

#[test]
fn water_only_context_retains_specific_failure_and_satisfaction_results() {
    use crate::core::quantity::Volume;
    use crate::survival::DrinkStoreToTargetError;

    let minimum = build_registries()
        .survival()
        .physiology()
        .direct_consumption()
        .minimum_drink_volume();
    let tiny = Volume::from_microliters(minimum.microliters() - 1);
    let (registries, mut state, stores) = water_and_gathering_context(&[tiny], false, true);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::DrinkingRejected(DrinkStoreToTargetError::InsufficientVolume {
            store: stores[0],
            available: tiny,
            required: minimum,
        })
    );

    let (registries, mut state, _) = water_and_gathering_context(&[minimum], false, false);
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut state),
        SecondaryActionOutcome::AlreadySatisfied
    );
}
