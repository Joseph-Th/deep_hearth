//! Regression contracts for physical-source and gathering-method contextual selection.

#[path = "secondary_tests/food_preview.rs"]
mod food_preview;

use super::*;
use crate::content::{FORM_FOOD, FORM_LUMP, MATERIAL_BERRIES, MATERIAL_STONE, build_registries};
use crate::core::quantity::Temperature;
use crate::material::MaterialComposition;
use crate::player::validate_admit_player;
use crate::spatial::VoxelCoord;
use crate::surface::{SurfaceResourceKind, SurfaceResourceSpec, validate_create_surface_resources};

#[test]
fn contextual_gathering_counts_physical_sources_before_method_choices() {
    let registries = build_registries();
    let mut state = AppState::new();
    let position = VoxelCoord::new(0, 0, 0);
    let specs = [
        CommodityKey::new(MATERIAL_STONE, FORM_LUMP),
        CommodityKey::new(MATERIAL_BERRIES, FORM_FOOD),
    ]
    .map(|commodity| {
        SurfaceResourceSpec::new(
            SurfaceResourceKind::Opportunity,
            position,
            commodity,
            Mass::from_milligrams(500_000),
            Temperature::from_millikelvin(293_150),
            MaterialComposition::pure(commodity.material()),
        )
        .unwrap()
    });
    validate_create_surface_resources(&registries, &state, specs)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    validate_admit_player(&registries, &state, position)
        .unwrap()
        .commit(&mut state)
        .unwrap();
    let options = player_surface_gathering_options(&registries, &state).unwrap();
    assert_eq!(options.len(), 2);
    assert_ne!(options[0].resource(), options[1].resource());

    assert!(matches!(
        classify_direct_surface_gathering_options(&[]),
        DirectSurfaceGatheringResolution::None
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&options[..1]),
        DirectSurfaceGatheringResolution::One(_)
    ));
    // An additional method for the same observed source must not be
    // counted as another physical resource or hide the second source.
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0]]),
        DirectSurfaceGatheringResolution::AmbiguousMethods { count: 2 }
    ));
    assert!(matches!(
        classify_direct_surface_gathering_options(&[options[0], options[0], options[1]]),
        DirectSurfaceGatheringResolution::AmbiguousResources { count: 2 }
    ));
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

    // The ordinary held-use input must buffer across 20 Hz control samples,
    // then choose the same viable drink at the slow boundary after revalidation.
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut via_input,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let checkpoint = serde_json::to_vec(&SaveEnvelope::new(&registries, &via_input)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&checkpoint)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut started = false;
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let current = advance_control_tick(&registries, &mut via_input).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(current, restored);
        assert_eq!(via_input, replay);
        if matches!(
            current.secondary_action(),
            Some(SecondaryActionOutcome::DrinkingStarted(drink)) if drink.store() == stores[0]
        ) {
            started = true;
            break;
        }
    }
    assert!(
        started,
        "buffered use must start drinking on its next slow boundary"
    );
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
    assert_eq!(
        attempt_empty_hand_secondary_action(&registries, &mut off_boundary),
        SecondaryActionOutcome::DeferredUntilSimulationBoundary,
        "off-boundary admission preview must not make an impossible drink compete with gathering"
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
    // Buffered use replays across the slow boundary and never touches the
    // unspendable local water.
    let mut via_input = off_boundary;
    let conserved = calculate_matter_accounting(&via_input).unwrap().total();
    let direction = ViewDirection::new(1_000_000, 0, 0).unwrap();
    let queued = advance_control_tick_with_direct_input(
        &registries,
        &mut via_input,
        PlayerDirectControlInput::NEUTRAL
            .with_secondary(SecondaryControlIntent::use_active(direction)),
    )
    .unwrap();
    assert_eq!(
        queued.secondary_action(),
        Some(&SecondaryActionOutcome::QueuedUntilSimulationBoundary)
    );
    let saved = serde_json::to_vec(&SaveEnvelope::new(&registries, &via_input)).unwrap();
    let mut replay = serde_json::from_slice::<LoadedSaveEnvelope>(&saved)
        .unwrap()
        .into_state(&registries)
        .unwrap();
    let mut started = false;
    for _ in 0..crate::core::time::CONTROL_TICKS_PER_SIMULATION_TICK {
        let outcome = advance_control_tick(&registries, &mut via_input).unwrap();
        let restored = advance_control_tick(&registries, &mut replay).unwrap();
        assert_eq!(outcome, restored);
        assert_eq!(via_input, replay);
        if matches!(
            outcome.secondary_action(),
            Some(SecondaryActionOutcome::SurfaceGatheringStarted(work))
                if work.resource() == option.resource()
        ) {
            started = true;
            break;
        }
    }
    assert!(
        started,
        "buffered use must start gathering instead of impossible drinking"
    );
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
